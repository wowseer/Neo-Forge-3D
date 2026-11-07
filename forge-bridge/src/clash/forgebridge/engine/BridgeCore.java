package clash.forgebridge.engine;

import java.io.BufferedReader;
import java.io.File;
import java.io.FileDescriptor;
import java.io.FileOutputStream;
import java.io.InputStreamReader;
import java.io.PrintStream;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Random;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicInteger;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonNull;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;

import forge.LobbyPlayer;
import forge.deck.Deck;
import forge.deck.io.DeckSerializer;
import forge.game.Game;
import forge.game.GameOutcome;
import forge.game.GameRules;
import forge.game.GameType;
import forge.game.Match;
import forge.game.player.RegisteredPlayer;
import forge.model.FModel;
import forge.player.GamePlayerUtil;
import forge.util.MyRandom;

/**
 * Kern der Brücke zwischen Neo Forge 3D (Godot/Rust) und der Forge-Regel-Engine (intern – Gastgeber
 * und andere Projekte benutzen die Fassade {@link clash.forgebridge.ClashBridge}).
 *
 * <p>Läuft als eigener Prozess ({@link clash.forgebridge.app.BridgeMain}, stdin/stdout) oder eingebettet in einen
 * Gastgeber wie NeoForge ({@link #attach}, lokaler Socket). Protokoll: JSON Lines (UTF-8), eine
 * Nachricht pro Zeile. Im eigenen Prozess werden alle sonstigen Ausgaben von Forge nach stderr
 * umgeleitet, damit stdout sauber bleibt.
 * Das vollständige Protokoll steht in {@code docs/Forge-Protocol.md}.
 *
 * <p>Muss mit dem Forge-Installationsordner als Arbeitsverzeichnis gestartet werden
 * (Forge lädt {@code res/} relativ dazu).
 */
public final class BridgeCore {
    /** Version des Protokolls; wird in "ready" gemeldet. */
    public static final int PROTOCOL_VERSION = 3;

    private static PrintStream out;
    private static final Object OUT_LOCK = new Object();
    private static final Map<Integer, CompletableFuture<JsonElement>> PENDING = new ConcurrentHashMap<>();
    /** Offene Anfragen vom Typ {@code Priority} (für {@link #debugChanged}). */
    private static final java.util.Set<Integer> PRIORITY_IDS = ConcurrentHashMap.newKeySet();
    /**
     * Änderungen des Debug-Werkzeugs: laufen im Spiel-Thread, während er in {@link #request} auf
     * eine Antwort wartet (dort ruht Forge; aus einem anderen Thread gäbe es
     * {@code ConcurrentModificationException}).
     */
    private static final java.util.concurrent.ConcurrentLinkedQueue<Runnable> GAME_TASKS = new java.util.concurrent.ConcurrentLinkedQueue<>();

    /** {@code task} im Spiel-Thread ausführen, sobald er das nächste Mal auf eine Antwort wartet. */
    static void runOnGameThread(Runnable task) {
        GAME_TASKS.add(task);
    }
    private static final AtomicInteger NEXT_ID = new AtomicInteger(1);
    private static volatile Thread gameThread;
    private static volatile Game currentGame;
    /** Eingebettet in einen Gastgeber (NeoForge) statt eigener Prozess. */
    private static volatile boolean embedded;
    /** Zuletzt gemeldete Lage des Tisch-Fensters (Befehl {@code window}), siehe {@link #tableWindow()}. */
    private static volatile JsonObject tableWindow;
    /** Eingebettet: Verbindung zum Tisch beendet, Anfragen entscheidet die Engine. */
    private static volatile boolean disconnected;

    private BridgeCore() {}

    /**
     * Eigenständiger Betrieb, vor dem Laden von Forge: Protokoll-Ausgabe auf das echte stdout,
     * alle übrigen Ausgaben ({@code System.out} von Forge) nach stderr.
     */
    public static void prepareStandalone() {
        out = new PrintStream(new FileOutputStream(FileDescriptor.out), true, StandardCharsets.UTF_8);
        System.setOut(System.err);
    }

    /**
     * Eigenständiger Betrieb (Prozess, gestartet von Godot): Nachrichten auf stdout, Befehle von
     * stdin. Forge muss vorher initialisiert sein (siehe {@link clash.forgebridge.app.BridgeMain}).
     */
    public static void runStandalone() throws Exception {
        sendReady();
        BufferedReader in = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));
        readCommands(in);
        // stdin geschlossen: Godot ist weg -> beenden.
        System.exit(0);
    }

    /** Zuletzt vom Tisch gemeldete Fensterlage (Befehl {@code window}) oder {@code null}. */
    public static JsonObject tableWindow() {
        return tableWindow;
    }

    /**
     * Eingebetteter Betrieb (z. B. in NeoForge): Die Partie startet der Gastgeber selbst; Neo Forge 3D
     * ist nur der Spieltisch. Nachrichten gehen auf {@code os}, Befehle kommen von {@code is}
     * (typischerweise ein lokaler Socket). Liest in einem eigenen Thread; ist die Verbindung zu,
     * werden offene Anfragen beendet (die Engine entscheidet dann selbst) und {@code onClose}
     * läuft. {@code start_match} ist hier nicht erlaubt; die Partie meldet
     * {@link RemoteLobbyPlayer} mit {@link #gameStarted}.
     */
    public static void attach(java.io.OutputStream os, java.io.InputStream is, Runnable onClose) {
        out = new PrintStream(os, true, StandardCharsets.UTF_8);
        embedded = true;
        tableWindow = null;
        disconnected = false;
        sendReady();
        Thread reader = new Thread(() -> {
            try {
                readCommands(new BufferedReader(new InputStreamReader(is, StandardCharsets.UTF_8)));
            } catch (Exception e) {
                // Verbindung abgebrochen
            }
            // Ab jetzt antwortet niemand mehr: offene und alle späteren Anfragen entscheidet die
            // Engine (sonst wartete die nächste Anfrage ewig und die Aufgabe käme nie zum Zug)
            disconnected = true;
            PENDING.values().forEach(f -> f.complete(JsonNull.INSTANCE));
            PENDING.clear();
            if (onClose != null) {
                onClose.run();
            }
        }, "clash-bridge-reader");
        reader.setDaemon(true);
        reader.start();
    }

    /** Aktuelle Partie (oder {@code null}), z. B. damit der Gastgeber aufgeben kann. */
    public static Game currentGame() {
        return currentGame;
    }

    /** Läuft die Brücke eingebettet ({@link #attach})? */
    public static boolean isEmbedded() {
        return embedded;
    }

    private static void sendReady() {
        JsonObject ready = message("ready");
        ready.addProperty("protocol", PROTOCOL_VERSION);
        ready.addProperty("embedded", embedded);
        ready.addProperty("working_dir", new File(".").getAbsolutePath());
        send(ready);
    }

    private static void readCommands(BufferedReader in) throws java.io.IOException {
        String line;
        while ((line = in.readLine()) != null) {
            line = line.trim();
            if (line.isEmpty()) {
                continue;
            }
            if (embedded && line.contains("\"quit\"")) {
                return;
            }
            try {
                handle(JsonParser.parseString(line).getAsJsonObject());
            } catch (Exception e) {
                sendError("bad_command", e);
            }
        }
    }

    /**
     * Eingebettet: eine neue Partie hat begonnen (von {@link RemoteLobbyPlayer} beim Anlegen des
     * Spielers gemeldet). Hängt Ereignis- und Protokoll-Weiterleitung an und meldet
     * {@code match_started}; der vollständige Zustand kommt mit der ersten Anfrage.
     */
    static void gameStarted(Game game) {
        if (currentGame == game) {
            return;
        }
        currentGame = game;
        game.subscribeToEvents(new EventForwarder(game));
        game.subscribeToEvents(new DebugTools.Refiller(game));
        game.getGameLog().addObserver(new LogForwarder(game));
        send(message("match_started"));
    }

    /** Partie zu Ende: {@code game_over} mit Sieger, Unentschieden und Grund. */
    static void sendGameOver(Game game) {
        JsonObject over = message("game_over");
        GameOutcome outcome = game.getOutcome();
        if (outcome != null) {
            over.addProperty("draw", outcome.isDraw());
            LobbyPlayer winner = outcome.getWinningLobbyPlayer();
            over.addProperty("winner", winner == null ? null : winner.getName());
            over.addProperty("reason", String.valueOf(outcome.getWinCondition()));
        }
        send(over);
    }

    // ------------------------------------------------------------------ Befehle

    private static void handle(JsonObject cmd) {
        String name = cmd.has("cmd") ? cmd.get("cmd").getAsString() : "";
        switch (name) {
            case "ping" -> {
                JsonObject pong = message("pong");
                if (cmd.has("nonce")) {
                    pong.add("nonce", cmd.get("nonce"));
                }
                send(pong);
            }
            case "start_match" -> {
                if (embedded) {
                    sendError("embedded", "Die Partie startet der Gastgeber (NeoForge)");
                } else {
                    startMatch(cmd);
                }
            }
            case "answer" -> {
                int id = cmd.get("id").getAsInt();
                CompletableFuture<JsonElement> f = PENDING.remove(id);
                if (f == null) {
                    sendError("unknown_request", "Keine offene Anfrage mit id " + id);
                } else {
                    f.complete(cmd.has("choice") ? cmd.get("choice") : JsonNull.INSTANCE);
                }
            }
            case "state" -> {
                Game g = currentGame;
                if (g == null) {
                    sendError("no_match", "Kein laufendes Spiel");
                } else {
                    JsonObject st = message("state");
                    st.add("state", StateJson.snapshotFromOtherThread(g, cmd.has("viewer") ? cmd.get("viewer").getAsInt() : -1));
                    send(st);
                }
            }
            // Kartendaten zu Namen (z. B. Treffer bei „Kartennamen nennen“): Name, Typ, Kosten,
            // Text, Stärke/Widerstand, Bild – Antwort {"type":"card_info","cards":[…]}
            case "card_info" -> {
                JsonArray cards = new JsonArray();
                if (cmd.has("names") && cmd.get("names").isJsonArray()) {
                    for (JsonElement n : cmd.getAsJsonArray("names")) {
                        String cardName = n.getAsString();
                        forge.item.PaperCard pc = forge.StaticData.instance().getCommonCards().getCard(cardName);
                        if (pc == null) {
                            continue;
                        }
                        forge.card.CardRules r = pc.getRules();
                        forge.card.ICardFace f = r.getMainPart();
                        JsonObject o = RemotePlayerController.faceJson(0, cardName, String.valueOf(f.getType()),
                                f.getManaCost() == null ? "" : f.getManaCost().getShortString(), f.getOracleText(),
                                f.getPower(), f.getToughness());
                        String img = CardImages.paperImage(pc);
                        if (!img.isEmpty()) {
                            o.addProperty("image", img);
                        }
                        cards.add(o);
                    }
                }
                JsonObject m = message("card_info");
                m.add("cards", cards);
                send(m);
            }
            // Debug-Fenster der UI (nur in Debug-Builds von Godot): siehe DebugTools
            case "debug_card_names" -> DebugTools.sendCardNames();
            case "debug_add_card" -> {
                if (currentGame == null) {
                    sendError("no_match", "Kein laufendes Spiel");
                } else {
                    DebugTools.addCardToHand(currentGame, cmd.has("seat") ? cmd.get("seat").getAsInt() : 0,
                            cmd.has("name") ? cmd.get("name").getAsString() : "");
                }
            }
            case "debug_infinite_mana" -> DebugTools.setInfiniteMana(currentGame,
                    cmd.has("seat") ? cmd.get("seat").getAsInt() : 0, cmd.has("on") && cmd.get("on").getAsBoolean());
            // Lage des Tisch-Fensters (Fensterübergabe im Gastgeber), siehe ClashBridge.tableWindow
            case "window" -> tableWindow = cmd;
            case "quit" -> {
                if (!embedded) {
                    System.exit(0);
                }
            }
            default -> sendError("unknown_command", "Unbekannter Befehl: " + name);
        }
    }

    private static void startMatch(JsonObject cmd) {
        if (gameThread != null && gameThread.isAlive()) {
            sendError("match_running", "Es läuft bereits ein Spiel");
            return;
        }
        JsonArray decks = cmd.getAsJsonArray("decks");
        JsonArray remote = cmd.has("remote") ? cmd.getAsJsonArray("remote") : new JsonArray();
        JsonArray names = cmd.has("names") ? cmd.getAsJsonArray("names") : new JsonArray();
        // Tempo der KI (1 = normal, 0 = ohne Pausen), siehe AiPace
        AiPace.setFactor(cmd.has("ai_pace") ? cmd.get("ai_pace").getAsDouble() : 1.0);
        if (decks == null || decks.size() < 2) {
            sendError("bad_command", "start_match braucht mindestens zwei Decks");
            return;
        }
        if (cmd.has("seed")) {
            MyRandom.setRandom(new Random(cmd.get("seed").getAsLong()));
        }

        List<RegisteredPlayer> players = new ArrayList<>();
        for (int i = 0; i < decks.size(); i++) {
            Deck deck = loadDeck(decks.get(i).getAsString());
            if (deck == null) {
                sendError("deck_not_found", "Deck nicht gefunden: " + decks.get(i).getAsString());
                return;
            }
            String playerName = i < names.size() ? names.get(i).getAsString() : "Spieler " + (i + 1);
            boolean isRemote = i < remote.size() && remote.get(i).getAsBoolean();
            LobbyPlayer lobby = isRemote
                    ? new RemoteLobbyPlayer(playerName, i)
                    : GamePlayerUtil.createAiPlayer(playerName, 0);
            // Commander-Partie (CR 903): Deck mit Abschnitt [Commander], Startleben 40
            boolean commander = cmd.has("game_type") && "Commander".equalsIgnoreCase(cmd.get("game_type").getAsString());
            RegisteredPlayer rp = commander ? RegisteredPlayer.forCommander(deck) : new RegisteredPlayer(deck);
            rp.setPlayer(lobby);
            if (cmd.has("starting_life")) {
                rp.setStartingLife(cmd.get("starting_life").getAsInt());
            }
            players.add(rp);
        }

        boolean commanderGame = cmd.has("game_type") && "Commander".equalsIgnoreCase(cmd.get("game_type").getAsString());
        GameRules rules = new GameRules(commanderGame ? GameType.Commander : GameType.Constructed);
        if (commanderGame) {
            rules.addAppliedVariant(GameType.Commander);
        }
        rules.setGamesPerMatch(1);
        // Alte Regel „Kampfteilnehmer ordnen“ (Schadens-Reihenfolge der Blocker, vor CR-Änderung 2024)
        if (cmd.has("order_combatants")) {
            rules.setOrderCombatants(cmd.get("order_combatants").getAsBoolean());
        }
        Match match = new Match(rules, players, "Neo Forge 3D");
        Game game = match.createGame();
        currentGame = game;
        game.subscribeToEvents(new EventForwarder(game));
        game.subscribeToEvents(new DebugTools.Refiller(game));
        game.getGameLog().addObserver(new LogForwarder(game));

        gameThread = new Thread(() -> runGame(match, game), "forge-game");
        gameThread.setDaemon(true);
        gameThread.start();
    }

    private static void runGame(Match match, Game game) {
        JsonObject started = message("match_started");
        started.add("state", StateJson.snapshot(game, -1));
        send(started);
        try {
            match.startGame(game);
        } catch (Throwable t) {
            sendError("game_crashed", t);
        }
        sendGameOver(game);
        // currentGame bleibt gesetzt, damit "state" auch nach Spielende den Endstand liefert.
        // Offene Anfragen beenden, damit kein Thread hängen bleibt.
        PENDING.values().forEach(f -> f.complete(JsonNull.INSTANCE));
        PENDING.clear();
    }

    /** Lädt ein Deck aus einer .dck-Datei oder (falls keine Datei) aus Forges Constructed-Decks. */
    private static Deck loadDeck(String spec) {
        File file = new File(spec);
        if (file.isFile()) {
            return DeckSerializer.fromFile(file);
        }
        return FModel.getDecks().getConstructed().get(spec);
    }

    // ------------------------------------------------------------------ Ausgabe

    /** Neue Nachricht mit gesetztem "type". */
    static JsonObject message(String type) {
        JsonObject o = new JsonObject();
        o.addProperty("type", type);
        return o;
    }

    /** Schreibt eine Nachricht als eine JSON-Zeile auf stdout (thread-sicher). */
    static void send(JsonObject o) {
        synchronized (OUT_LOCK) {
            out.println(o.toString());
            out.flush();
        }
    }

    public static void sendError(String code, Object detail) {
        JsonObject e = message("error");
        e.addProperty("code", code);
        if (detail instanceof Throwable t) {
            e.addProperty("message", t.toString());
            t.printStackTrace(System.err);
        } else {
            e.addProperty("message", String.valueOf(detail));
        }
        send(e);
    }

    /**
     * Stellt dem Client eine Entscheidung (Protokoll v3) und blockiert den Spiel-Thread bis zur
     * Antwort ({"cmd":"answer","id":…,"choice":{…}}). {@code envelope} enthält
     * {@code decision} ({@code Decision}) und die Anzeigehilfen (siehe {@link Decisions});
     * {@code type}, {@code id} und {@code player} werden hier ergänzt. Liefert die {@code choice}
     * (oder JsonNull).
     */
    static JsonElement request(int player, JsonObject envelope) {
        if (disconnected) {
            return JsonNull.INSTANCE;
        }
        int id = NEXT_ID.getAndIncrement();
        CompletableFuture<JsonElement> f = new CompletableFuture<>();
        PENDING.put(id, f);
        if (disconnected) {
            // gerade getrennt, nachdem die offenen Anfragen beendet wurden
            PENDING.remove(id);
            return JsonNull.INSTANCE;
        }
        boolean priority = envelope.has("decision") && envelope.get("decision").isJsonObject()
                && envelope.getAsJsonObject("decision").has("type")
                && "Priority".equals(envelope.getAsJsonObject("decision").get("type").getAsString());
        if (priority) {
            PRIORITY_IDS.add(id);
        }
        JsonObject req = message("request");
        req.addProperty("id", id);
        req.addProperty("player", player);
        for (Map.Entry<String, JsonElement> e : envelope.entrySet()) {
            req.add(e.getKey(), e.getValue());
        }
        send(req);
        try {
            while (true) {
                Runnable task;
                while ((task = GAME_TASKS.poll()) != null) {
                    try {
                        task.run();
                    } catch (RuntimeException e) {
                        sendError("debug_failed", String.valueOf(e));
                    }
                }
                try {
                    return f.get(50, java.util.concurrent.TimeUnit.MILLISECONDS);
                } catch (java.util.concurrent.TimeoutException e) {
                    // weiter warten, zwischendurch Debug-Aufgaben abarbeiten
                }
            }
        } catch (Exception e) {
            return JsonNull.INSTANCE;
        } finally {
            PRIORITY_IDS.remove(id);
        }
    }

    /**
     * Neue Kartenbilder sind fertig heruntergeladen ({@link CardImages}): neuen Zustand schicken,
     * damit die UI sie zeigt, auch wenn im Spiel gerade nichts passiert (Sitzplatz 0 wie bei den
     * Debug-Werkzeugen).
     */
    static void imagesChanged() {
        Game g = currentGame;
        if (g == null) {
            return;
        }
        JsonObject st = message("state");
        st.add("state", StateJson.snapshotFromOtherThread(g, 0));
        send(st);
    }

    /**
     * Debug-Werkzeug hat den Spielzustand geändert: neuen Zustand schicken; offene
     * Priorität mit {@code {"type":"Refresh"}} beantworten, damit sie mit den neuen Karten bzw.
     * neuem Mana neu gestellt wird.
     */
    static void debugChanged(Game game) {
        JsonObject st = message("state");
        st.add("state", StateJson.snapshot(game, 0));
        send(st);
        for (Integer id : PRIORITY_IDS) {
            CompletableFuture<JsonElement> f = PENDING.remove(id);
            if (f != null) {
                JsonObject refresh = new JsonObject();
                refresh.addProperty("type", "Refresh");
                f.complete(refresh);
            }
        }
    }
}
