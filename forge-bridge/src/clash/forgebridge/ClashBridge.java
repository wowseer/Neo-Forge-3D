package clash.forgebridge;

import java.io.InputStream;
import java.io.OutputStream;

import clash.forgebridge.engine.BridgeCore;
import clash.forgebridge.engine.RemoteLobbyPlayer;
import forge.LobbyPlayer;
import forge.game.Game;
import forge.game.player.Player;

/**
 * Öffentliche Schnittstelle der Forge-Brücke – der einzige Einstieg, den Gastgeber und andere
 * Projekte brauchen. Alles in {@code clash.forgebridge.engine} ist intern und kann sich ändern.
 *
 * <p>Die Brücke übersetzt Forges {@code PlayerController}-Aufrufe eines Sitzplatzes in
 * {@code Decision}-Nachrichten (JSON Lines, Protokoll v{@value #PROTOCOL_VERSION}) und die
 * {@code Choice}-Antworten zurück; dazu schickt sie den Spielzustand, Ereignisse und das
 * Spielprotokoll. Das Protokoll steht in {@code docs/Forge-Protocol.md}.
 *
 * <h2>Zwei Betriebsarten</h2>
 * <ul>
 * <li><b>Eigenständig</b> ({@link clash.forgebridge.app.BridgeMain}): eigener Java-Prozess,
 * Nachrichten auf stdout, Befehle von stdin; der Tisch startet Partien mit {@code start_match}.</li>
 * <li><b>Eingebettet</b> ({@link #attach}): ein Gastgeber (z. B. NeoForge oder eine andere
 * Forge-Oberfläche) hat Forge schon geladen und startet die Partie selbst. Er setzt für den Sitz,
 * den der Tisch spielt, {@link #remotePlayer} ein und verbindet die Brücke mit einem Datenstrom –
 * am einfachsten über {@link TableLauncher}, der den Tisch-Prozess startet und den Socket öffnet.</li>
 * </ul>
 *
 * <h2>Einbau in einen Forge-Gastgeber (kurz)</h2>
 * <pre>{@code
 * TableLauncher.Session table = TableLauncher.launch(new TableLauncher.Config(godotExe, projectDir));
 * RegisteredPlayer human = ...;
 * human.setPlayer(ClashBridge.remotePlayer("Spieler", seatIndex));
 * // … Partie wie gewohnt über HostedMatch / Match starten …
 * table.close(); // nach der Partie
 * }</pre>
 */
public final class ClashBridge {
    /** Version des Protokolls (Nachricht {@code ready}). */
    public static final int PROTOCOL_VERSION = BridgeCore.PROTOCOL_VERSION;

    private ClashBridge() {}

    /**
     * Eingebetteter Betrieb: Nachrichten gehen auf {@code out}, Befehle kommen von {@code in}
     * (typischerweise ein lokaler Socket). Liest in einem eigenen Thread; ist die Verbindung zu,
     * werden offene Anfragen beendet (die Engine entscheidet dann selbst) und {@code onClose}
     * läuft. {@code start_match} ist eingebettet nicht erlaubt – die Partie startet der Gastgeber.
     */
    public static void attach(OutputStream out, InputStream in, Runnable onClose) {
        BridgeCore.attach(out, in, onClose);
    }

    /**
     * Lobby-Spieler für den Sitz, den der Tisch spielt: Forge erzeugt für ihn einen
     * Controller, der jede Entscheidung an den Tisch schickt. {@code seat} = Sitzplatz-Index
     * (Reihenfolge der Spieler in der Partie, im Protokoll {@code PlayerId}).
     */
    public static LobbyPlayer remotePlayer(String name, int seat) {
        return new RemoteLobbyPlayer(name, seat);
    }

    /** Wird {@code player} vom Tisch gesteuert (siehe {@link #remotePlayer})? */
    public static boolean isRemote(LobbyPlayer player) {
        return player instanceof RemoteLobbyPlayer;
    }

    /** Aktuelle Partie der Brücke (oder {@code null}). */
    public static Game currentGame() {
        return BridgeCore.currentGame();
    }

    /**
     * Lage des Tisch-Fensters, die der Tisch zuletzt gemeldet hat (Befehl {@code window}), für die
     * Fensterübergabe; {@code null}, solange er keine gemeldet hat (bei jeder neuen Verbindung
     * zurückgesetzt).
     */
    public static TableWindow tableWindow() {
        com.google.gson.JsonObject w = BridgeCore.tableWindow();
        return w == null ? null : TableWindow.fromJson(w);
    }

    /**
     * Aussehen eines Sitzplatzes an den Tisch schicken (Nachricht {@code look}): Porträt für sein
     * Medaillon und Kartenhülle für die Rückseiten seiner Karten, je als Bilddatei (PNG/JPG,
     * absoluter Pfad; {@code null} = der Tisch nimmt seine eigene). Eingebettet, am besten gleich
     * nach dem Verbinden; der Tisch übernimmt es auch mitten in der Partie.
     */
    public static void sendLook(int seat, String portrait, String sleeve) {
        BridgeCore.sendLook(seat, portrait, sleeve);
    }

    /** Läuft die Brücke eingebettet ({@link #attach})? */
    public static boolean isEmbedded() {
        return BridgeCore.isEmbedded();
    }

    /**
     * Alle vom Tisch gesteuerten Spieler der laufenden Partie geben auf – z. B. wenn das
     * Tisch-Fenster geschlossen wurde, damit die Partie nicht ohne sie weiterläuft.
     */
    public static void concedeRemotePlayers() {
        Game game = currentGame();
        if (game == null || game.isGameOver()) {
            return;
        }
        for (Player p : game.getPlayers()) {
            if (isRemote(p.getLobbyPlayer()) && !p.hasLost()) {
                p.concede();
            }
        }
    }

    /**
     * Eigenständiger Betrieb, vor dem Laden von Forge: Protokoll-Ausgabe auf das echte stdout,
     * alle übrigen Ausgaben nach stderr (Forge schreibt viel auf {@code System.out}).
     */
    public static void prepareStandalone() {
        BridgeCore.prepareStandalone();
    }

    /**
     * Eigenständiger Betrieb: {@code ready} senden, Befehle von stdin lesen, bis stdin schließt;
     * beendet dann den Prozess. Forge muss initialisiert sein.
     */
    public static void runStandalone() throws Exception {
        BridgeCore.runStandalone();
    }

    /** Fehlermeldung {@code error} an den Tisch schicken (z. B. wenn Forge nicht lädt). */
    public static void sendError(String code, Object detail) {
        BridgeCore.sendError(code, detail);
    }
}
