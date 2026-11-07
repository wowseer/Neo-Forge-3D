package clash.forgebridge.engine;

import com.google.common.eventbus.Subscribe;
import com.google.gson.JsonObject;

import forge.game.Game;
import forge.game.event.GameEvent;

/**
 * Leitet jedes Forge-{@link GameEvent} als {"type":"event"} an den Client weiter:
 * "event" = Klassenname (z. B. GameEventCardChangeZone), "text" = Forges Beschreibung,
 * dazu Runde und Phase. Für Effekte der UI zusätzlich strukturierte Felder:
 * <ul>
 * <li>{@code GameEventCardTapped}: {@code card}, {@code tapped}</li>
 * <li>{@code GameEventManaPool}: {@code player} (Sitzplatz), {@code mode} (Added/Removed/…),
 * {@code colors} (Buchstaben WUBRGC)</li>
 * <li>{@code GameEventCardDamaged}: {@code card}, {@code source}, {@code amount}</li>
 * <li>{@code GameEventPlayerDamaged}: {@code player} (Sitzplatz), {@code source}, {@code amount},
 * {@code combat}</li>
 * </ul>
 */
final class EventForwarder {
    private final Game game;

    EventForwarder(Game game) {
        this.game = game;
    }

    @Subscribe
    public void onEvent(GameEvent ev) {
        JsonObject m = BridgeCore.message("event");
        m.addProperty("event", ev.getClass().getSimpleName());
        m.addProperty("text", String.valueOf(ev));
        // vor dem ersten Zug gibt es noch keinen Phasen-Ablauf (z. B. Mana-Splitter, die das
        // Adventure zu Beginn vergibt): dann Runde 0 ohne Phase
        var ph = game.getPhaseHandler();
        m.addProperty("turn", ph == null ? 0 : ph.getTurn());
        m.addProperty("phase", ph == null || ph.getPhase() == null ? null : ph.getPhase().name());
        addDetails(m, ev);
        BridgeCore.send(m);
        // KI-Züge sichtbar machen: nach Aktionen der KI kurz warten (siehe AiPace)
        AiPace.after(game, ev);
        // Eingebettet startet der Gastgeber die Partie, also meldet sie hier ihr Ende
        if (BridgeCore.isEmbedded() && ev instanceof forge.game.event.GameEventGameOutcome) {
            BridgeCore.sendGameOver(game);
        }
    }

    /** Strukturierte Felder für die Ereignisse, auf die die UI mit Effekten reagiert. */
    private void addDetails(JsonObject m, GameEvent ev) {
        if (ev instanceof forge.game.event.GameEventCardTapped e && e.card() != null) {
            m.addProperty("card", e.card().getId());
            m.addProperty("tapped", e.tapped());
        } else if (ev instanceof forge.game.event.GameEventManaPool e) {
            m.addProperty("player", seat(e.player()));
            m.addProperty("mode", String.valueOf(e.mode()));
            StringBuilder colors = new StringBuilder();
            if (e.colors() != null) {
                for (forge.card.MagicColor.Color c : e.colors()) {
                    colors.append(c == forge.card.MagicColor.Color.COLORLESS ? "C" : c.getShortName());
                }
            }
            m.addProperty("colors", colors.toString());
        } else if (ev instanceof forge.game.event.GameEventCardDamaged e) {
            m.addProperty("card", e.card() == null ? -1 : e.card().getId());
            m.addProperty("source", e.source() == null ? -1 : e.source().getId());
            m.addProperty("amount", e.amount());
        } else if (ev instanceof forge.game.event.GameEventPlayerDamaged e) {
            m.addProperty("player", seat(e.target()));
            m.addProperty("source", e.source() == null ? -1 : e.source().getId());
            m.addProperty("amount", e.amount());
            m.addProperty("combat", e.combat());
        }
    }

    /** Sitzplatz eines Spielers aus seiner Ansicht (-1 = unbekannt). */
    private int seat(forge.game.player.PlayerView view) {
        if (view == null) {
            return -1;
        }
        for (forge.game.player.Player p : game.getRegisteredPlayers()) {
            if (p.getView() == view || p.getId() == view.getId()) {
                return StateJson.indexOf(game, p);
            }
        }
        return -1;
    }
}
