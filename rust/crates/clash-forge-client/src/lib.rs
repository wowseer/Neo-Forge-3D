//! # clash-forge-client
//!
//! Verbindung zur Forge-Brücke von Neo Forge 3D, ohne Godot: entweder die Brücke als eigenen
//! Java-Prozess starten ([`ForgeClient::spawn`], JSON Lines über stdin/stdout) oder sich mit einem
//! Gastgeber verbinden, der Forge schon laufen hat und den Tisch mit `--clash-port=N` gestartet
//! hat ([`ForgeClient::connect`], TCP `127.0.0.1:N`, z. B. NeoForge).
//!
//! Ein Lesethread liest die Nachrichten der Brücke zeilenweise; die Oberfläche holt sie ohne zu
//! blockieren mit [`ForgeClient::poll`] (z. B. einmal je Frame) ab. Befehle baut
//! [`clash_protocol::command`]; [`ForgeClient`] setzt [`clash_protocol::RulesEngine`] um.
//!
//! ```no_run
//! use clash_forge_client::{ForgeClient, Incoming, LaunchConfig};
//! use clash_protocol::{command, message_type, Request, RulesEngine, Choice, Action};
//!
//! let cfg = LaunchConfig::new("/pfad/zu/forge", "/pfad/zu/forge-bridge.jar");
//! let mut client = ForgeClient::spawn(&cfg).unwrap();
//! loop {
//!     match client.recv() {
//!         Some(Incoming::Message(m)) if message_type(&m) == "ready" => {
//!             let start = command::StartMatch { decks: vec!["a.dck".into(), "b.dck".into()], ..Default::default() };
//!             client.send(&start.to_json()).unwrap();
//!         }
//!         Some(Incoming::Message(m)) if message_type(&m) == "request" => {
//!             let req = Request::from_json(&m).unwrap();
//!             client.submit(req.id, &Choice::Action(Action::Pass));
//!         }
//!         Some(Incoming::Message(_)) | Some(Incoming::Invalid(_)) => {}
//!         Some(Incoming::Closed) | None => break,
//!     }
//! }
//! ```

use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::net::{Shutdown, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, TryRecvError};
use std::thread;
use std::time::Duration;

use clash_protocol::{command, Choice, PlayerId, RulesEngine, Value};

/// Hauptklasse der eigenständigen Brücke (`forge-bridge.jar`).
pub const BRIDGE_MAIN_CLASS: &str = "clash.forgebridge.app.BridgeMain";
/// Programmargument, mit dem ein Gastgeber den Tisch startet (`--clash-port=N`).
pub const HOST_PORT_ARG: &str = "--clash-port=";

/// Was der Lesethread liefert.
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    /// Eine Nachricht der Brücke (JSON-Objekt, Feld `type`).
    Message(Value),
    /// Eine Zeile, die kein JSON war.
    Invalid(String),
    /// Verbindung zu (Prozess beendet bzw. Gastgeber hat geschlossen).
    Closed,
}

/// Fehler beim Starten bzw. Verbinden.
#[derive(Debug)]
pub enum ClientError {
    /// Kein Forge-Jar gefunden (Ordner bzw. Datei).
    ForgeJarNotFound(PathBuf),
    /// Brücken-Jar fehlt (`forge-bridge/build.sh` ausführen).
    BridgeJarNotFound(PathBuf),
    /// Java-Prozess startet nicht.
    Spawn(io::Error),
    /// Gastgeber nicht erreichbar.
    Connect(io::Error),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::ForgeJarNotFound(p) => write!(f, "Kein Forge-Jar in {}", p.display()),
            ClientError::BridgeJarNotFound(p) => write!(f, "{} fehlt – forge-bridge/build.sh ausführen", p.display()),
            ClientError::Spawn(e) => write!(f, "Java-Prozess startet nicht: {e}"),
            ClientError::Connect(e) => write!(f, "Gastgeber nicht erreichbar: {e}"),
        }
    }
}

impl std::error::Error for ClientError {}

impl ClientError {
    /// Kurzer Code für Meldungen (`forge_jar_not_found`, `bridge_jar_not_found`, `spawn_failed`, `connect_failed`).
    pub fn code(&self) -> &'static str {
        match self {
            ClientError::ForgeJarNotFound(_) => "forge_jar_not_found",
            ClientError::BridgeJarNotFound(_) => "bridge_jar_not_found",
            ClientError::Spawn(_) => "spawn_failed",
            ClientError::Connect(_) => "connect_failed",
        }
    }
}

/// Speicher-Einstellungen der JVM: G1 gibt ungenutzten Heap zurück, statt ihn zu behalten
/// (`MaxHeapFreeRatio`), und sammelt auch in Ruhe regelmäßig (`G1PeriodicGCInterval`, 10 s).
/// Forge hält während einer Partie nur rund 200 MB lebende Daten, ohne diese Einstellungen
/// belegte die Brücke trotzdem rund 1,2 GB, mit ihnen rund 0,7 GB (gleiches `-Xmx`).
pub const JVM_MEMORY_ARGS: [&str; 4] =
    ["-XX:+UseG1GC", "-XX:MinHeapFreeRatio=10", "-XX:MaxHeapFreeRatio=40", "-XX:G1PeriodicGCInterval=10000"];

/// Wie die eigenständige Brücke gestartet wird.
#[derive(Debug, Clone, PartialEq)]
pub struct LaunchConfig {
    /// Java-Programm (Name im PATH oder Pfad), Java 17 oder neuer.
    pub java: String,
    /// Forge-Installationsordner (enthält `res/`); Arbeitsverzeichnis des Prozesses.
    pub forge_dir: PathBuf,
    /// Forge-Jar mit allen Abhängigkeiten; `None` = neueste
    /// `forge-gui-desktop-*-jar-with-dependencies.jar` in `forge_dir`.
    pub forge_jar: Option<PathBuf>,
    /// Gebaute Brücke (`forge-bridge/build/forge-bridge.jar`).
    pub bridge_jar: PathBuf,
    /// Maximaler Java-Heap in MB (`-Xmx`).
    pub max_memory_mb: u32,
    /// Datei für stderr (Forge-Logausgaben); `None` = verwerfen.
    pub stderr_log: Option<PathBuf>,
}

impl LaunchConfig {
    /// Standardwerte: `java` aus dem PATH, Forge-Jar automatisch, 2 GB Heap, stderr verwerfen.
    pub fn new(forge_dir: impl Into<PathBuf>, bridge_jar: impl Into<PathBuf>) -> Self {
        Self {
            java: "java".into(),
            forge_dir: forge_dir.into(),
            forge_jar: None,
            bridge_jar: bridge_jar.into(),
            max_memory_mb: 2048,
            stderr_log: None,
        }
    }

    /// Forge-Jar: angegeben (falls vorhanden) bzw. das neueste im Forge-Ordner.
    pub fn resolve_forge_jar(&self) -> Option<PathBuf> {
        match &self.forge_jar {
            Some(p) => p.is_file().then(|| p.clone()),
            None => find_forge_jar(&self.forge_dir),
        }
    }
}

/// Neueste `forge-gui-desktop-*-jar-with-dependencies.jar` in `dir`.
pub fn find_forge_jar(dir: &Path) -> Option<PathBuf> {
    let mut jars: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("forge-gui-desktop-") && n.ends_with("-jar-with-dependencies.jar"))
        })
        .collect();
    jars.sort();
    jars.pop()
}

/// Port des Gastgebers aus Programmargumenten (`--clash-port=N`), falls der Tisch von einem
/// Gastgeber wie NeoForge gestartet wurde.
pub fn host_port<I: IntoIterator<Item = S>, S: AsRef<str>>(args: I) -> Option<u16> {
    args.into_iter().find_map(|a| a.as_ref().strip_prefix(HOST_PORT_ARG).and_then(|p| p.parse().ok()))
}

enum Link {
    Process { child: Child, stdin: Option<ChildStdin> },
    Host(TcpStream),
}

/// Verbindung zur Brücke (eigener Java-Prozess oder Gastgeber).
pub struct ForgeClient {
    link: Link,
    rx: Receiver<Incoming>,
    reader: Option<thread::JoinHandle<()>>,
}

impl ForgeClient {
    /// Startet die Brücke als eigenen Java-Prozess. Fertig ist sie mit der Nachricht `ready`
    /// (Forge braucht einige Sekunden zum Laden).
    pub fn spawn(cfg: &LaunchConfig) -> Result<ForgeClient, ClientError> {
        let forge_jar = cfg.resolve_forge_jar().ok_or_else(|| ClientError::ForgeJarNotFound(cfg.forge_jar.clone().unwrap_or_else(|| cfg.forge_dir.clone())))?;
        if !cfg.bridge_jar.is_file() {
            return Err(ClientError::BridgeJarNotFound(cfg.bridge_jar.clone()));
        }
        let stderr = cfg.stderr_log.as_ref().and_then(|p| File::create(p).ok()).map_or_else(Stdio::null, Stdio::from);
        // absolut: der Prozess läuft im Forge-Ordner, relative Pfade zeigten dann ins Leere
        let abs = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        let sep = if cfg!(windows) { ";" } else { ":" };
        let classpath = format!("{}{}{}", abs(&cfg.bridge_jar).display(), sep, abs(&forge_jar).display());
        let mut child = Command::new(&cfg.java)
            .arg(format!("-Xmx{}m", cfg.max_memory_mb))
            .args(JVM_MEMORY_ARGS)
            .arg("-Dfile.encoding=UTF-8")
            .arg("-Djava.awt.headless=true")
            .arg("-cp")
            .arg(classpath)
            .arg(BRIDGE_MAIN_CLASS)
            .current_dir(&cfg.forge_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(stderr)
            .spawn()
            .map_err(ClientError::Spawn)?;
        let stdout = child.stdout.take().expect("stdout ist gepiped");
        let stdin = child.stdin.take();
        let (rx, reader) = read_lines(stdout);
        Ok(ForgeClient { link: Link::Process { child, stdin }, rx, reader: Some(reader) })
    }

    /// Verbindet sich mit einem Gastgeber auf `127.0.0.1:port` (eingebetteter Betrieb: der
    /// Gastgeber startet die Partie selbst).
    pub fn connect(port: u16) -> Result<ForgeClient, ClientError> {
        let stream = TcpStream::connect(("127.0.0.1", port)).map_err(ClientError::Connect)?;
        let reader = stream.try_clone().map_err(ClientError::Connect)?;
        let (rx, handle) = read_lines(reader);
        Ok(ForgeClient { link: Link::Host(stream), rx, reader: Some(handle) })
    }

    /// Läuft die Brücke eingebettet in einem Gastgeber?
    pub fn is_hosted(&self) -> bool {
        matches!(self.link, Link::Host(_))
    }

    /// Schickt einen Befehl (JSON-Objekt mit `cmd`, siehe [`clash_protocol::command`]).
    pub fn send(&mut self, cmd: &Value) -> io::Result<()> {
        let line = cmd.to_string();
        match &mut self.link {
            Link::Host(s) => writeln!(s, "{line}").and_then(|_| s.flush()),
            Link::Process { stdin: Some(s), .. } => writeln!(s, "{line}").and_then(|_| s.flush()),
            Link::Process { stdin: None, .. } => Err(io::Error::new(io::ErrorKind::NotConnected, "Brücke beendet")),
        }
    }

    /// Nächste Nachricht, ohne zu warten (`None` = gerade keine).
    pub fn poll(&mut self) -> Option<Incoming> {
        match self.rx.try_recv() {
            Ok(m) => Some(m),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Incoming::Closed),
        }
    }

    /// Nächste Nachricht, wartet höchstens `timeout`.
    pub fn recv_timeout(&mut self, timeout: Duration) -> Option<Incoming> {
        match self.rx.recv_timeout(timeout) {
            Ok(m) => Some(m),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => Some(Incoming::Closed),
        }
    }

    /// Nächste Nachricht, wartet bis eine kommt (`None` nur, wenn der Lesethread weg ist).
    pub fn recv(&mut self) -> Option<Incoming> {
        self.rx.recv().ok()
    }

    /// Nach [`Incoming::Closed`]: Exit-Code des Java-Prozesses (`None` bei Gastgeber/unbekannt).
    pub fn exit_code(&mut self) -> Option<i32> {
        match &mut self.link {
            Link::Process { child, .. } => child.wait().ok().and_then(|s| s.code()),
            Link::Host(_) => None,
        }
    }

    /// Beendet die Verbindung: Befehl `quit`, Prozess nach 1 s hart beenden bzw. Socket
    /// schließen; wartet auf das Ende des Lesethreads.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        let _ = self.send(&command::quit());
        match &mut self.link {
            Link::Host(s) => {
                let _ = s.shutdown(Shutdown::Both);
            }
            Link::Process { child, stdin } => {
                stdin.take();
                for _ in 0..20 {
                    if matches!(child.try_wait(), Ok(Some(_))) {
                        break;
                    }
                    thread::sleep(Duration::from_millis(50));
                }
                if !matches!(child.try_wait(), Ok(Some(_))) {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        }
        if let Some(r) = self.reader.take() {
            let _ = r.join();
        }
    }
}

impl Drop for ForgeClient {
    fn drop(&mut self) {
        if self.reader.is_some() {
            self.shutdown();
        }
    }
}

/// Die Brücke als Regel-Engine einer Oberfläche.
impl RulesEngine for ForgeClient {
    fn submit(&mut self, request_id: i64, choice: &Choice) -> bool {
        self.send(&command::answer(request_id, choice)).is_ok()
    }

    fn request_state(&mut self, viewer: PlayerId) -> bool {
        self.send(&command::request_state(Some(viewer))).is_ok()
    }
}

/// Lesethread: Zeilen von `src` als [`Incoming`], am Ende [`Incoming::Closed`].
fn read_lines<R: io::Read + Send + 'static>(src: R) -> (Receiver<Incoming>, thread::JoinHandle<()>) {
    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        for line in BufReader::new(src).lines() {
            let Ok(line) = line else { break };
            if line.trim().is_empty() {
                continue;
            }
            let item = match serde_json::from_str::<Value>(&line) {
                Ok(v) if v.is_object() => Incoming::Message(v),
                _ => Incoming::Invalid(line),
            };
            if tx.send(item).is_err() {
                return;
            }
        }
        let _ = tx.send(Incoming::Closed);
    });
    (rx, handle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_host_port() {
        assert_eq!(host_port(["--foo", "--clash-port=4711"]), Some(4711));
        assert_eq!(host_port(["--clash-port=x"]), None);
        assert_eq!(host_port(Vec::<String>::new()), None);
    }

    #[test]
    fn reads_lines_from_stream() {
        let data = b"{\"type\":\"ready\",\"protocol\":3}\nkein json\n".to_vec();
        let (rx, h) = read_lines(io::Cursor::new(data));
        h.join().unwrap();
        let got: Vec<Incoming> = rx.iter().collect();
        assert!(matches!(&got[0], Incoming::Message(m) if m["protocol"] == 3));
        assert_eq!(got[1], Incoming::Invalid("kein json".into()));
        assert_eq!(got[2], Incoming::Closed);
    }
}
