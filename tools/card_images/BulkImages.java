import java.io.File;
import java.io.InputStream;
import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.nio.file.Files;
import java.nio.file.StandardCopyOption;
import java.time.Duration;
import java.util.ArrayList;
import java.util.Date;
import java.util.EnumSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;

import forge.StaticData;
import forge.card.CardEdition;
import forge.gui.GuiBase;
import forge.gui.download.CdnUuidCache;
import forge.item.PaperCard;
import forge.item.PaperToken;
import forge.localinstance.properties.ForgeConstants;
import forge.util.ImageUtil;
import forge.util.ScryfallRateLimiter;

/**
 * Lädt für NeoForge mit dem 3D-Tisch alle Bilder vorab, die der Tisch sonst erst beim Ausspielen
 * holt (dann zeigt er so lange die gezeichnete Kachel):
 *
 * <ul>
 * <li>Bildausschnitte ({@code art_crop}) für die Kacheln auf dem Spielfeld, je Karte für den Druck,
 * den die Brücke anzeigt (normaler Rahmen bevorzugt, wie {@code CardImages.displayPrinting}),
 * auch für Rückseiten: {@code pics/cards/<SET>/<Name>.artcrop.jpg};</li>
 * <li>Bilder aller Spielsteine: {@code pics/tokens/<Datei>.jpg}.</li>
 * </ul>
 *
 * <p>Dateinamen und Adressen genau wie {@code clash.forgebridge.engine.CardImages}. Bilder von
 * Scryfalls CDN parallel (ohne Limit), Anfragen an die API nacheinander über Forges
 * {@link ScryfallRateLimiter}. Vorhandene Dateien werden übersprungen (Neustart setzt fort).
 * Start über {@code tools/neoforge_images.sh}, nicht während NeoForge läuft.
 */
public class BulkImages {
    static final AtomicInteger done = new AtomicInteger(), ok = new AtomicInteger(),
            missing = new AtomicInteger(), failed = new AtomicInteger();
    static final String UA = "MTGClash/0.1 (Forge bridge)";
    static HttpClient http;
    static int total;

    /** Editionsarten mit normalem Rahmen (wie CardImages.REGULAR_TYPES). */
    static final EnumSet<CardEdition.Type> REGULAR_TYPES = EnumSet.of(
            CardEdition.Type.CORE, CardEdition.Type.EXPANSION, CardEdition.Type.REPRINT,
            CardEdition.Type.COMMANDER, CardEdition.Type.DRAFT, CardEdition.Type.STARTER,
            CardEdition.Type.DUEL_DECK);

    public static void main(String[] args) throws Exception {
        int threads = args.length > 0 ? Integer.parseInt(args[0]) : 16;
        GuiBase.setInterface(new forge.neo.platform.NeoGuiBase(new forge.neo.platform.ConsoleUiDispatcher()));
        forge.neo.NeoBoot.loadEngine(null);

        // Ziel -> API-Adresse; CDN-Adresse wird beim Laden bestimmt
        Map<File, String> files = new LinkedHashMap<>();
        int crops = 0, tokens = 0;
        for (PaperCard pc : shownPrintings()) {
            List<String> faces = new ArrayList<>(List.of(""));
            if (pc.hasBackFace()) {
                faces.add("back");
            }
            for (String face : faces) {
                String rel;
                try {
                    rel = ImageUtil.getImageRelativePath(pc, face, true, false);
                } catch (RuntimeException e) {
                    continue;
                }
                if (rel == null) {
                    continue;
                }
                File target = new File(ForgeConstants.CACHE_CARD_PICS_DIR, artcrop(rel) + ".jpg");
                if (target.isFile() && target.length() > 0) {
                    continue;
                }
                try {
                    CardEdition ed = StaticData.instance().getCardEdition(pc.getEdition());
                    String url = ForgeConstants.URL_PIC_SCRYFALL_DOWNLOAD + ImageUtil.getScryfallDownloadUrl(pc,
                            face.isEmpty() ? "front" : face, ed == null ? null : ed.getScryfallCode(), "en", true);
                    if (files.putIfAbsent(target, url) == null) {
                        crops++;
                    }
                } catch (RuntimeException ignored) {
                    // ohne Adresse nichts zu laden
                }
            }
        }
        // Forge liest die Spielsteine erst bei Bedarf ein
        StaticData.instance().getAllTokens().preloadTokens();
        for (PaperToken pt : StaticData.instance().getAllTokens().getAllTokens()) {
            try {
                String file = pt.getImageFilename(1);
                CardEdition ed = StaticData.instance().getCardEdition(pt.getEdition());
                String cn = pt.getCollectorNumber();
                if (file == null || file.isEmpty() || ed == null || cn == null || cn.isEmpty()) {
                    continue;
                }
                File target = new File(ForgeConstants.CACHE_TOKEN_PICS_DIR, file + ".jpg");
                if ((target.isFile() && target.length() > 0)
                        || new File(ForgeConstants.CACHE_TOKEN_PICS_DIR, file + ".png").isFile()) {
                    continue;
                }
                String url = ForgeConstants.URL_PIC_SCRYFALL_DOWNLOAD
                        + ImageUtil.getScryfallTokenDownloadUrl(cn, ed.getTokensCode().toLowerCase(), "en", "");
                if (files.putIfAbsent(target, url) == null) {
                    tokens++;
                }
            } catch (RuntimeException ignored) {
                // Spielstein ohne Druck
            }
        }
        total = files.size();
        System.out.println("[bilder] fehlend: " + total + " (Bildausschnitte " + crops + ", Spielsteine " + tokens + ")");

        http = HttpClient.newBuilder().version(HttpClient.Version.HTTP_1_1)
                .connectTimeout(Duration.ofSeconds(20)).followRedirects(HttpClient.Redirect.NORMAL).build();
        ExecutorService pool = Executors.newFixedThreadPool(threads);
        List<Map.Entry<File, String>> viaApi = new ArrayList<>();
        for (Map.Entry<File, String> e : files.entrySet()) {
            String cdn = cdnFor(e.getValue());
            if (cdn != null) {
                pool.submit(() -> fetch(e.getKey(), cdn, e.getValue()));
            } else {
                viaApi.add(e);
            }
        }
        // ohne CDN-Adresse: über die API, im Takt von Forges Ratenbegrenzer
        Thread api = new Thread(() -> {
            for (Map.Entry<File, String> e : viaApi) {
                ScryfallRateLimiter.awaitCooldownCleared(() -> false, s -> { });
                ScryfallRateLimiter.acquire(e.getValue());
                fetch(e.getKey(), e.getValue(), null);
            }
        });
        System.out.println("[bilder] über die API (langsam): " + viaApi.size());
        api.start();
        pool.shutdown();
        pool.awaitTermination(7, TimeUnit.DAYS);
        api.join();
        System.out.println("[bilder] FERTIG: geladen " + ok + ", nicht vorhanden " + missing + ", Fehler " + failed);
        System.exit(0);
    }

    /** Je Karte und Druck der Druck, den die Brücke anzeigt (ohne Dubletten). */
    static List<PaperCard> shownPrintings() {
        Map<String, PaperCard> out = new LinkedHashMap<>();
        Map<String, PaperCard> bestByName = new java.util.HashMap<>();
        for (PaperCard pc : StaticData.instance().getCommonCards().getAllCards()) {
            PaperCard shown = regular(pc) ? pc
                    : bestByName.computeIfAbsent(pc.getName(), n -> newestRegular(pc));
            out.putIfAbsent(shown.getName() + "|" + shown.getEdition() + "|" + shown.getArtIndex(), shown);
        }
        return new ArrayList<>(out.values());
    }

    static boolean regular(PaperCard pc) {
        CardEdition ed = StaticData.instance().getCardEdition(pc.getEdition());
        if (ed == null || !REGULAR_TYPES.contains(ed.getType()) || "PLST".equals(ed.getCode())) {
            return false;
        }
        String section = ed.getSectionForCollectorNumber(pc.getCollectorNumber());
        return section == null || section.isEmpty() || section.equalsIgnoreCase("cards");
    }

    static boolean isMain(PaperCard pc) {
        CardEdition ed = StaticData.instance().getCardEdition(pc.getEdition());
        return ed != null && (ed.getType() == CardEdition.Type.CORE || ed.getType() == CardEdition.Type.EXPANSION);
    }

    /** Wie CardImages.displayPrinting für einen Druck ohne normalen Rahmen. */
    static PaperCard newestRegular(PaperCard pc) {
        PaperCard best = null;
        Date bestDate = null;
        for (PaperCard other : StaticData.instance().getCommonCards().getAllCards(pc.getName())) {
            if (!regular(other)) {
                continue;
            }
            CardEdition ed = StaticData.instance().getCardEdition(other.getEdition());
            Date d = ed == null ? null : ed.getDate();
            boolean main = isMain(other);
            boolean bestMain = best != null && isMain(best);
            if (best == null || (main && !bestMain)
                    || (main == bestMain && d != null && (bestDate == null || d.after(bestDate)))) {
                best = other;
                bestDate = d;
            }
        }
        return best != null ? best : pc;
    }

    static String artcrop(String rel) {
        return rel.endsWith(".full") ? rel.substring(0, rel.length() - ".full".length()) + ".artcrop" : rel + ".artcrop";
    }

    /** CDN-Adresse zur API-Bildadresse (wie CardImages.cdnFor), sonst null. */
    static String cdnFor(String apiUrl) {
        String prefix = ForgeConstants.URL_PIC_SCRYFALL_DOWNLOAD;
        int q = apiUrl.indexOf('?');
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
        try {
            return CdnUuidCache.getCdnUrl(parts[0], java.net.URLDecoder.decode(parts[1],
                    java.nio.charset.StandardCharsets.UTF_8), lang, face, size);
        } catch (RuntimeException e) {
            return null;
        }
    }

    /** Laden mit Wiederholung bei Verbindungsfehlern; von der CDN zuerst, sonst über die API. */
    static void fetch(File target, String url, String apiFallback) {
        try {
            int code = get(url, target);
            if (code != 200 && apiFallback != null && !ScryfallRateLimiter.shouldSkip(apiFallback)) {
                ScryfallRateLimiter.acquire(apiFallback);
                code = get(apiFallback, target);
            }
            if (code == 200) {
                ok.incrementAndGet();
            } else if (code == 404) {
                missing.incrementAndGet();
            } else {
                failed.incrementAndGet();
                System.out.println("  HTTP " + code + ": " + url);
            }
        } catch (Exception ex) {
            failed.incrementAndGet();
            System.out.println("  Fehler " + url + ": " + ex);
        }
        int n = done.incrementAndGet();
        if (n % 500 == 0 || n == total) {
            System.out.println("[bilder] " + n + "/" + total + " (ok " + ok + ", fehlt " + missing + ", Fehler " + failed + ")");
        }
    }

    static int get(String url, File target) throws Exception {
        for (int attempt = 0; ; attempt++) {
            try {
                HttpRequest req = HttpRequest.newBuilder(URI.create(url)).timeout(Duration.ofSeconds(60))
                        .header("User-Agent", UA).header("Accept", "image/*").GET().build();
                HttpResponse<InputStream> r = http.send(req, HttpResponse.BodyHandlers.ofInputStream());
                if (r.statusCode() != 200) {
                    r.body().close();
                    ScryfallRateLimiter.noteIfRateLimited(r.statusCode(), url, r.headers().firstValue("Retry-After").orElse(null));
                    return r.statusCode();
                }
                target.getParentFile().mkdirs();
                File tmp = new File(target.getPath() + ".part");
                try (InputStream in = r.body()) {
                    Files.copy(in, tmp.toPath(), StandardCopyOption.REPLACE_EXISTING);
                }
                Files.move(tmp.toPath(), target.toPath(), StandardCopyOption.REPLACE_EXISTING);
                return 200;
            } catch (java.io.IOException ex) {
                // Netz weg o. ä.: warten (bis 60 s) und erneut
                if (attempt >= 30) {
                    throw ex;
                }
                Thread.sleep(Math.min(60_000, 2_000L << Math.min(attempt, 5)));
            }
        }
    }
}
