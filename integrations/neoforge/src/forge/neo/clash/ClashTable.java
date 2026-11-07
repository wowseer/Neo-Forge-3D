package forge.neo.clash;

import java.io.File;
import java.io.IOException;
import java.util.Collection;
import java.util.EnumSet;
import java.util.List;
import java.util.Set;

import clash.forgebridge.ClashBridge;
import clash.forgebridge.TableLauncher;
import clash.forgebridge.TableWindow;
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
 * <li>{@code neo.clash.table} / {@code CLASH_TABLE}: exportierter Tisch (ausführbare Datei);
 * sonst der mitgelieferte Export ({@link #bundledTable()})</li>
 * <li>{@code neo.clash.godot} / {@code CLASH_GODOT}: Godot-Programm (Standard {@code godot}) und
 * {@code neo.clash.project} / {@code CLASH_PROJECT}: Godot-Projektordner von Neo Forge 3D – nur
 * ohne exportierten Tisch (Entwicklung)</li>
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
    public static final String SETTING_TABLE = "clash3d.table";

    /** Ordner des mitgelieferten Tisches neben NeoForges Klassen bzw. Jar (siehe {@link #bundledTable()}). */
    public static final String BUNDLE_DIR = "clash3d";

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
     * Exportierter Tisch, der gestartet wird: eingestellt ({@code neo.clash.table} / {@code CLASH_TABLE}),
     * sonst der mitgelieferte ({@link #bundledTable()}); {@code null} = keiner, dann Godot-Programm
     * und Projektordner.
     */
    public static File tableProgram() {
        String set = setting("neo.clash.table", "CLASH_TABLE", SETTING_TABLE, "");
        if (!set.isEmpty()) {
            File f = new File(set);
            return f.canExecute() ? f : null;
        }
        return bundledTable();
    }

    /**
     * Mitgelieferter Tisch: der Godot-Export von Neo Forge 3D ({@code tools/export_table.sh}) im
     * Ordner {@value #BUNDLE_DIR} neben NeoForges Klassen – im Quellcode-Build
     * {@code forge-gui-neo/target/clash3d/} (neben {@code target/classes}), im Paket neben dem Jar.
     * Wird über den Ort der Klassen gesucht, nicht über das Arbeitsverzeichnis (das Adventure läuft
     * in einem anderen). {@code null}, wenn es keinen gibt.
     */
    public static File bundledTable() {
        try {
            File code = new File(ClashTable.class.getProtectionDomain().getCodeSource().getLocation().toURI());
            String os = System.getProperty("os.name", "").toLowerCase(java.util.Locale.ROOT);
            String arch = System.getProperty("os.arch", "").toLowerCase(java.util.Locale.ROOT);
            String name = os.contains("win") ? "neo-forge-3d.exe"
                    : "neo-forge-3d." + (arch.contains("aarch64") || arch.contains("arm") ? "arm64" : "x86_64");
            File exe = new File(new File(code.getParentFile(), BUNDLE_DIR), name);
            return exe.canExecute() ? exe : null;
        } catch (java.net.URISyntaxException | RuntimeException e) {
            return null;
        }
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
        return open((TableWindow) null);
    }

    /**
     * Fensterübergabe: startet den Tisch dort, wo das Fenster {@code over} ist (Lage, Größe,
     * maximiert, Vollbild), und versteckt {@code over}, sobald der Tisch verbunden ist – es ist
     * immer nur ein Fenster zu sehen. Nach der Partie {@link #close(TableLauncher.Session,
     * javafx.stage.Stage)}. Ohne {@code over} wie {@link #open()}. Nicht im JavaFX-Faden aufrufen
     * (wartet, bis der Tisch verbunden ist).
     */
    public static TableLauncher.Session open(javafx.stage.Stage over) {
        if (over == null) {
            return open();
        }
        TableWindow at = onFx(() -> over.isShowing() ? windowOf(over) : null);
        TableLauncher.Session session = open(at);
        if (session != null && at != null) {
            onFx(() -> {
                // Das Verstecken des einzigen Fensters beendete sonst JavaFX (wie beim Adventure)
                implicitExit = javafx.application.Platform.isImplicitExit();
                javafx.application.Platform.setImplicitExit(false);
                over.hide();
                return null;
            });
        }
        return session;
    }

    /**
     * Ende der Fensterübergabe: {@code over} kommt dorthin, wo das Tisch-Fenster zuletzt war
     * (gleicher Modus: maximiert, Vollbild oder Lage und Größe), wird gezeigt – erst dann schließt
     * der Tisch. Ohne {@code over} nur {@code session.close()}.
     */
    public static void close(TableLauncher.Session session, javafx.stage.Stage over) {
        if (session == null) {
            return;
        }
        if (over != null) {
            TableWindow last = session.window();
            System.out.println("[clash] Fensterübergabe zurück: " + (last == null ? "keine Lage gemeldet" : last));
            onFx(() -> {
                if (!over.isShowing()) {
                    if (last != null) {
                        applyTo(over, last);
                    }
                    over.show();
                    javafx.application.Platform.setImplicitExit(implicitExit);
                }
                over.toFront();
                over.requestFocus();
                return null;
            });
        }
        session.close();
    }

    /** Fenster, in dem {@code node} gerade zu sehen ist, oder {@code null} (aus jedem Faden). */
    public static javafx.stage.Stage stageOf(javafx.scene.Node node) {
        if (node == null) {
            return null;
        }
        return onFx(() -> node.getScene() != null && node.getScene().getWindow() instanceof javafx.stage.Stage st
                ? st : null);
    }

    /** Wert von {@code Platform.isImplicitExit()} vor dem Verstecken (siehe {@link #open(javafx.stage.Stage)}). */
    private static volatile boolean implicitExit = true;

    /**
     * Lage des Inhaltsbereichs von {@code stage} in Bildschirmpixeln und ihr Modus (JavaFX-Faden).
     * JavaFX rechnet in skalierten Einheiten und mit Rahmen; der Tisch in Pixeln ohne Rahmen.
     */
    public static TableWindow windowOf(javafx.stage.Stage stage) {
        javafx.scene.Scene sc = stage.getScene();
        double sx = stage.getOutputScaleX();
        double sy = stage.getOutputScaleY();
        TableWindow.Mode mode = stage.isFullScreen() ? TableWindow.Mode.FULLSCREEN
                : stage.isMaximized() ? TableWindow.Mode.MAXIMIZED : TableWindow.Mode.WINDOWED;
        return new TableWindow((int) Math.round((stage.getX() + sc.getX()) * sx),
                (int) Math.round((stage.getY() + sc.getY()) * sy),
                (int) Math.round(sc.getWidth() * sx), (int) Math.round(sc.getHeight() * sy), mode);
    }

    /** {@code stage} in Modus und Lage {@code w} bringen (JavaFX-Faden; Gegenstück zu {@link #windowOf}). */
    public static void applyTo(javafx.stage.Stage stage, TableWindow w) {
        switch (w.mode()) {
            case FULLSCREEN -> stage.setFullScreen(true);
            case MAXIMIZED -> {
                stage.setFullScreen(false);
                stage.setMaximized(true);
            }
            case WINDOWED -> {
                stage.setFullScreen(false);
                stage.setMaximized(false);
                javafx.scene.Scene sc = stage.getScene();
                double sx = stage.getOutputScaleX();
                double sy = stage.getOutputScaleY();
                // Rahmen und Titelleiste kennt JavaFX vom letzten Zeigen: Inhaltsbereich angleichen
                double frameW = stage.getWidth() - sc.getWidth();
                double frameH = stage.getHeight() - sc.getHeight();
                stage.setX(w.x() / sx - sc.getX());
                stage.setY(w.y() / sy - sc.getY());
                stage.setWidth(w.width() / sx + frameW);
                stage.setHeight(w.height() / sy + frameH);
            }
        }
    }

    /** {@code task} im JavaFX-Faden ausführen und auf das Ergebnis warten (auch aus ihm heraus). */
    private static <T> T onFx(java.util.function.Supplier<T> task) {
        if (javafx.application.Platform.isFxApplicationThread()) {
            return task.get();
        }
        java.util.concurrent.CompletableFuture<T> f = new java.util.concurrent.CompletableFuture<>();
        javafx.application.Platform.runLater(() -> {
            try {
                f.complete(task.get());
            } catch (RuntimeException e) {
                f.completeExceptionally(e);
            }
        });
        try {
            return f.get();
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            return null;
        } catch (java.util.concurrent.ExecutionException e) {
            System.err.println("[clash] Fensterübergabe: " + e.getCause());
            return null;
        }
    }

    /** Wie {@link #open()}, das Tisch-Fenster öffnet sich bei {@code window} ({@code null} = egal). */
    public static TableLauncher.Session open(TableWindow window) {
        File log = new File(System.getProperty("java.io.tmpdir"), "neoforge-clash-godot.log");
        TableLauncher.Config config;
        File table = tableProgram();
        if (table != null) {
            // exportierter Tisch: Programm und Spieldaten in einer Datei, kein Projektordner
            config = new TableLauncher.Config(table.getPath(), List.of(), 120_000, log);
        } else {
            String project = projectDir();
            if (!TableLauncher.isGodotProject(project)) {
                System.err.println("[clash] Kein exportierter Tisch und kein Godot-Projekt (neo.clash.table / "
                        + "neo.clash.project / CLASH_PROJECT): " + project);
                return null;
            }
            TableLauncher.Config godot = new TableLauncher.Config(godotProgram(), project);
            config = new TableLauncher.Config(godot.program(), godot.arguments(), godot.timeoutMs(), log);
        }
        if (window != null) {
            config = config.withWindow(window);
        }
        try {
            TableLauncher.Session session = TableLauncher.launch(config);
            System.out.println("[clash] Tisch verbunden (" + config.program() + ", Port " + session.port()
                    + ", Log " + config.log() + ")");
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
        return openForDuel(players, human, gameType, variants, null);
    }

    /**
     * Wie {@link #openForDuel(List, RegisteredPlayer, GameType, Collection)}; das Tisch-Fenster
     * öffnet sich bei {@code window} (Fensterübergabe; {@code null} = egal).
     */
    public static TableLauncher.Session openForDuel(List<RegisteredPlayer> players, RegisteredPlayer human,
            GameType gameType, Collection<GameType> variants, TableWindow window) {
        Set<GameType> all = EnumSet.of(gameType);
        if (variants != null) {
            all.addAll(variants);
        }
        if (!players.contains(human) || !useFor(players.size() - 1, false, all)) {
            return null;
        }
        TableLauncher.Session session = open(window);
        if (session == null) {
            return null;
        }
        players.remove(human);
        players.add(0, human);
        human.setPlayer(player(human.getPlayer().getName(), 0));
        return session;
    }

    /** Zufall für die Gesichter der Gegner. */
    private static final java.util.Random RANDOM = new java.util.Random();

    /**
     * Aussehen aus „Anpassen“ an den Tisch schicken ({@link ClashBridge#sendLook}): Der vom Tisch
     * gespielte Sitz bekommt dein Gesicht und deine Kartenhülle (die des Decks, sonst die aus
     * „Anpassen“, {@code NeoLook.sleeveInPlay()} – wie NeoForges eigener Tisch), jeder Gegner ein
     * zufälliges Gesicht aus Forges Avataren und seine Hülle (aus „Anpassen → Rivalen“, sonst die,
     * die ihm die Engine gibt). Bilder aus Forges Sprite-Bögen werden als PNG in den Temp-Ordner
     * geschrieben. Nach dem Aufstellen der Spieler aufrufen (Sitzplatz = Index in {@code players}).
     */
    public static void sendLook(List<RegisteredPlayer> players) {
        File dir = new File(System.getProperty("java.io.tmpdir"), "neoforge-clash-look");
        // Bilder der vorigen Partie weg (immer nur eine Partie am Tisch)
        File[] old = dir.listFiles();
        if (old != null) {
            for (File f : old) {
                f.delete();
            }
        }
        dir.mkdirs();
        long stamp = System.currentTimeMillis();
        for (int seat = 0; seat < players.size(); seat++) {
            LobbyPlayer lp = players.get(seat).getPlayer();
            try {
                forge.neo.look.LookItem face;
                forge.neo.look.LookItem sleeve;
                if (ClashBridge.isRemote(lp)) {
                    face = forge.neo.look.NeoLook.currentAvatar();
                    sleeve = forge.neo.look.NeoLook.sleeveInPlay();
                } else {
                    int faces = forge.neo.look.NeoLook.builtInAvatarCount();
                    face = faces > 0 ? forge.neo.look.NeoLook.builtInAvatar(RANDOM.nextInt(faces)) : null;
                    String custom = forge.neo.look.NeoPlayers.customSleeveOf(lp.getName());
                    forge.neo.look.LookItem own = custom == null ? null : forge.neo.look.NeoLook.sleeveById(custom);
                    sleeve = own != null ? own : forge.neo.look.NeoLook.builtInSleeve(lp.getSleeveIndex());
                }
                String facePath = lookFile(face, dir, "face-" + seat + "-" + stamp);
                String sleevePath = lookFile(sleeve, dir, "sleeve-" + seat + "-" + stamp);
                System.out.println("[clash] Aussehen Sitz " + seat + ": Gesicht " + facePath + ", Hülle " + sleevePath);
                ClashBridge.sendLook(seat, facePath, sleevePath);
            } catch (RuntimeException e) {
                // ohne Gesicht und Hülle spielt es sich genauso
                System.err.println("[clash] Aussehen für Sitz " + seat + " nicht verfügbar: " + e);
            }
        }
    }

    /** Bilddatei zu {@code item}: eigene Datei direkt, Sprite als PNG nach {@code dir}; sonst {@code null}. */
    private static String lookFile(forge.neo.look.LookItem item, File dir, String name) {
        if (item == null || item.isNone()) {
            return null;
        }
        if (item.getFile() != null && item.getFile().isFile()) {
            return item.getFile().getAbsolutePath();
        }
        javafx.scene.image.Image image = item.image();
        if (image == null) {
            return null;
        }
        File f = new File(dir, name + ".png");
        try {
            forge.neo.Snapshots.writePng(image, f);
            return f.getAbsolutePath();
        } catch (IOException e) {
            System.err.println("[clash] Bild nicht geschrieben (" + f + "): " + e.getMessage());
            return null;
        }
    }

    /**
     * Lobby-Spieler für den menschlichen Sitz: vom Tisch gesteuert, Sitzplatz-Index {@code seat}
     * (Reihenfolge der Spieler in der Partie).
     */
    public static LobbyPlayer player(String name, int seat) {
        return ClashBridge.remotePlayer(name, seat);
    }
}
