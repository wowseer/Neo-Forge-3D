package forge.neo.clash;

import java.io.File;
import java.io.IOException;
import java.util.Collection;
import java.util.EnumSet;
import java.util.List;
import java.util.Set;

import clash.forgebridge.ClashBridge;
import clash.forgebridge.TableLauncher;
import forge.LobbyPlayer;
import forge.game.GameType;
import forge.game.player.RegisteredPlayer;

/**
 * 3D-Spieltisch aus Neo Forge 3D (Godot) für NeoForge-Partien – die NeoForge-spezifische Hülle um
 * die Brücke ({@link ClashBridge}, {@link TableLauncher}).
 *
 * <p>Ist er eingeschaltet und die Partie ein 1 gegen 1 ({@link #useFor}), setzt
 * {@link forge.neo.match.NeoGame} (bzw. für Adventure-Duelle {@code forge.neo.adventure.NeoDuelBridge}
 * über {@link #openForDuel}) für den menschlichen Sitz {@link #player} ein: Jede Entscheidung
 * des Spielers geht dann über das Protokoll (JSON-Zeilen, siehe Neo Forge 3D
 * {@code docs/Forge-Protocol.md}) an den Godot-Tisch statt an {@code NeoMatchUI}. Menüs, Decks und
 * Modi bleiben bei NeoForge; Partien mit mehr Spielern laufen weiter an NeoForges eigenem Tisch.
 *
 * <p>Einstellungen (Systemeigenschaft, sonst NeoForge-Einstellungen unter Einstellungen → Tisch →
 * „3D-Tisch (Neo Forge 3D)“, sonst Umgebungsvariable):
 * <ul>
 * <li>{@code neo.clash} / {@code NEOFORGE_CLASH} = {@code true}: 3D-Tisch verwenden</li>
 * <li>{@code neo.clash.godot} / {@code CLASH_GODOT}: Godot-Programm (Standard {@code godot})</li>
 * <li>{@code neo.clash.project} / {@code CLASH_PROJECT}: Godot-Projektordner von Neo Forge 3D</li>
 * </ul>
 *
 * <p>Maßgebliche Fassung: Neo Forge 3D {@code integrations/neoforge/src/forge/neo/clash/ClashTable.java}
 * ({@code tools/sync_neoforge.sh} kopiert sie nach NeoForge).
 */
public final class ClashTable {
    /** Schlüssel in den NeoForge-Einstellungen ({@code NeoSettings}, Menü Einstellungen). */
    public static final String SETTING_ON = "clash3d";
    public static final String SETTING_GODOT = "clash3d.godot";
    public static final String SETTING_PROJECT = "clash3d.project";

    private ClashTable() {
    }

    /**
     * Varianten, für die der 3D-Tisch keine Oberfläche hat (außerhalb des Umfangs von Neo Forge 3D:
     * Archenemy, Planechase, Vanguard samt Momir/MoJhoSto mit ihren Avataren).
     */
    private static final Set<GameType> UNSUPPORTED = EnumSet.of(GameType.Archenemy,
            GameType.ArchenemyRumble, GameType.Planechase, GameType.Vanguard, GameType.MomirBasic,
            GameType.MoJhoSto);

    /** Soll der 3D-Tisch verwendet werden (Schalter in den Einstellungen bzw. {@code neo.clash})? */
    public static boolean isEnabled() {
        return Boolean.parseBoolean(setting("neo.clash", "NEOFORGE_CLASH", SETTING_ON, "false"));
    }

    /**
     * Läuft diese Partie am 3D-Tisch? Nur wenn er eingeschaltet ist und genau ein Mensch gegen genau
     * einen Gegner spielt (1 gegen 1, auch Commander) – ohne zweiten Menschen am selben Rechner und
     * ohne Variante, für die der Tisch keine Oberfläche hat. Sonst spielt NeoForge mit seinem
     * eigenen Tisch.
     *
     * @param opponents Zahl der Gegner
     * @param hotSeat   spielt ein Gegner als Mensch am selben Rechner?
     * @param variants  Varianten der Partie ({@code GameRules.setAppliedVariants})
     */
    public static boolean useFor(int opponents, boolean hotSeat, Collection<GameType> variants) {
        if (!isEnabled() || opponents != 1 || hotSeat) {
            return false;
        }
        for (GameType v : variants) {
            if (UNSUPPORTED.contains(v)) {
                return false;
            }
        }
        return true;
    }

    /** Godot-Programm (Einstellung, sonst {@code godot} aus dem PATH). */
    public static String godotProgram() {
        return setting("neo.clash.godot", "CLASH_GODOT", SETTING_GODOT, "godot");
    }

    /** Projektordner von Neo Forge 3D ({@code godot/} mit {@code project.godot}), leer = nicht gesetzt. */
    public static String projectDir() {
        return setting("neo.clash.project", "CLASH_PROJECT", SETTING_PROJECT, "");
    }

    /**
     * Wert: Systemeigenschaft (gilt nur für diesen Start), sonst NeoForge-Einstellung (vom Spieler im
     * Menü gesetzt), sonst Umgebungsvariable (Vorgabe, z. B. aus einem Startskript), sonst
     * {@code fallback}.
     */
    private static String setting(String property, String env, String neoKey, String fallback) {
        String v = System.getProperty(property);
        if (v == null || v.isBlank()) {
            v = forge.neo.NeoSettings.get(neoKey, "");
        }
        if (v == null || v.isBlank()) {
            v = System.getenv(env);
        }
        return v == null || v.isBlank() ? fallback : v.trim();
    }

    /**
     * Startet den Godot-Tisch und verbindet die Brücke. {@code null}, wenn das nicht klappt (dann
     * spielt NeoForge wie gewohnt mit seinem eigenen Tisch).
     */
    public static TableLauncher.Session open() {
        String godot = godotProgram();
        String project = projectDir();
        if (!TableLauncher.isGodotProject(project)) {
            System.err.println("[clash] Godot-Projekt nicht gefunden (neo.clash.project / CLASH_PROJECT): " + project);
            return null;
        }
        TableLauncher.Config config = new TableLauncher.Config(godot, project);
        config = new TableLauncher.Config(config.program(), config.arguments(), config.timeoutMs(),
                new File(System.getProperty("java.io.tmpdir"), "neoforge-clash-godot.log"));
        try {
            TableLauncher.Session session = TableLauncher.launch(config);
            System.out.println("[clash] Godot verbunden (Port " + session.port() + ", Log " + config.log() + ")");
            return session;
        } catch (IOException | RuntimeException e) {
            System.err.println("[clash] 3D-Tisch nicht verfügbar: " + e.getMessage());
            return null;
        }
    }

    /**
     * Für Partien, deren Spieler schon fertig aufgestellt sind (Adventure-Duelle von NeoForge,
     * {@code forge.neo.adventure.NeoDuelBridge}): Läuft die Partie am 3D-Tisch ({@link #useFor}), wird
     * er gestartet und der menschliche Sitz an ihn gehängt. Der Mensch rückt dafür auf Sitzplatz 0
     * (der Tisch zeigt Sitz 0 unten; das Adventure setzt ihn als Letzten) – bei zwei Spielern ändert
     * die Reihenfolge nichts an der Partie. Blockiert, bis Godot verbunden ist (nicht im Faden einer
     * Oberfläche aufrufen).
     *
     * @param players  Spieler der Partie (wird bei Erfolg umsortiert)
     * @param human    menschlicher Sitz (bekommt bei Erfolg den ferngesteuerten Spieler)
     * @param gameType Spielart ({@code GameRules.getGameType()})
     * @param variants Varianten der Partie (darf {@code null} sein)
     * @return die Verbindung zum Tisch (nach der Partie schließen) oder {@code null}: NeoForges
     *         eigener Tisch
     */
    public static TableLauncher.Session openForDuel(List<RegisteredPlayer> players, RegisteredPlayer human,
            GameType gameType, Collection<GameType> variants) {
        Set<GameType> all = EnumSet.of(gameType);
        if (variants != null) {
            all.addAll(variants);
        }
        if (!players.contains(human) || !useFor(players.size() - 1, false, all)) {
            return null;
        }
        TableLauncher.Session session = open();
        if (session == null) {
            return null;
        }
        players.remove(human);
        players.add(0, human);
        human.setPlayer(player(human.getPlayer().getName(), 0));
        return session;
    }

    /**
     * Lobby-Spieler für den menschlichen Sitz: vom Tisch gesteuert, Sitzplatz-Index {@code seat}
     * (Reihenfolge der Spieler in der Partie).
     */
    public static LobbyPlayer player(String name, int seat) {
        return ClashBridge.remotePlayer(name, seat);
    }
}
