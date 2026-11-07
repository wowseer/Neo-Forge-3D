package clash.forgebridge;

import java.io.File;
import java.io.IOException;
import java.net.InetAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.net.SocketTimeoutException;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.TimeUnit;

/**
 * Startet einen Spieltisch als eigenen Prozess und verbindet ihn mit der eingebetteten Brücke –
 * unabhängig vom Gastgeber (NeoForge, Forge-Desktop, eigene Programme).
 *
 * <p>Ablauf: lokalen Port öffnen (nur Loopback) → Tisch-Prozess mit {@code -- --clash-port=N}
 * starten (Godot: {@code <godot> --path <projekt> -- --clash-port=N}) → Verbindung annehmen →
 * {@link ClashBridge#attach}. Wird das Fenster des Tisches während der Partie geschlossen, geben
 * die vom Tisch gesteuerten Spieler auf ({@link ClashBridge#concedeRemotePlayers}).
 *
 * <p>Ein anderer Tisch (nicht Godot) muss nur {@code --clash-port=N} verstehen, sich mit
 * {@code 127.0.0.1:N} verbinden und das Protokoll sprechen ({@code docs/Forge-Protocol.md}).
 */
public final class TableLauncher {
    private TableLauncher() {}

    /**
     * Wie der Tisch gestartet wird.
     *
     * @param program   ausführbares Programm (z. B. der Godot-Pfad oder ein exportierter Tisch)
     * @param arguments Argumente vor {@code --} (bei Godot {@code --path <projekt>})
     * @param timeoutMs so lange wird auf die Verbindung gewartet (Start, Shader-Übersetzung)
     * @param log       Datei für die Ausgaben des Tisch-Prozesses ({@code null} = verwerfen)
     * @param window    wo sich das Fenster öffnen soll (Fensterübergabe, als Godot-Argumente
     *                  {@link TableWindow#godotArgs()}); {@code null} = wo der Tisch will
     */
    public record Config(String program, List<String> arguments, int timeoutMs, File log, TableWindow window) {
        /** Ohne Fensterlage. */
        public Config(String program, List<String> arguments, int timeoutMs, File log) {
            this(program, arguments, timeoutMs, log, null);
        }

        /** Godot-Tisch: {@code godot --path project}, 120 s Wartezeit, Log im Temp-Ordner. */
        public Config(String godot, String project) {
            this(godot, List.of("--path", project), 120_000,
                    new File(System.getProperty("java.io.tmpdir"), "clash-table.log"), null);
        }

        /** Dieselbe Konfiguration mit Fensterlage {@code w}. */
        public Config withWindow(TableWindow w) {
            return new Config(program, arguments, timeoutMs, log, w);
        }
    }

    /**
     * Tisch starten und verbinden. Wirft {@link IOException}, wenn der Prozess nicht startet oder
     * sich nicht rechtzeitig verbindet (der Prozess ist dann beendet).
     */
    public static Session launch(Config config) throws IOException {
        ServerSocket server = new ServerSocket(0, 1, InetAddress.getLoopbackAddress());
        Process process = null;
        try {
            server.setSoTimeout(config.timeoutMs());
            List<String> cmd = new ArrayList<>();
            cmd.add(config.program());
            cmd.addAll(config.arguments());
            if (config.window() != null) {
                cmd.addAll(config.window().godotArgs());
            }
            cmd.add("--");
            cmd.add("--clash-port=" + server.getLocalPort());
            ProcessBuilder pb = new ProcessBuilder(cmd).redirectErrorStream(true);
            pb.redirectOutput(config.log() == null ? ProcessBuilder.Redirect.DISCARD : ProcessBuilder.Redirect.to(config.log()));
            process = pb.start();
            Socket socket;
            try {
                socket = server.accept();
            } catch (SocketTimeoutException e) {
                throw new IOException("Tisch hat sich nicht verbunden (" + config.timeoutMs() + " ms)", e);
            }
            Session session = new Session(socket, process, server.getLocalPort());
            ClashBridge.attach(socket.getOutputStream(), socket.getInputStream(), session::onTableClosed);
            return session;
        } catch (IOException | RuntimeException e) {
            if (process != null) {
                process.destroy();
            }
            throw e;
        } finally {
            server.close();
        }
    }

    /**
     * Ist ein Ordner ein Godot-Projekt ({@code project.godot} vorhanden)? Hilfe für Gastgeber, die
     * den Pfad aus Einstellungen lesen.
     */
    public static boolean isGodotProject(String dir) {
        return dir != null && !dir.isBlank() && new File(dir, "project.godot").isFile();
    }

    /** Verbindung zu einem laufenden Tisch. */
    public static final class Session implements AutoCloseable {
        private final Socket socket;
        private final Process process;
        private final int port;
        private volatile boolean closing;

        private Session(Socket socket, Process process, int port) {
            this.socket = socket;
            this.process = process;
            this.port = port;
        }

        /** Lokaler Port der Verbindung. */
        public int port() {
            return port;
        }

        /** Läuft der Tisch-Prozess noch? */
        public boolean isAlive() {
            return process.isAlive();
        }

        /**
         * Zuletzt gemeldete Lage des Tisch-Fensters ({@link ClashBridge#tableWindow()}), z. B. um
         * das Fenster des Gastgebers nach der Partie dorthin zu legen; {@code null} = keine.
         */
        public TableWindow window() {
            return ClashBridge.tableWindow();
        }

        private void onTableClosed() {
            if (!closing) {
                ClashBridge.concedeRemotePlayers();
            }
        }

        /** Partie vorbei: Verbindung schließen (der Tisch beendet sich dann selbst, sonst nach 8 s hart). */
        @Override
        public void close() {
            closing = true;
            try {
                socket.close();
            } catch (IOException ignored) {
                // schon zu
            }
            try {
                if (!process.waitFor(8, TimeUnit.SECONDS)) {
                    process.destroy();
                }
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
                process.destroy();
            }
        }
    }
}
