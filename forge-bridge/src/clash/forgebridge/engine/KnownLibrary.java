package clash.forgebridge.engine;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;

import forge.game.Game;
import forge.game.card.Card;
import forge.game.event.GameEvent;
import forge.game.player.Player;
import forge.game.player.PlayerView;
import forge.game.zone.ZoneType;

/**
 * Was ein Spieler über die Karten oben auf einer Bibliothek weiß (wie in Arena: bekannte Karten
 * liegen dort offen). Forge merkt sich das nicht; die Brücke führt es selbst:
 * <ul>
 * <li>Karten, die ein Spieler selbst in eine Reihenfolge zurückgelegt hat (Sensei's Divining Top,
 * Brainstorm, Ponder …: {@code orderMoveToZoneList} in die Bibliothek) oder bei Hellsicht oben
 * gelassen hat ({@link #learn});</li>
 * <li>Karten, die aus einer offenen Zone (Spielfeld, Friedhof, offenes Exil, Stack,
 * Kommandozone) auf eine Bibliothek gelegt wurden – z. B. Sensei's Divining Top selbst –, kennen
 * alle; aus der eigenen Hand nur ihr Besitzer ({@link #onEvent});</li>
 * <li>Mischen löscht das Wissen über diese Bibliothek.</li>
 * </ul>
 * Gezeigt werden nur zusammenhängend von oben bekannte Karten ({@link #knownTop}); zieht man,
 * rückt die nächste bekannte nach. Karten behalten in Forge ihre ID, solange sie in der
 * Bibliothek liegen.
 */
final class KnownLibrary {
    private KnownLibrary() {}

    /** Spieler (ID seiner Ansicht) → IDs der Bibliothekskarten, die er kennt. */
    private static final Map<Integer, Set<Integer>> KNOWN = new ConcurrentHashMap<>();
    private static volatile Game game;

    /** Neue Partie: altes Wissen verwerfen. */
    static void startGame(Game g) {
        if (game != g) {
            game = g;
            KNOWN.clear();
        }
    }

    /** {@code viewer} kennt diese Karten (er hat sie gerade gesehen und zurückgelegt). */
    static void learn(Player viewer, Iterable<Card> cards) {
        if (viewer == null || cards == null) {
            return;
        }
        Set<Integer> set = KNOWN.computeIfAbsent(viewer.getId(), k -> ConcurrentHashMap.newKeySet());
        for (Card c : cards) {
            set.add(c.getId());
        }
    }

    /** Zonenwechsel in eine Bibliothek bzw. Mischen auswerten (aus dem Ereignis-Strom). */
    static void onEvent(Game g, GameEvent ev) {
        startGame(g);
        if (ev instanceof forge.game.event.GameEventCardChangeZone e && e.card() != null
                && e.to() != null && e.to().zoneType() == ZoneType.Library && e.from() != null) {
            int id = e.card().getId();
            switch (e.from().zoneType()) {
                case Battlefield, Graveyard, Stack, Command, Ante -> {
                    for (Player p : g.getPlayers()) {
                        KNOWN.computeIfAbsent(p.getId(), k -> ConcurrentHashMap.newKeySet()).add(id);
                    }
                }
                case Exile -> {
                    if (!e.card().isFaceDown()) {
                        for (Player p : g.getPlayers()) {
                            KNOWN.computeIfAbsent(p.getId(), k -> ConcurrentHashMap.newKeySet()).add(id);
                        }
                    }
                }
                case Hand -> {
                    PlayerView owner = e.from().player();
                    if (owner != null) {
                        KNOWN.computeIfAbsent(owner.getId(), k -> ConcurrentHashMap.newKeySet()).add(id);
                    }
                }
                default -> { }
            }
        } else if (ev instanceof forge.game.event.GameEventShuffle e && e.player() != null) {
            for (Player p : g.getPlayers()) {
                if (p.getView() == e.player() || p.getId() == e.player().getId()) {
                    for (Card c : p.getCardsIn(ZoneType.Library)) {
                        for (Set<Integer> s : KNOWN.values()) {
                            s.remove(c.getId());
                        }
                    }
                }
            }
        }
    }

    /**
     * Die obersten Karten der Bibliothek von {@code owner}, die {@code viewer} kennt bzw. sehen
     * darf (Forges {@code canBeShownTo}: Mystic Forge, aufgedeckt …), von oben, bis zur ersten
     * unbekannten.
     */
    static List<Card> knownTop(Player owner, Player viewer) {
        List<Card> out = new ArrayList<>();
        if (owner == null || viewer == null) {
            return out;
        }
        Set<Integer> known = KNOWN.getOrDefault(viewer.getId(), Set.of());
        for (Card c : owner.getCardsIn(ZoneType.Library)) {
            if (known.contains(c.getId()) || c.getView().canBeShownTo(viewer.getView())) {
                out.add(c);
            } else {
                break;
            }
        }
        return out;
    }
}
