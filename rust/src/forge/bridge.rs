//! `ForgeBridge`: Godot-Knoten für die Forge-Brücke. Übersetzt das JSON-Lines-Protokoll in
//! Godot-Signale und -Methoden; Prozess bzw. Socket und Lesethread kommen aus dem godot-freien
//! Crate `clash-forge-client` ([`ForgeClient`]), die Befehle aus `clash-protocol`.
//!
//! - Eigenständig: startet die Java-Brücke (`forge-bridge/`) als eigenen Prozess; stderr
//!   (Forge-Logausgaben) landet in `user://forge-bridge.log`.
//! - Eingebettet (Godot von einem Gastgeber wie NeoForge gestartet mit `-- --clash-port=N`):
//!   statt des Java-Prozesses eine TCP-Verbindung zu `127.0.0.1:N`, gleiches Protokoll; die
//!   Partie startet der Gastgeber. Für die Fensterübergabe meldet der Knoten dabei die Lage seines
//!   Fensters (Befehl `window`), damit der Gastgeber nach der Partie dort wieder erscheint.

use std::path::{Path, PathBuf};

use clash_forge_client::{ForgeClient, Incoming, LaunchConfig};
use clash_protocol::{command, Value};
use godot::classes::display_server::WindowMode;
use godot::classes::{DisplayServer, INode, Node, ProjectSettings};
use godot::prelude::*;

use crate::rules::json;
use crate::rules::{Choice, PlayerId, RulesEngine};

/// Höchstens so viele Nachrichten werden pro Frame verarbeitet (Forge sendet in Spitzen
/// Hunderte Ereignisse auf einmal).
const MAX_MESSAGES_PER_FRAME: usize = 400;
/// Eingebettet: so lange (Sekunden) bleibt der Tisch nach dem Ende der Verbindung offen.
const HOSTED_CLOSE_DELAY: f64 = 4.0;

/// Verbindung zur Forge-Regel-Engine: startet die Java-Brücke (`forge-bridge/`) als eigenen
/// Prozess bzw. verbindet sich mit einem Gastgeber und spricht mit ihr über JSON-Zeilen
/// (Protokoll: `docs/Forge-Protocol.md`).
///
/// Ablauf: [method start_bridge] → Signal [signal bridge_ready] → [method start_match] →
/// Signale [signal forge_event] (jedes Spielereignis) und [signal decision_requested]
/// (ferngesteuerter Spieler hat Priorität / Mulligan) → Antwort mit [method answer] →
/// [signal game_over]. Ferngesteuert sind die Sitzplätze aus `remote_seats`; alle anderen
/// spielt die Forge-KI.
///
/// Zahlen in empfangenen Dictionaries sind ganzzahlig (int), wenn sie keinen Nachkommaanteil
/// haben.
#[derive(GodotClass)]
#[class(base=Node)]
pub struct ForgeBridge {
    /// Java-Programm (Name im PATH oder absoluter Pfad). Java 17 oder neuer.
    #[export]
    java_path: GString,
    /// Forge-Installationsordner (enthält `res/`). Leer = Umgebungsvariable `FORGE_DIR`,
    /// sonst die Projektkopie `forge-bridge/forge/` (falls vorhanden), sonst `~/Games/Forge`.
    #[export(global_dir)]
    forge_dir: GString,
    /// Forge-Jar mit allen Abhängigkeiten. Leer = erste Datei
    /// `forge-gui-desktop-*-jar-with-dependencies.jar` in [member forge_dir].
    #[export(global_file = "*.jar")]
    forge_jar: GString,
    /// Gebaute Brücke (`forge-bridge/build.sh`). `res://`-Pfade werden umgerechnet.
    #[export]
    bridge_jar: GString,
    /// Maximaler Java-Heap in MB (`-Xmx`).
    #[export(range = (256.0, 16384.0))]
    java_max_memory_mb: i32,
    /// Tempo der Forge-KI: Faktor für die Pausen nach ihren Aktionen (Land, Zauber, Angriff,
    /// Blocker, Phasenwechsel), damit man ihrem Zug zusehen kann. 1 = normal, 2 = doppelt so
    /// langsam, 0 = ohne Pausen. Wird bei [method start_match] an die Brücke geschickt.
    #[export(range = (0.0, 4.0))]
    ai_pace: f64,
    /// Spielart für [method start_match]: leer = Constructed, `Commander` = Commander-Partie
    /// (Decks mit Abschnitt `[Commander]`, CR 903). Eingebettet in NeoForge legt der Gastgeber
    /// die Spielart fest.
    #[export]
    game_type: GString,
    /// Brücke automatisch in `ready()` starten.
    #[export]
    autostart: bool,
    /// Jede empfangene Nachricht in die Godot-Konsole schreiben (zum Debuggen).
    #[export]
    log_messages: bool,

    /// Verbindung (Java-Prozess oder Gastgeber), None = keine.
    client: Option<ForgeClient>,
    ready: bool,
    /// Zuletzt gemeldete Fensterlage (eingebettet, Fensterübergabe) und Zeit bis zur nächsten Prüfung.
    window_sent: Option<(i32, i32, i32, i32, &'static str)>,
    window_check: f64,
    base: Base<Node>,
}

#[godot_api]
impl INode for ForgeBridge {
    fn init(base: Base<Node>) -> Self {
        Self {
            java_path: "java".into(),
            forge_dir: GString::new(),
            forge_jar: GString::new(),
            bridge_jar: "res://../forge-bridge/build/forge-bridge.jar".into(),
            java_max_memory_mb: 2048,
            ai_pace: 1.0,
            game_type: GString::new(),
            autostart: false,
            log_messages: false,
            client: None,
            ready: false,
            window_sent: None,
            window_check: 0.0,
            base,
        }
    }

    fn ready(&mut self) {
        if self.autostart {
            self.start_bridge();
        }
    }

    fn process(&mut self, delta: f64) {
        self.report_window(delta);
        let mut messages = Vec::new();
        let mut closed = false;
        if let Some(client) = self.client.as_mut() {
            while messages.len() < MAX_MESSAGES_PER_FRAME {
                match client.poll() {
                    Some(Incoming::Closed) => {
                        closed = true;
                        break;
                    }
                    Some(m) => messages.push(m),
                    None => break,
                }
            }
        }
        if !messages.is_empty() {
            // Nachrichten der Engine lösen Animationen aus: volle Bildrate
            crate::table::frame_pacer::wake();
        }
        for m in messages {
            match m {
                Incoming::Message(v) => self.dispatch(&v),
                Incoming::Invalid(line) => self.emit_error("bad_message", &line),
                Incoming::Closed => {}
            }
        }
        if closed {
            self.on_closed();
        }
    }

    fn exit_tree(&mut self) {
        self.stop_bridge();
    }
}

#[godot_api]
impl ForgeBridge {
    /// Die Brücke ist gestartet und Forge geladen; `protocol` = Protokollversion.
    #[signal]
    fn bridge_ready(protocol: i64);

    /// Jede empfangene Nachricht (vor den speziellen Signalen), z. B. für eigene Auswertung.
    #[signal]
    fn message_received(message: VarDictionary);

    /// Ein Forge-Spielereignis: `{event, text, turn, phase}` (`event` = Forge-Klassenname,
    /// z. B. "GameEventCardChangeZone").
    #[signal]
    fn forge_event(event: VarDictionary);

    /// Neuer Eintrag im Spielprotokoll der Engine: `{kind, text, card, turn}` (`kind` z. B.
    /// "ZONE_CHANGE", "DAMAGE", "STACK_ADD", "TURN"; `card` = Karten-ID der Quelle, falls bekannt).
    #[signal]
    fn log_entry(entry: VarDictionary);

    /// Ein ferngesteuerter Spieler muss entscheiden (Protokoll v3): `{id, player, decision, prompt,
    /// source, ability, cancelable, labels, state}`; `decision` ist eine `Decision`
    /// (z. B. `{"type": "Priority", "actions": […]}`). Antwort mit [method answer] und einer
    /// `Choice`. Aufbau siehe `docs/Forge-Protocol.md`.
    #[signal]
    fn decision_requested(request: VarDictionary);

    /// Die gewählte Option wurde von Forge abgelehnt (`{player, option, reason}`);
    /// es folgt eine neue Anfrage.
    #[signal]
    fn decision_rejected(info: VarDictionary);

    /// Die Partie hat begonnen; `state` = Spielzustand (siehe Protokoll).
    #[signal]
    fn match_started(state: VarDictionary);

    /// Antwort auf [method request_state].
    #[signal]
    fn state_received(state: VarDictionary);

    /// Antwort auf [method debug_request_card_names]: alle Kartennamen der Kartendatenbank,
    /// sortiert (Debug-Fenster).
    #[signal]
    fn debug_card_names_received(names: PackedStringArray);

    /// Antwort auf [method request_card_info]: Kartendaten (`{name, type, cost, text, power,
    /// toughness, image}`) je bekanntem Namen.
    #[signal]
    fn card_info_received(cards: VarArray);

    /// Die Partie ist vorbei: `{winner, draw, reason}`.
    #[signal]
    fn game_over(result: VarDictionary);

    /// Aussehen eines Sitzplatzes vom Gastgeber (eingebettet, z. B. NeoForges „Anpassen“):
    /// `{seat, portrait, sleeve}` mit Pfaden zu Bilddateien (fehlend = eigenes Aussehen).
    #[signal]
    fn look_received(look: VarDictionary);

    /// Fehler der Brücke oder beim Start (`code` z. B. "deck_not_found", "spawn_failed").
    #[signal]
    fn bridge_error(code: GString, message: GString);

    /// Der Java-Prozess ist beendet (`exit_code` -1 = unbekannt / von uns beendet).
    #[signal]
    fn bridge_stopped(exit_code: i64);

    /// Startet den Java-Prozess. Gibt false zurück, wenn er schon läuft oder nicht starten
    /// konnte (dann zusätzlich [signal bridge_error]). Fertig ist er erst mit
    /// [signal bridge_ready] (Forge braucht einige Sekunden zum Laden).
    #[func]
    pub fn start_bridge(&mut self) -> bool {
        if self.client.is_some() {
            return false;
        }
        // Gestartet von einem Gastgeber (NeoForge) mit `-- --clash-port=N`: dorthin verbinden
        let result = if let Some(port) = host_port() {
            ForgeClient::connect(port)
        } else {
            let mut cfg = LaunchConfig::new(self.resolved_forge_dir(), globalize(&self.bridge_jar));
            cfg.java = self.java_path.to_string();
            cfg.forge_jar = (!self.forge_jar.is_empty()).then(|| PathBuf::from(globalize(&self.forge_jar)));
            cfg.max_memory_mb = self.java_max_memory_mb.max(256) as u32;
            cfg.stderr_log = Some(PathBuf::from(globalize(&GString::from("user://forge-bridge.log"))));
            ForgeClient::spawn(&cfg)
        };
        match result {
            Ok(c) => {
                self.client = Some(c);
                self.ready = false;
                true
            }
            Err(e) => {
                self.emit_error(e.code(), &e.to_string());
                false
            }
        }
    }

    /// Läuft die Brücke eingebettet in einem Gastgeber (NeoForge startet die Partie)?
    #[func]
    pub fn is_hosted(&self) -> bool {
        self.client.as_ref().is_some_and(ForgeClient::is_hosted) || host_port().is_some()
    }

    /// Beendet die Brücke (Befehl "quit", nach 1 s hart beendet). Sendet
    /// [signal bridge_stopped] mit -1, falls sie lief.
    #[func]
    fn stop_bridge(&mut self) {
        // im Hauptthread warten: ein losgelöster Warte-Thread liefe beim Beenden von Godot noch,
        // wenn die Erweiterung schon entladen ist (Absturz)
        let Some(client) = self.client.take() else { return };
        client.stop();
        self.ready = false;
        self.signals().bridge_stopped().emit(-1);
    }

    /// Läuft der Java-Prozess bzw. die Verbindung zum Gastgeber?
    #[func]
    pub fn is_running(&self) -> bool {
        self.client.is_some()
    }

    /// Hat Forge fertig geladen ([signal bridge_ready] empfangen)?
    #[func]
    fn is_ready(&self) -> bool {
        self.ready
    }

    /// Schickt einen beliebigen Befehl (Dictionary mit "cmd", siehe Protokoll).
    /// false, wenn die Brücke nicht läuft.
    #[func]
    fn send_command(&mut self, command: VarDictionary) -> bool {
        self.send_value(&json::from_variant(&command.to_variant()))
    }

    /// Verbindungstest; Antwort kommt als Nachricht `{type: "pong", nonce}`.
    #[func]
    fn ping(&mut self, nonce: i64) -> bool {
        self.send_value(&command::ping(nonce))
    }

    /// Startet eine Partie. `decks`: Pfade zu Forge-`.dck`-Dateien (`res://` erlaubt) oder
    /// Namen von Decks aus Forges Constructed-Sammlung, einer je Sitzplatz (mind. 2).
    /// `remote_seats`: Sitzplätze, die Neo Forge 3D steuert (Rest = Forge-KI).
    /// `names`: Spielernamen (darf kürzer sein). `seed` < 0 = zufällig.
    #[func]
    pub fn start_match(
        &mut self,
        decks: PackedStringArray,
        remote_seats: PackedInt32Array,
        names: PackedStringArray,
        seed: i64,
    ) -> bool {
        let start = command::StartMatch {
            decks: decks
                .as_slice()
                .iter()
                .map(|d| {
                    let s = d.to_string();
                    if s.starts_with("res://") || s.starts_with("user://") { globalize(d) } else { s }
                })
                .collect(),
            remote: remote_seats.as_slice().iter().filter_map(|s| PlayerId::try_from(*s).ok()).collect(),
            names: names.as_slice().iter().map(|n| n.to_string()).collect(),
            seed: (seed >= 0).then_some(seed),
            ai_pace: self.ai_pace,
            game_type: (!self.game_type.is_empty()).then(|| self.game_type.to_string()),
        };
        self.send_value(&start.to_json())
    }

    /// Beantwortet eine Anfrage aus [signal decision_requested]. `choice` ist eine Antwort im
    /// Format des Protokoll-Modells (Protokoll v3), z. B. `{"type": "Action", "action": {"type": "Pass"}}`;
    /// Aufbau siehe `docs/Forge-Protocol.md`.
    #[func]
    pub fn answer(&mut self, request_id: i64, choice: Variant) -> bool {
        self.send_value(&command::answer_json(request_id, json::from_variant(&choice)))
    }

    /// Fragt den aktuellen Spielzustand ab → [signal state_received]. `viewer` = Sitzplatz,
    /// dessen Handkarten mitgeschickt werden (-1 = keiner).
    #[func]
    pub fn request_state(&mut self, viewer: i64) -> bool {
        self.send_value(&command::request_state(PlayerId::try_from(viewer).ok()))
    }

    /// Kartendaten zu Kartennamen anfordern → [signal card_info_received] (z. B. für Treffer
    /// bei „Kartennamen nennen“).
    #[func]
    pub fn request_card_info(&mut self, names: PackedStringArray) -> bool {
        let names: Vec<String> = names.as_slice().iter().map(|n| n.to_string()).collect();
        self.send_value(&command::card_info(&names))
    }

    /// Debug: alle Kartennamen anfordern → [signal debug_card_names_received].
    #[func]
    pub fn debug_request_card_names(&mut self) -> bool {
        self.send_value(&command::debug_card_names())
    }

    /// Debug: Karte `name` neu erzeugen und Sitzplatz `seat` auf die Hand legen. Danach kommt
    /// der neue Zustand; eine offene Priorität wird neu gestellt.
    #[func]
    pub fn debug_add_card(&mut self, seat: i64, name: GString) -> bool {
        self.send_value(&command::debug_add_card(seat.clamp(0, 255) as PlayerId, &name.to_string()))
    }

    /// Debug: unendliches Mana für Sitzplatz `seat` an/aus (Pool wird bei jedem Phasenwechsel
    /// und Zauber auf 50 je Manaart aufgefüllt).
    #[func]
    pub fn debug_infinite_mana(&mut self, seat: i64, on: bool) -> bool {
        self.send_value(&command::debug_infinite_mana(seat.clamp(0, 255) as PlayerId, on))
    }
}

impl ForgeBridge {
    /// Befehl senden; false (und [signal bridge_error]), wenn die Brücke nicht läuft bzw. das
    /// Schreiben scheitert.
    fn send_value(&mut self, cmd: &Value) -> bool {
        let Some(client) = self.client.as_mut() else { return false };
        let ok = client.send(cmd).is_ok();
        if !ok {
            self.emit_error("write_failed", "Befehl konnte nicht gesendet werden");
        }
        ok
    }

    fn resolved_forge_dir(&self) -> PathBuf {
        if !self.forge_dir.is_empty() {
            return PathBuf::from(globalize(&self.forge_dir));
        }
        if let Ok(d) = std::env::var("FORGE_DIR") {
            return PathBuf::from(d);
        }
        let local = PathBuf::from(globalize(&GString::from("res://../forge-bridge/forge")));
        if local.join("res").is_dir() {
            return local;
        }
        let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_default();
        Path::new(&home).join("Games").join("Forge")
    }

    fn emit_error(&mut self, code: &str, message: &str) {
        godot_warn!("ForgeBridge: {code}: {message}");
        self.signals().bridge_error().emit(&GString::from(code), &GString::from(message));
    }

    fn dispatch(&mut self, msg: &Value) {
        let Some(obj) = msg.as_object() else { return };
        let dict = json::to_dictionary(obj);
        if self.log_messages {
            godot_print!("ForgeBridge <- {msg}");
        }
        self.signals().message_received().emit(&dict);
        // Teilobjekte aus dem schon umgewandelten Wörterbuch nehmen (der Zustand ist die größte
        // Nachricht und wurde früher ein zweites Mal umgewandelt)
        let sub = |key: &str| dict.get(key).and_then(|v| v.try_to::<VarDictionary>().ok()).unwrap_or_default();
        match clash_protocol::message_type(msg) {
            "ready" => {
                self.ready = true;
                let protocol = msg.get("protocol").and_then(Value::as_i64).unwrap_or(0);
                self.signals().bridge_ready().emit(protocol);
            }
            "event" => self.signals().forge_event().emit(&dict),
            "log" => self.signals().log_entry().emit(&dict),
            "request" => self.signals().decision_requested().emit(&dict),
            "rejected" => self.signals().decision_rejected().emit(&dict),
            "match_started" => self.signals().match_started().emit(&sub("state")),
            "state" => self.signals().state_received().emit(&sub("state")),
            "game_over" => self.signals().game_over().emit(&dict),
            "look" => self.signals().look_received().emit(&dict),
            "card_info" => {
                let cards = msg.get("cards").map(json::to_variant).and_then(|v| v.try_to::<VarArray>().ok()).unwrap_or_default();
                self.signals().card_info_received().emit(&cards);
            }
            "debug_card_names" => {
                let names: PackedStringArray = msg
                    .get("names")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().map(|v| GString::from(v.as_str().unwrap_or_default())).collect())
                    .unwrap_or_default();
                self.signals().debug_card_names_received().emit(&names);
            }
            "error" => {
                let s = |k: &str| msg.get(k).map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_string)).unwrap_or_default();
                let (code, text) = (s("code"), s("message"));
                self.emit_error(&code, &text);
            }
            _ => {}
        }
    }

    fn on_closed(&mut self) {
        self.ready = false;
        let Some(mut client) = self.client.take() else { return };
        if client.is_hosted() {
            // Eingebettet: der Gastgeber hat die Partie beendet → Spieltisch nach kurzer Zeit
            // schließen (die Spielende-Meldung bleibt so lange sichtbar)
            drop(client);
            self.signals().bridge_stopped().emit(0);
            let mut tree = self.base().get_tree();
            let quit = Callable::from_object_method(&tree, "quit");
            tree.create_timer(HOSTED_CLOSE_DELAY).connect("timeout", &quit);
            return;
        }
        let code = client.exit_code().map_or(-1, i64::from);
        drop(client);
        self.signals().bridge_stopped().emit(code);
    }
}

/// `res://`/`user://` → absoluter Pfad (andere Pfade unverändert).
impl ForgeBridge {
    /// Eingebettet (Gastgeber wie NeoForge): Lage des Fensters melden, sobald sie sich ändert
    /// (höchstens viermal je Sekunde), damit der Gastgeber nach der Partie sein Fenster dorthin legt
    /// (Fensterübergabe). Minimiert wird nichts gemeldet – zurück kommt die letzte sichtbare Lage.
    fn report_window(&mut self, delta: f64) {
        self.window_check -= delta;
        if self.window_check > 0.0 || !self.ready || !self.client.as_ref().is_some_and(ForgeClient::is_hosted) {
            return;
        }
        self.window_check = 0.25;
        let ds = DisplayServer::singleton();
        let mode = match ds.window_get_mode() {
            WindowMode::MINIMIZED => return,
            WindowMode::MAXIMIZED => "maximized",
            WindowMode::FULLSCREEN | WindowMode::EXCLUSIVE_FULLSCREEN => "fullscreen",
            _ => "windowed",
        };
        let (pos, size) = (ds.window_get_position(), ds.window_get_size());
        let now = (pos.x, pos.y, size.x, size.y, mode);
        if self.window_sent != Some(now) {
            self.window_sent = Some(now);
            self.send_value(&command::window(pos.x, pos.y, size.x, size.y, mode));
        }
    }
}

fn globalize(path: &GString) -> String {
    ProjectSettings::singleton().globalize_path(path).to_string()
}

/// Die Java-Brücke als Regel-Engine der UI (Protokoll v3).
impl RulesEngine for ForgeBridge {
    fn submit(&mut self, request_id: i64, choice: &Choice) -> bool {
        self.send_value(&command::answer(request_id, choice))
    }

    fn request_state(&mut self, viewer: PlayerId) -> bool {
        self.send_value(&command::request_state(Some(viewer)))
    }
}

/// Port des Gastgebers aus den Programmargumenten nach `--` (`--clash-port=N`), falls Godot von
/// einem Gastgeber wie NeoForge als Spieltisch gestartet wurde.
fn host_port() -> Option<u16> {
    let args = godot::classes::Os::singleton().get_cmdline_user_args();
    clash_forge_client::host_port(args.as_slice().iter().map(|a| a.to_string()))
}
