package clash.forgebridge.engine;

import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.time.Duration;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

import forge.StaticData;
import forge.card.CardEdition;
import forge.game.card.Card;
import forge.item.IPaperCard;
import forge.item.PaperCard;
import forge.gui.download.CdnUuidCache;
import forge.localinstance.properties.ForgeConstants;
import forge.util.ImageUtil;
import forge.util.ScryfallRateLimiter;

/**
 * Kartenbilder (JPG) für die 3D-Karten, wie Forge und NeoForge sie benutzen: Scryfall-Bilder
 * im Bildordner von Forge ({@code pics/cards/<SET>/<Name>.fullborder.jpg}).
 *
 * <p>Gesucht wird im Bildordner dieser Forge-Installation ({@link ForgeConstants#CACHE_CARD_PICS_DIR})
 * und in dem von NeoForge (Umgebungsvariable {@code CLASH_EXTRA_PICS}, sonst
 * {@code ~/Games/NeoForge/datos/cache/pics/cards}). Fehlt ein Bild, lädt es ein Hintergrund-Thread
 * von Scryfall in den Forge-Bildordner – mehrere gleichzeitig, möglichst direkt von der CDN
 * ({@link #cdnFor}), sonst über die API im Takt von Forges {@link ScryfallRateLimiter}; der
 * nächste Zustand enthält dann den Pfad. Die Bilder gehören Wizards of the Coast
 * und kommen nie ins Git.
 */
final class CardImages {
    private CardImages() {}

    /** Bildpfad je Bild-Schlüssel ("" = keins gefunden und keins ladbar). */
    private static final Map<String, String> FOUND = new ConcurrentHashMap<>();
    /** Schlüssel, deren Download läuft oder fehlschlug (nicht erneut anstoßen). */
    private static final Set<String> REQUESTED = ConcurrentHashMap.newKeySet();
    /**
     * Mehrere Downloads gleichzeitig: die meisten Bilder kommen von Scryfalls CDN
     * ({@code cards.scryfall.io}, ohne Limit). Anfragen an die API takten trotzdem über
     * {@link ScryfallRateLimiter} (dieselbe Uhr wie Forge und NeoForge).
     */
    private static final int PARALLEL = 6;
    private static final ExecutorService DOWNLOADER = Executors.newFixedThreadPool(PARALLEL, r -> {
        Thread t = new Thread(r, "clash-card-images");
        t.setDaemon(true);
        return t;
    });
    /**
     * HTTP/1.1 mit eigenen Verbindungen je Download: mit HTTP/2 bündelt der Client alle
     * parallelen Downloads in EINE Verbindung zur CDN, und jedes Bild brauchte ~0,9 s statt
     * ~0,3 s (gemessen: 40 Karten 5,8 s gegen 2,1 s).
     */
    private static final HttpClient HTTP = HttpClient.newBuilder()
            .version(HttpClient.Version.HTTP_1_1)
            .followRedirects(HttpClient.Redirect.NORMAL)
            .connectTimeout(Duration.ofSeconds(10))
            .build();
    /** Wartende Downloads (für die Zustandsmeldung nach dem letzten). */
    private static final java.util.concurrent.atomic.AtomicInteger QUEUED = new java.util.concurrent.atomic.AtomicInteger();
    /** Zeitpunkt der letzten Zustandsmeldung wegen neuer Bilder (Millisekunden). */
    private static final java.util.concurrent.atomic.AtomicLong LAST_NOTIFY = new java.util.concurrent.atomic.AtomicLong();
    /** Bei einer langen Reihe spätestens so oft einen Zwischenstand schicken (Millisekunden). */
    private static final long NOTIFY_EVERY_MS = 400;

    /**
     * Bilder einer Karte: {@code image} = ganzes Kartenbild, {@code art} = Bildausschnitt (Scryfall
     * {@code art_crop}) nur bei Karten ohne Druck mit normalem Rahmen (sonst leer), {@code crop} =
     * Bildausschnitt immer (für die Kacheln auf dem Spielfeld).
     */
    record Info(String image, String art, String crop) {
        static final Info NONE = new Info("", "", "");
    }

    /** Gewählter Anzeige-Druck je Kartenname und Druck (normaler Rahmen bevorzugt). */
    private static final Map<String, PaperCard> DISPLAY = new ConcurrentHashMap<>();

    /**
     * Bilder der angezeigten Seite von {@code c}; fehlende werden im Hintergrund geladen (dann
     * leer, ein späterer Zustand bringt den Pfad). Für die Anzeige zählt ein Druck mit normalem
     * Rahmen ({@link #regular}): ist der Druck der Karte keiner, wird der neueste normale Druck
     * gezeigt (die Karte im Spiel bleibt unverändert); gibt es keinen, kommt zusätzlich der
     * Bildausschnitt mit, damit die UI eigene Rahmen bauen kann.
     */
    static Info info(Card c) {
        if (c == null || c.isFaceDown()) {
            return Info.NONE;
        }
        IPaperCard ip = c.getPaperCard();
        // Zauber-Kopie einer vorbereiteten Kreatur (Prepare): die gedruckte Kreaturenkarte, auf der
        // der Zauber mit abgedruckt ist (wie Forges eigener Bildschlüssel für diese Seite)
        if (c.getCurrentStateName() == forge.card.CardStateName.PreparedSpell) {
            ip = forge.StaticData.instance().getCommonCards()
                    .getCard(c.getState(forge.card.CardStateName.Original).getName());
        } else if (c.isToken() || ip instanceof forge.item.PaperToken) {
            // Spielsteine und Spielsteine-artige Karten (Dungeons, CR 309) mit Token-Druck
            String t = tokenImage(c);
            return t.isEmpty() ? Info.NONE : new Info(t, "", "");
        }
        if (!(ip instanceof PaperCard pc)) {
            return Info.NONE;
        }
        String face = c.isBackSide() || c.isFlipped() ? "back" : "";
        PaperCard shown = DISPLAY.computeIfAbsent(pc.getName() + "|" + pc.getEdition() + "|" + pc.getArtIndex(),
                k -> displayPrinting(pc));
        String image = resolve(shown, face, false);
        // Bildausschnitt: immer (Kacheln auf dem Spielfeld); `art` nur bei Sonderrahmen
        String crop = resolve(shown, face, true);
        String art = regular(shown) ? "" : crop;
        return new Info(image, art, crop);
    }

    /** Bild (Anzeige-Druck mit normalem Rahmen) einer Karte aus der Kartendatenbank; "" = (noch) keins. */
    static String paperImage(PaperCard pc) {
        if (pc == null) {
            return "";
        }
        PaperCard shown = DISPLAY.computeIfAbsent(pc.getName() + "|" + pc.getEdition() + "|" + pc.getArtIndex(),
                k -> displayPrinting(pc));
        return resolve(shown, "", false);
    }

    /** Bild einer Seite (Name) der Karte {@code host}, z. B. für „Seite wählen“; sonst "". */
    static String faceImage(Card host, String faceName) {
        if (host == null || !(host.getPaperCard() instanceof PaperCard pc)) {
            return "";
        }
        PaperCard shown = DISPLAY.computeIfAbsent(pc.getName() + "|" + pc.getEdition() + "|" + pc.getArtIndex(),
                k -> displayPrinting(pc));
        String face = pc.getRules().getMainPart() != null && faceName != null
                && !faceName.equals(pc.getRules().getMainPart().getName()) ? "back" : "";
        return resolve(shown, face, false);
    }

    /**
     * Bild eines Spielsteins (Scryfall, Token-Druck der Edition): im Token-Bildordner von Forge
     * bzw. NeoForge, sonst im Hintergrund geladen; "" = (noch) keins.
     */
    private static String tokenImage(Card c) {
        return c.getPaperCard() instanceof forge.item.PaperToken pt ? tokenImage(pt) : "";
    }

    /**
     * Bild eines Dungeons (bzw. anderen Token-Skripts) nach seinem Namen, z. B. für „Dungeon
     * wählen“, wo es die Karte noch nicht gibt; "" = (noch) keins.
     */
    static String tokenImageByName(String name) {
        if (name == null || name.isEmpty()) {
            return "";
        }
        try {
            for (var e : StaticData.instance().getAllTokens().getRules().entrySet()) {
                if (e.getValue().getMainPart() != null && name.equals(e.getValue().getMainPart().getName())) {
                    forge.item.PaperToken pt = StaticData.instance().getAllTokens().getToken(e.getKey());
                    return pt == null ? "" : tokenImage(pt);
                }
            }
        } catch (RuntimeException ignored) {
            // keine Token-Datenbank
        }
        return "";
    }

    private static String tokenImage(forge.item.PaperToken pt) {
        String file;
        try {
            file = pt.getImageFilename(1);
        } catch (RuntimeException e) {
            return "";
        }
        if (file == null || file.isEmpty()) {
            return "";
        }
        String key = "token|" + file;
        String known = FOUND.get(key);
        if (known != null && !known.isEmpty()) {
            return known;
        }
        List<File> dirs = new ArrayList<>();
        dirs.add(new File(ForgeConstants.CACHE_TOKEN_PICS_DIR));
        dirs.add(new File(System.getProperty("user.home"), "Games/NeoForge/datos/cache/pics/tokens"));
        for (File d : dirs) {
            for (String ext : List.of(".jpg", ".png")) {
                File f = new File(d, file + ext);
                if (f.isFile() && f.length() > 0) {
                    FOUND.put(key, f.getAbsolutePath());
                    return f.getAbsolutePath();
                }
            }
        }
        CardEdition ed = StaticData.instance().getCardEdition(pt.getEdition());
        String cn = pt.getCollectorNumber();
        if (ed == null || cn == null || cn.isEmpty() || REQUESTED.contains(key)) {
            return "";
        }
        REQUESTED.add(key);
        String url;
        try {
            url = ForgeConstants.URL_PIC_SCRYFALL_DOWNLOAD
                    + ImageUtil.getScryfallTokenDownloadUrl(cn, ed.getTokensCode().toLowerCase(), "en", "");
        } catch (RuntimeException e) {
            return "";
        }
        File target = new File(ForgeConstants.CACHE_TOKEN_PICS_DIR, file + ".jpg");
        enqueue(url, target, key);
        return "";
    }

    /** Editionsarten mit normalem Rahmen (Kernset, Erweiterung, Reprint-, Commander-, Draft-Sets …). */
    private static final java.util.EnumSet<CardEdition.Type> REGULAR_TYPES = java.util.EnumSet.of(
            CardEdition.Type.CORE, CardEdition.Type.EXPANSION, CardEdition.Type.REPRINT,
            CardEdition.Type.COMMANDER, CardEdition.Type.DRAFT, CardEdition.Type.STARTER,
            CardEdition.Type.DUEL_DECK);

    /**
     * Druck mit normalem Rahmen: aus einer Edition mit normalen Rahmen ({@link #REGULAR_TYPES},
     * nicht Secret Lair, Promo, Mystery Booster, „The List“ …) und im Abschnitt {@code cards} der
     * Editionsdatei (nicht Showcase, Borderless, Extended Art …). Bei solchen Drucken liegen
     * Name, Kosten und Textfeld im Bild an der gewohnten Stelle.
     */
    private static boolean regular(PaperCard pc) {
        CardEdition ed = StaticData.instance().getCardEdition(pc.getEdition());
        if (ed == null || !REGULAR_TYPES.contains(ed.getType()) || "PLST".equals(ed.getCode())) {
            return false;
        }
        String section = ed.getSectionForCollectorNumber(pc.getCollectorNumber());
        return section == null || section.isEmpty() || section.equalsIgnoreCase("cards");
    }

    /** Anzeige-Druck: der eigene, wenn er normal ist, sonst der neueste normale, sonst der eigene. */
    private static PaperCard displayPrinting(PaperCard pc) {
        if (regular(pc)) {
            return pc;
        }
        PaperCard best = null;
        java.util.Date bestDate = null;
        try {
            for (PaperCard other : StaticData.instance().getCommonCards().getAllCards(pc.getName())) {
                if (!regular(other)) {
                    continue;
                }
                CardEdition ed = StaticData.instance().getCardEdition(other.getEdition());
                java.util.Date d = ed == null ? null : ed.getDate();
                // Kernsets und Erweiterungen vor Reprint-/Commander-Sets, dann der neueste
                boolean main = ed != null && (ed.getType() == CardEdition.Type.CORE || ed.getType() == CardEdition.Type.EXPANSION);
                boolean bestMain = best != null && isMain(best);
                if (best == null || (main && !bestMain)
                        || (main == bestMain && d != null && (bestDate == null || d.after(bestDate)))) {
                    best = other;
                    bestDate = d;
                }
            }
        } catch (RuntimeException e) {
            return pc;
        }
        return best != null ? best : pc;
    }

    private static boolean isMain(PaperCard pc) {
        CardEdition ed = StaticData.instance().getCardEdition(pc.getEdition());
        return ed != null && (ed.getType() == CardEdition.Type.CORE || ed.getType() == CardEdition.Type.EXPANSION);
    }

    /** Pfad des Bilds ({@code art} = Bildausschnitt) von {@code pc}, oder "" (Download angestoßen). */
    private static String resolve(PaperCard pc, String face, boolean art) {
        String key = pc.getName() + "|" + pc.getEdition() + "|" + pc.getArtIndex() + "|" + face + (art ? "|art" : "");
        String known = FOUND.get(key);
        if (known != null && !known.isEmpty()) {
            return known;
        }
        String rel;
        try {
            rel = ImageUtil.getImageRelativePath(pc, face, true, false);
        } catch (RuntimeException e) {
            return "";
        }
        if (rel == null) {
            return "";
        }
        for (File f : candidates(rel, art)) {
            if (f.isFile() && f.length() > 0) {
                FOUND.put(key, f.getAbsolutePath());
                return f.getAbsolutePath();
            }
        }
        if (REQUESTED.add(key)) {
            File target = new File(ForgeConstants.CACHE_CARD_PICS_DIR, (art ? artcrop(rel) : fullborder(rel)) + ".jpg");
            String url;
            try {
                // Scryfalls Set-Code (weicht bei manchen Editionen von Forges Code ab); ohne
                // Edition wie bisher Forges Code
                CardEdition ed = StaticData.instance().getCardEdition(pc.getEdition());
                String scryfall = ed == null ? null : ed.getScryfallCode();
                url = ForgeConstants.URL_PIC_SCRYFALL_DOWNLOAD
                        + ImageUtil.getScryfallDownloadUrl(pc, face.isEmpty() ? "front" : face, scryfall, "en", art);
            } catch (RuntimeException e) {
                return "";
            }
            enqueue(url, target, key);
        }
        return "";
    }

    /** Download in die Warteschlange; danach (gedrosselt) den neuen Zustand an die UI schicken. */
    private static void enqueue(String url, File target, String key) {
        QUEUED.incrementAndGet();
        DOWNLOADER.submit(() -> {
            boolean ok = download(url, target, key);
            int left = QUEUED.decrementAndGet();
            // nach dem letzten Bild einer Reihe immer; bei langen Reihen zusätzlich alle
            // NOTIFY_EVERY_MS, damit die Bilder nach und nach erscheinen statt alle am Schluss
            long now = System.currentTimeMillis();
            long last = LAST_NOTIFY.get();
            if (left == 0) {
                // immer: ein paralleler Zwischenstand kann vor dem letzten Bild entstanden sein
                LAST_NOTIFY.set(now);
                BridgeCore.imagesChanged();
            } else if (ok && now - last >= NOTIFY_EVERY_MS && LAST_NOTIFY.compareAndSet(last, now)) {
                BridgeCore.imagesChanged();
            }
        });
    }

    /** „SET/Name.full“ → „SET/Name.artcrop“ (Bildausschnitt, Name wie bei NeoForge). */
    private static String artcrop(String rel) {
        return rel.endsWith(".full") ? rel.substring(0, rel.length() - ".full".length()) + ".artcrop" : rel + ".artcrop";
    }

    /** „SET/Name.full“ → „SET/Name.fullborder“ (Forges Name für Scryfall-Bilder mit Rand). */
    private static String fullborder(String rel) {
        return rel.endsWith(".full") ? rel.substring(0, rel.length() - ".full".length()) + ".fullborder" : rel;
    }

    /** Mögliche Dateien für {@code rel} in allen Bildordnern (mit und ohne Set-Ordner). */
    private static List<File> candidates(String rel, boolean art) {
        List<File> dirs = new ArrayList<>();
        dirs.add(new File(ForgeConstants.CACHE_CARD_PICS_DIR));
        String extra = System.getenv("CLASH_EXTRA_PICS");
        if (extra != null && !extra.isBlank()) {
            for (String d : extra.split(File.pathSeparator)) {
                dirs.add(new File(d));
            }
        } else {
            dirs.add(new File(System.getProperty("user.home"), "Games/NeoForge/datos/cache/pics/cards"));
        }
        String setless = rel.contains("/") ? rel.substring(rel.indexOf('/') + 1) : rel;
        List<File> out = new ArrayList<>();
        for (File d : dirs) {
            List<String> names = art ? List.of(artcrop(rel), artcrop(setless))
                    : List.of(fullborder(rel), rel, fullborder(setless), setless);
            for (String r : names) {
                out.add(new File(d, r + ".jpg"));
                out.add(new File(d, r + ".png"));
            }
        }
        return out;
    }

    /**
     * Lädt das Bild der API-Adresse {@code url} nach {@code target}: zuerst direkt von der CDN,
     * wenn Forges Index der Edition die Karte kennt ({@link #cdnFor}), sonst über die API im
     * Takt von {@link ScryfallRateLimiter} (10 Anfragen je Sekunde, mit Pause nach einem 429).
     *
     * @return ob das Bild jetzt da ist
     */
    private static boolean download(String url, File target, String key) {
        String cdn = cdnFor(url);
        if (cdn != null && fetch(cdn, target, key)) {
            return true;
        }
        if (ScryfallRateLimiter.shouldSkip(url)) {
            return false;
        }
        ScryfallRateLimiter.acquire(url);
        return fetch(url, target, key);
    }

    /** Eine Adresse in eine Datei (über eine Teildatei, damit nie ein halbes Bild gelesen wird). */
    private static boolean fetch(String url, File target, String key) {
        try {
            HttpRequest req = HttpRequest.newBuilder(URI.create(url))
                    .timeout(Duration.ofSeconds(30))
                    .header("User-Agent", "MTGClash/0.1 (Forge bridge)")
                    .header("Accept", "image/*")
                    .GET()
                    .build();
            HttpResponse<InputStream> resp = HTTP.send(req, HttpResponse.BodyHandlers.ofInputStream());
            if (resp.statusCode() != 200) {
                resp.body().close();
                ScryfallRateLimiter.noteIfRateLimited(resp.statusCode(), url,
                        resp.headers().firstValue("Retry-After").orElse(null));
                System.err.println("Kartenbild nicht ladbar (" + resp.statusCode() + "): " + url);
                return false;
            }
            Files.createDirectories(target.getParentFile().toPath());
            Path tmp = Files.createTempFile(target.getParentFile().toPath(), "dl", ".part");
            try (InputStream in = resp.body()) {
                Files.copy(in, tmp, StandardCopyOption.REPLACE_EXISTING);
            }
            Files.move(tmp, target.toPath(), StandardCopyOption.REPLACE_EXISTING);
            FOUND.put(key, target.getAbsolutePath());
            return true;
        } catch (IOException e) {
            System.err.println("Kartenbild-Download fehlgeschlagen: " + url + " – " + e.getMessage());
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
        }
        return false;
    }

    /**
     * Die CDN-Adresse ({@code cards.scryfall.io}, ohne Anfrage-Limit) zur API-Bildadresse
     * {@code .../cards/<set>/<nr>/<sprache>?format=image&version=<größe>[&face=back]}, aus Forges
     * {@link CdnUuidCache}; sonst null. Kennt Forge die Edition noch nicht, holt
     * {@code getCdnUrl} ihren Index im Hintergrund (eine Suche je ~175 Karten statt einer Anfrage
     * je Bild): die ersten Bilder kommen dann noch über die API, die folgenden von der CDN. Der
     * Index liegt danach in Forges Cache und gilt auch beim nächsten Start.
     */
    static String cdnFor(String apiUrl) {
        String prefix = ForgeConstants.URL_PIC_SCRYFALL_DOWNLOAD;
        int q = apiUrl == null ? -1 : apiUrl.indexOf('?');
        if (q < 0 || !apiUrl.startsWith(prefix) || !apiUrl.contains("format=image")) {
            return null;
        }
        String[] parts = apiUrl.substring(prefix.length(), q).split("/", -1);
        if (parts.length < 2 || parts[0].isEmpty() || parts[1].isEmpty()) {
            return null;
        }
        String lang = parts.length > 2 && !parts[2].isEmpty() ? parts[2] : "en";
        String size = "normal";
        String face = "front";
        for (String p : apiUrl.substring(q + 1).split("&")) {
            if (p.startsWith("version=")) {
                size = p.substring("version=".length());
            } else if (p.equals("face=back")) {
                face = "back";
            }
        }
        if (!size.equals("normal") && !size.equals("art_crop")) {
            return null;
        }
        try {
            return CdnUuidCache.getCdnUrl(parts[0], java.net.URLDecoder.decode(parts[1],
                    java.nio.charset.StandardCharsets.UTF_8), lang, face, size);
        } catch (RuntimeException e) {
            return null;
        }
    }
}
