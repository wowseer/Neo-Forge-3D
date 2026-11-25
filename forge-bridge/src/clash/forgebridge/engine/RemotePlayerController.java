package clash.forgebridge.engine;

import java.util.ArrayList;
import java.util.Collection;
import java.util.List;
import java.util.HashMap;
import java.util.Map;
import java.util.TreeSet;
import java.util.function.Predicate;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;

import com.google.common.collect.ListMultimap;

import forge.LobbyPlayer;
import forge.StaticData;
import forge.card.ColorSet;
import forge.card.ICardFace;
import forge.card.MagicColor;
import forge.ai.ComputerUtilMana;
import forge.ai.PlayerControllerAi;
import forge.game.Game;
import forge.game.GameEntity;
import forge.game.spellability.TargetChoices;
import forge.game.card.CardLists;
import forge.game.spellability.SpellAbilityStackInstance;
import forge.game.GameObject;
import forge.game.card.Card;
import forge.game.card.CardState;
import forge.game.card.CardCollection;
import forge.game.card.CardCollectionView;
import forge.game.card.CounterType;
import forge.game.keyword.Keyword;
import forge.game.player.DelayedReveal;
import forge.util.collect.FCollectionView;
import org.apache.commons.lang3.tuple.ImmutablePair;
import forge.game.combat.Combat;
import forge.game.combat.CombatUtil;
import forge.game.cost.Cost;
import forge.game.keyword.KeywordInterface;
import forge.game.player.Player;
import forge.game.player.PlayerActionConfirmMode;
import forge.game.player.PlaySpellAbility;
import forge.game.cost.CostPartMana;
import forge.game.cost.CostPayment;
import forge.game.cost.CostDecisionMakerBase;
import forge.game.mana.ManaConversionMatrix;
import forge.game.mana.ManaCostBeingPaid;
import forge.card.mana.ManaCost;
import forge.ai.AiCostDecision;
import forge.ai.ComputerUtilCost;
import forge.game.replacement.ReplacementEffect;
import forge.game.spellability.AbilitySub;
import forge.game.spellability.OptionalCostValue;
import forge.game.spellability.SpellAbility;
import forge.game.staticability.StaticAbility;
import forge.game.trigger.WrappedAbility;
import forge.game.zone.ZoneType;

/**
 * Spielersteuerung, deren Hauptentscheidungen Neo Forge 3D trifft.
 *
 * <p>Jede Frage geht als {@code Decision} an den Client ({@link Decisions},
 * Protokoll v3): Priorität, Mulligan, beim Wirken optionale Kosten, Modi, X und Ziele, dazu alle
 * allgemeinen Auswahl-Dialoge, Angreifer, Blocker, Kartenauswahl, Scry, Reihenfolgen. Andere Kosten als Mana trifft vorerst
 * die Forge-KI, von der diese Klasse erbt. Antwortet der Client ungültig, entscheidet ebenfalls
 * die KI. Weitere Entscheidungen lassen sich ferngesteuert machen, indem man die passende Methode
 * von {@code PlayerController} überschreibt und die passende Methode von {@link Decisions} aufruft.
 */
public class RemotePlayerController extends PlayerControllerAi {
    private static final ZoneType[] PLAY_ZONES = {
        ZoneType.Hand, ZoneType.Battlefield, ZoneType.Graveyard, ZoneType.Exile, ZoneType.Command,
    };

    private final int seat;

    public RemotePlayerController(Game game, Player player, LobbyPlayer lobby) {
        super(game, player, lobby);
        this.seat = lobby instanceof RemoteLobbyPlayer r ? r.getSeat() : -1;
    }

    /**
     * Alle Fähigkeiten, die der Spieler jetzt spielen darf und bezahlen kann und für die es
     * genug mögliche Ziele gibt (ohne Manafähigkeiten – Mana wird beim Ausführen automatisch
     * bezahlt).
     */
    private List<SpellAbility> playableOptions() {
        Player me = getPlayer();
        List<SpellAbility> options = new ArrayList<>();
        for (ZoneType zone : PLAY_ZONES) {
            for (Card card : me.getCardsIn(zone)) {
                for (SpellAbility sa : card.getAllPossibleAbilities(me, true)) {
                    if (sa.isManaAbility()) {
                        continue;
                    }
                    sa.setActivatingPlayer(me);
                    if (!sa.isLandAbility() && !hasPossibleTargets(sa)) {
                        continue; // braucht ein Ziel, aber es gibt keins
                    }
                    if (sa.isLandAbility() || ComputerUtilMana.canPayManaCost(sa, me, 0, false)) {
                        options.add(sa);
                    }
                }
            }
        }
        return options;
    }

    /**
     * Priorität: {@code Decision::Priority} mit allen spielbaren Aktionen ({@code PlayLand},
     * {@code Cast}, {@code CastAlt}, {@code Activate} …) und {@code Pass}. Gibt es nichts zu
     * spielen, wird automatisch gepasst – außer in der eigenen zweiten Hauptphase und der
     * eigenen Endphase ({@link #mustStop()}): dort hält das Spiel immer an, bis der Spieler passt.
     * Sonst kommen die Manafähigkeiten als
     * {@code ActivateMana} dazu (Erweiterung, CR 605.3a): Der Spieler kann Mana vorab in den Pool
     * tappen und es danach beim Bezahlen selbst einsetzen; die Fähigkeit wird sofort ausgeführt
     * und erneut gefragt. Ist die gewählte Aktion nicht ausführbar (abgebrochen,
     * kein gültiges Ziel), kommt "rejected" und es wird erneut gefragt. Ob die KI den Zug für
     * sinnvoll hält, spielt keine Rolle.
     */
    /**
     * Pflicht-Halt: eigene zweite Hauptphase (CR 505.1) und eigene Endphase (CR 513.1) mit leerem
     * Stack werden nie automatisch übersprungen – der Spieler sieht den Wechsel dorthin und passt
     * selbst (Leertaste / „Passen“).
     */
    private boolean mustStop() {
        forge.game.phase.PhaseHandler ph = getGame().getPhaseHandler();
        forge.game.phase.PhaseType phase = ph.getPhase();
        return ph.isPlayerTurn(getPlayer())
                && getGame().getStack().isEmpty()
                && (phase == forge.game.phase.PhaseType.MAIN2 || phase == forge.game.phase.PhaseType.END_OF_TURN);
    }

    @Override
    public List<SpellAbility> chooseSpellAbilityToPlay() {
        while (true) {
            List<SpellAbility> options = playableOptions();
            if (options.isEmpty() && !mustStop()) {
                return null;
            }
            JsonArray actions = new JsonArray();
            List<String> labels = new ArrayList<>();
            JsonObject pass = new JsonObject();
            pass.addProperty("type", "Pass");
            actions.add(pass);
            labels.add("Passen");
            Map<String, Integer> counters = new HashMap<>();
            for (SpellAbility sa : options) {
                actions.add(actionJson(sa, counters));
                labels.add(sa.toUnsuppressedString());
            }
            ManaSources mana = manaSources();
            for (int i = 0; i < mana.sources.size(); i++) {
                JsonObject a = new JsonObject();
                a.addProperty("type", "ActivateMana");
                a.addProperty("card", mana.refs.get(i)[0]);
                a.addProperty("index", mana.refs.get(i)[1]);
                actions.add(a);
                labels.add(mana.labels.get(i));
            }
            // vorab getapptes, noch ungenutztes Mana: Quelle wieder enttappen (Undo-Stapel der Engine)
            List<Card> undoable = undoableManaSources();
            for (Card c : undoable) {
                JsonObject a = new JsonObject();
                a.addProperty("type", "UndoMana");
                a.addProperty("card", c.getId());
                actions.add(a);
                labels.add("Mana zurücknehmen: " + c.getName());
            }
            // Spielbar aus einer anderen Zone (Flashback, Foretell, Commander, Companion …):
            // die UI zeigt diese Karten zusätzlich im Handfächer
            // je Karte die Manakosten der Spielweise aus dieser Zone (Flashback, Escape …)
            List<Card> elsewhere = new ArrayList<>();
            List<String> elsewhereCosts = new ArrayList<>();
            for (SpellAbility sa : options) {
                Card host = sa.getHostCard();
                forge.game.zone.Zone z = host == null ? null : getGame().getZoneOf(host);
                if (z == null || elsewhere.contains(host)) {
                    continue;
                }
                ZoneType zt = z.getZoneType();
                if (zt != ZoneType.Hand && zt != ZoneType.Battlefield && zt != ZoneType.Stack) {
                    elsewhere.add(host);
                    String cost = "";
                    if (sa.getPayCosts() != null && sa.getPayCosts().getTotalMana() != null) {
                        forge.card.mana.ManaCost base = sa.getPayCosts().getTotalMana();
                        // Commander aus der Kommandozone: Commander-Steuer {2} je bisherigem Wirken
                        // (CR 903.8) direkt in den Kosten
                        if (zt == ZoneType.Command) {
                            base = StateJson.withCommanderTax(host, base);
                        }
                        cost = base.getShortString();
                    }
                    elsewhereCosts.add(cost == null ? "" : cost);
                }
            }
            JsonObject action = Decisions.priority(getGame(), seat, actions, labels, elsewhere, elsewhereCosts);
            if (action != null && action.has("type") && "Refresh".equals(action.get("type").getAsString())) {
                continue; // Debug-Werkzeug hat etwas geändert: neu fragen
            }
            int choice = action == null ? 0 : indexOf(actions, action);
            if (choice <= 0) {
                return null; // passen (oder unlesbare Antwort)
            }
            choice -= 1;
            if (choice >= options.size() + mana.sources.size()) {
                undoMana(undoable.get(choice - options.size() - mana.sources.size()));
                continue;
            }
            if (choice >= options.size()) {
                activateMana(mana.sources.get(choice - options.size()), false);
                continue;
            }
            SpellAbility sa = options.get(choice);
            lastChoice = choice;
            lastAction = action;
            List<SpellAbility> result = new ArrayList<>();
            result.add(sa);
            return result;
        }
    }

    /**
     * Tiefe des menschlichen Ablaufs ({@code PlaySpellAbility}): nur darin fragen Delve, Convoke
     * und Improvise den Spieler. Außerhalb (z. B. Spielbarkeits-Prüfungen der KI-Logik) entscheidet
     * die Engine, damit keine Frage ohne echtes Wirken erscheint.
     */
    private int humanFlow;

    /**
     * Der Spieler hat das Wirken bei einer Zusatzkosten-Frage mitten im Ablauf abgebrochen
     * (Casualty, Offspring …, {@link #addKeywordCost}): die nächste Zielwahl bzw. das Bezahlen
     * lehnt ab, Forge nimmt das Wirken dann vollständig zurück.
     */
    private boolean cancelCasting;

    /** Abbruch vor dem Stack (optionale Kosten wie Kicker): beendet {@link #playHuman} sofort. */
    private static final class CastCancelled extends RuntimeException {
        CastCancelled() {
            super(null, null, false, false);
        }
    }

    /**
     * Spielt {@code sa} über Forges menschlichen Ablauf; ein Abbruch des Spielers (vor dem Stack
     * oder mitten im Ablauf) ergibt {@code false}.
     */
    private boolean playHuman(SpellAbility sa) {
        cancelCasting = false;
        try {
            return human(() -> PlaySpellAbility.playSpellAbility(this, getPlayer(), sa));
        } catch (CastCancelled e) {
            return false;
        } finally {
            cancelCasting = false;
        }
    }

    private <T> T human(java.util.function.Supplier<T> body) {
        humanFlow++;
        try {
            return body.get();
        } finally {
            humanFlow--;
        }
    }

    /**
     * Delve (CR 702.66): Karten aus dem Friedhof exilieren, je eine verringert die generischen
     * Kosten um 1 → {@code ChooseCards} mit Grund {@code Delve} (Kartenfächer, 0 bis Anzahl).
     */
    @Override
    public CardCollectionView chooseCardsToDelve(int genericAmount, CardCollection grave) {
        int max = Math.min(genericAmount, grave.size());
        if (humanFlow == 0 || max <= 0) {
            return super.chooseCardsToDelve(genericAmount, grave);
        }
        CardCollection r = pickCards("Delve", grave, 0, max,
                "Delve: bis zu " + max + " Karten aus dem Friedhof exilieren (je {1} weniger)", null, false);
        return r != null ? r : super.chooseCardsToDelve(genericAmount, grave);
    }

    /**
     * Convoke (CR 702.51) / Improvise (CR 702.126) / Waterbend: ungetappte Kreaturen bzw.
     * Artefakte tappen, je eine bezahlt ein Mana → {@code ChooseCards} mit Grund {@code Convoke}
     * bzw. {@code Improvise} (auf dem Tisch anklicken). Welches Mana jede Karte bezahlt, legt die
     * Brücke fest: zuerst ein farbiges Symbol in einer Farbe der Kreatur, sonst generisch.
     */
    @Override
    public Map<Card, forge.card.mana.ManaCostShard> chooseCardsForConvokeOrImprovise(SpellAbility sa,
            forge.card.mana.ManaCost manaCost, CardCollectionView untappedCards, boolean artifacts,
            boolean creatures, Integer maxReduction) {
        int generic = manaCost.getGenericCost();
        List<forge.card.mana.ManaCostShard> colored = new ArrayList<>();
        if (creatures) {
            for (forge.card.mana.ManaCostShard sh : manaCost) {
                if (sh != forge.card.mana.ManaCostShard.GENERIC && sh != forge.card.mana.ManaCostShard.X
                        && sh != forge.card.mana.ManaCostShard.COLORED_X) {
                    colored.add(sh);
                }
            }
        }
        int max = Math.min(untappedCards.size(), generic + colored.size());
        if (maxReduction != null) {
            max = Math.min(max, maxReduction);
        }
        if (humanFlow == 0 || max <= 0) {
            return super.chooseCardsForConvokeOrImprovise(sa, manaCost, untappedCards, artifacts, creatures,
                    maxReduction);
        }
        String reason = creatures && !artifacts ? "Convoke" : artifacts && !creatures ? "Improvise" : "Choose";
        String what = creatures && !artifacts ? "Kreaturen" : artifacts && !creatures ? "Artefakte" : "Kreaturen/Artefakte";
        CardCollection chosen = pickCards(reason, untappedCards, 0, max,
                what + " tappen, um bis zu " + max + " Mana zu bezahlen (" + manaCost.getSimpleString() + ")", sa, false);
        if (chosen == null) {
            return super.chooseCardsForConvokeOrImprovise(sa, manaCost, untappedCards, artifacts, creatures,
                    maxReduction);
        }
        Map<Card, forge.card.mana.ManaCostShard> result = new java.util.LinkedHashMap<>();
        for (Card c : chosen) {
            forge.card.mana.ManaCostShard pay = null;
            if (c.isCreature()) {
                for (forge.card.mana.ManaCostShard sh : colored) {
                    boolean fits = false;
                    for (byte color : forge.card.MagicColor.WUBRG) {
                        if (c.getColor().hasAnyColor(color) && sh.canBePaidWithManaOfColor(color)) {
                            fits = true;
                        }
                    }
                    if (fits) {
                        pay = sh;
                        break;
                    }
                }
            }
            if (pay != null) {
                colored.remove(pay);
            } else if (generic > 0) {
                pay = forge.card.mana.ManaCostShard.GENERIC;
                generic--;
            } else {
                continue; // nichts mehr, was diese Karte bezahlen kann
            }
            result.put(c, pay);
        }
        return result;
    }

    /** Index und Aktion der zuletzt gewählten Priority-Option (für "rejected"). */
    private int lastChoice = -1;
    private JsonObject lastAction;

    private void sendRejected(int option, JsonObject action, String reason) {
        JsonObject rejected = BridgeCore.message("rejected");
        rejected.addProperty("player", seat);
        rejected.addProperty("option", option);
        rejected.add("action", action);
        rejected.addProperty("reason", reason);
        BridgeCore.send(rejected);
    }

    /**
     * Spielt die gewählte Fähigkeit über Forges Ablauf für menschliche Spieler
     * ({@code PlaySpellAbility.playSpellAbility}): Zusatz-/optionale Kosten, Modi, angesagte Werte,
     * X, Ziele und das Bezahlen fragen dabei diese Klasse ({@link #chooseOptionalCosts},
     * {@link #chooseModeForAbility}, {@link #announceRequirements}, {@link #chooseTargetsFor},
     * {@link #applyManaToCost}). Abgebrochen → "rejected", Forge fragt erneut nach Priorität.
     */
    @Override
    public boolean playChosenSpellAbility(SpellAbility sa) {
        boolean ok = playHuman(sa);
        if (!ok) {
            sendRejected(lastChoice, lastAction, "Abgebrochen oder kein gültiges Ziel");
        }
        return ok;
    }

    /**
     * Kosten-Entscheider beim Bezahlen: Karten, die als Kosten geopfert, abgeworfen, getappt oder
     * zurückgegeben werden, wählt der Spieler ({@link RemoteCostDecision}).
     */
    @Override
    public CostDecisionMakerBase getCostDecisionMaker(Player player, SpellAbility ability, boolean effect,
            String prompt) {
        return new RemoteCostDecision(seat, player, ability, effect, cancelCasting);
    }

    /** Manakosten: Forges Ablauf für menschliche Spieler, der {@link #applyManaToCost} fragt. */
    @Override
    public boolean payManaCost(ManaCost toPay, CostPartMana costPartMana, SpellAbility sa, String prompt,
            ManaConversionMatrix matrix, boolean effect) {
        return human(() -> PlaySpellAbility.payManaCost(this, toPay, costPartMana, sa, getPlayer(), prompt, matrix, effect));
    }

    /**
     * {@code Decision::PayMana}: bezahlt {@code toPay} interaktiv. Der Spieler wählt eine
     * Manaquelle ({@code ActivateMana}), schwebendes Mana einer Farbe aus dem Pool ({@code UsePool},
     * Erweiterung), lässt Forge automatisch bezahlen ({@code AutoPay}) oder bricht ab
     * ({@code Cancel} → Wirken wird zurückgenommen).
     */
    @Override
    public boolean applyManaToCost(ManaCostBeingPaid toPay, SpellAbility sa, String prompt,
            ManaConversionMatrix matrix, boolean effect) {
        if (cancelCasting) {
            return false; // Wirken wurde bei den Zusatzkosten abgebrochen
        }
        Player me = getPlayer();
        // Phyrexianisches Mana (CR 107.4f): je Symbol fragen, ob es mit 2 Leben bezahlt wird
        while (toPay.containsPhyrexianMana() && me.canPayLife(2, false, sa)) {
            Boolean life = Decisions.phyrexian(getGame(), seat, sa, toPay.toString());
            if (life == null || !life || !toPay.payPhyrexian()) {
                break;
            }
            me.payLife(2, sa, false);
        }
        while (true) {
            if (toPay.isPaid()) {
                return true;
            }
            // Pool-Mana, das einen Teil der Kosten bezahlen kann (je Farbe eine Option)
            List<Byte> poolColors = new ArrayList<>();
            for (byte color : forge.card.MagicColor.WUBRGC) {
                if (me.getManaPool().getAmountOfColor(color) > 0 && toPay.isAnyPartPayableWith(color, me.getManaPool())) {
                    poolColors.add(color);
                }
            }
            ManaSources mana = manaSources();
            String pick = Decisions.payMana(getGame(), seat, sa, toPay.toString(), mana.refs, mana.labels, poolColors);
            if (pick == null || pick.equals("Cancel")) {
                return false;
            }
            if (pick.startsWith("Pool:")) {
                byte color = Byte.parseByte(pick.substring(5));
                // nicht verwendbar (z. B. Einschränkung des Manas) → einfach erneut fragen
                me.getManaPool().tryPayCostWithColor(color, sa, toPay, new ArrayList<>());
                continue;
            }
            if (pick.equals("AutoPay")) {
                // automatisch: auch die Wahl des Pool-Manas trifft Forge (keine Rückfrage je Mana)
                autoPaying = true;
                try {
                    return ComputerUtilMana.payManaCost(toPay, sa, me, effect);
                } finally {
                    autoPaying = false;
                }
            }
            SpellAbility ma = mana.sources.get(Integer.parseInt(pick));
            if (activateMana(ma, effect)) {
                me.getManaPool().payManaFromAbility(sa, toPay, ma);
            }
        }
    }

    /**
     * Karten, deren Manafähigkeit gerade rückgängig gemacht werden kann (CR 605: Mana vorab
     * getappt und noch ungenutzt im Pool; Forges Undo-Stapel des Stacks).
     */
    private List<Card> undoableManaSources() {
        Player me = getPlayer();
        List<Card> result = new ArrayList<>();
        if (!getGame().getStack().canUndo(me)) {
            return result;
        }
        for (Card c : me.getCardsIn(ZoneType.Battlefield)) {
            if (getGame().getStack().filterUndoStackByHost(c).iterator().hasNext()) {
                result.add(c);
            }
        }
        return result;
    }

    /** Manafähigkeiten von {@code card} rückgängig machen: enttappen, Mana aus dem Pool nehmen. */
    private void undoMana(Card card) {
        List<SpellAbility> list = new ArrayList<>();
        getGame().getStack().filterUndoStackByHost(card).forEach(list::add);
        for (SpellAbility sa : list) {
            if (sa.undo()) {
                getGame().getStack().clearUndoStack(sa);
            }
        }
    }

    /** Manaquellen des Spielers: Fähigkeit, (Karten-ID, Index der Manafähigkeit), Beschriftung. */
    private record ManaSources(List<SpellAbility> sources, List<int[]> refs, List<String> labels) {
    }

    /** Alle Manafähigkeiten auf dem Spielfeld, die der Spieler jetzt aktivieren kann. */
    private ManaSources manaSources() {
        Player me = getPlayer();
        ManaSources m = new ManaSources(new ArrayList<>(), new ArrayList<>(), new ArrayList<>());
        for (Card c : me.getCardsIn(ZoneType.Battlefield)) {
            List<SpellAbility> abilities = new ArrayList<>(c.getManaAbilities());
            for (int i = 0; i < abilities.size(); i++) {
                SpellAbility ma = abilities.get(i);
                ma.setActivatingPlayer(me);
                if (ma.canPlay() && (!ma.getPayCosts().hasTapCost() || c.canTap())) {
                    m.sources.add(ma);
                    m.refs.add(new int[] {c.getId(), i});
                    m.labels.add(c.getName() + ": " + ma.toUnsuppressedString());
                }
            }
        }
        return m;
    }

    /**
     * Manafähigkeit ausführen (CR 605.3b: sie geht nicht auf den Stack): Kosten bezahlen (Tappen
     * …), dann auflösen – das Mana landet im Pool. {@code false}, wenn die Kosten nicht bezahlt
     * werden konnten.
     */
    private boolean activateMana(SpellAbility ma, boolean effect) {
        CostPayment pay = new CostPayment(ma.getPayCosts(), ma);
        if (!pay.payComputerCosts(new AiCostDecision(getPlayer(), ma, effect, true))) {
            return false;
        }
        getGame().getStack().addAndUnfreeze(ma);
        return true;
    }

    /**
     * {@code Action} zu einer spielbaren Fähigkeit: Land → {@code PlayLand} (weitere
     * Land-Seiten derselben Karte {@code PlayLandAlt}), Zauber → {@code Cast} (weitere Zauber-
     * Varianten derselben Karte {@code CastAlt}), sonst {@code Activate} mit laufendem Index je Karte.
     */
    private static JsonObject actionJson(SpellAbility sa, Map<String, Integer> counters) {
        int card = sa.getHostCard().getId();
        String kind = sa.isLandAbility() ? "land" : sa.isSpell() ? "spell" : "ability";
        int n = counters.merge(kind + card, 1, Integer::sum) - 1;
        JsonObject a = new JsonObject();
        switch (kind) {
            case "land" -> a.addProperty("type", n == 0 ? "PlayLand" : "PlayLandAlt");
            case "spell" -> a.addProperty("type", n == 0 ? "Cast" : "CastAlt");
            default -> a.addProperty("type", "Activate");
        }
        a.addProperty("card", card);
        if (n > 0 || kind.equals("ability")) {
            a.addProperty("index", n);
        }
        return a;
    }

    private static int indexOf(JsonArray arr, JsonObject o) {
        for (int i = 0; i < arr.size(); i++) {
            if (arr.get(i).equals(o)) {
                return i;
            }
        }
        return -1;
    }

    /**
     * Ziele für genau eine (Teil-)Fähigkeit: Liste der gültigen Ziele im Auswahlfenster (Optionen
     * tragen card_id / player). Wird von Forges {@code SpellAbility.setupTargets()} aufgerufen.
     * false = abgebrochen oder ungültig.
     */
    @Override
    public boolean chooseTargetsFor(SpellAbility cur) {
        if (cancelCasting) {
            return false; // Wirken wurde bei den Zusatzkosten abgebrochen
        }
        cur.resetTargets();
        List<GameObject> candidates = targetCandidates(cur, null);
        int min = cur.getMinTargets();
        int max = Math.min(cur.getMaxTargets(), candidates.size());
        if (candidates.isEmpty()) {
            return min == 0;
        }
        List<String> labels = new ArrayList<>();
        for (GameObject e : candidates) {
            labels.add(targetLabel(e));
        }
        String prompt = cur.getTargetRestrictions().getVTSelection();
        String description = "Ziel wählen" + (prompt == null || prompt.isBlank() ? "" : ": " + prompt);
        List<Integer> chosen = Decisions.targets(g(), seat, cur, description, candidates, labels, min, max);
        if (chosen == null) {
            return false;
        }
        for (int i : chosen) {
            GameObject e = candidates.get(i);
            if (!cur.canTarget(e)) {
                return false;
            }
            cur.getTargets().add(e);
        }
        if (!cur.isTargetNumberValid()) {
            return false;
        }
        // „… beliebig auf eine Anzahl Ziele aufgeteilt“ (CR 601.2d): Verteilung wählen
        if (cur.isDividedAsYouChoose() && cur.getDividedValue() != null) {
            int total = cur.getDividedValue();
            List<GameEntity> picked = new ArrayList<>();
            List<String> pickedLabels = new ArrayList<>();
            for (int i : chosen) {
                if (!(candidates.get(i) instanceof GameEntity ge)) {
                    return false; // Verteilen nur auf Karten/Spieler
                }
                picked.add(ge);
                pickedLabels.add(labels.get(i));
            }
            if (picked.size() == 1) {
                cur.addDividedAllocation(picked.get(0), total);
            } else {
                List<Integer> parts = Decisions.divide(getGame(), seat, cur, total, picked, pickedLabels, 1);
                if (parts == null) {
                    return false;
                }
                for (int i = 0; i < picked.size(); i++) {
                    cur.addDividedAllocation(picked.get(i), parts.get(i));
                }
            }
        }
        return true;
    }

    /**
     * Neue Ziele (Umleiten, Kopien; CR 115.7, 707.10c): wie {@link #chooseTargetsFor}, nur unter
     * Kandidaten, die {@code filter} erfüllen; {@code optional} = vorher fragen, ob die Ziele
     * geändert werden. Ändert die Ziele von {@code ability} und liefert sie; {@code null} = keine
     * Änderung (alte Ziele bleiben).
     */
    @Override
    public TargetChoices chooseNewTargetsFor(SpellAbility ability, java.util.function.Predicate<GameObject> filter,
            boolean optional) {
        if (ability.getTargetRestrictions() == null) {
            return null;
        }
        Card host = ability.getHostCard();
        if (optional) {
            Boolean yes = Decisions.confirm(g(), seat,
                    "Neue Ziele wählen für " + (host == null ? "die Kopie" : host.getName()) + "?", ability, host);
            if (yes == null || !yes) {
                return null;
            }
        }
        TargetChoices old = ability.getTargets();
        ability.resetTargets();
        List<GameObject> candidates = targetCandidates(ability, filter);
        int min = ability.getMinTargets();
        int max = Math.min(ability.getMaxTargets(), candidates.size());
        if (candidates.isEmpty() || max < min) {
            ability.setTargets(old);
            return null;
        }
        List<String> labels = new ArrayList<>();
        candidates.forEach(e -> labels.add(targetLabel(e)));
        List<Integer> chosen = Decisions.targets(g(), seat, ability, "Neue Ziele wählen", candidates, labels, min, max);
        if (chosen == null) {
            ability.setTargets(old);
            return null;
        }
        for (int i : chosen) {
            GameObject e = candidates.get(i);
            if (!ability.canTarget(e)) {
                ability.setTargets(old);
                return null;
            }
            ability.getTargets().add(e);
        }
        if (!ability.isTargetNumberValid()) {
            ability.setTargets(old);
            return null;
        }
        return ability.getTargets();
    }

    /** Erschöpfen (Exert, CR 701.43): welche Angreifer erschöpft werden (auf dem Tisch anklicken). */
    @Override
    public List<Card> exertAttackers(List<Card> attackers) {
        CardCollection r = attackers.isEmpty() ? null : pickCards("Choose", new CardCollection(attackers), 0,
                attackers.size(), "Erschöpfen (Exert): Angreifer wählen, die erschöpft werden", null, false);
        return r != null ? new ArrayList<>(r) : super.exertAttackers(attackers);
    }

    /** Rekrutieren (Enlist, CR 702.154): welche Angreifer rekrutieren (auf dem Tisch anklicken). */
    @Override
    public List<Card> enlistAttackers(List<Card> attackers) {
        CardCollection r = attackers.isEmpty() ? null : pickCards("Choose", new CardCollection(attackers), 0,
                attackers.size(), "Rekrutieren (Enlist): Angreifer wählen, die rekrutieren", null, false);
        return r != null ? new ArrayList<>(r) : super.enlistAttackers(attackers);
    }

    /** Einen Zauber bzw. eine Fähigkeit für einen Effekt wählen ({@code ChooseOption}). */
    @Override
    public SpellAbility chooseSingleSpellForEffect(List<SpellAbility> spells, SpellAbility sa, String title,
            Map<String, Object> params) {
        if (spells.size() <= 1) {
            return super.chooseSingleSpellForEffect(spells, sa, title, params);
        }
        // Nächsten Dungeon-Raum wählen (CR 309.5a): Option je Raum mit seinem Namen als Schlüssel,
        // Karte = der Dungeon (die UI zeigt ihn groß, die erreichbaren Räume leuchten)
        if (spells.stream().allMatch(s -> s.hasParam("RoomName"))) {
            List<Decisions.Opt> rooms = new ArrayList<>();
            List<Card> hosts = new ArrayList<>();
            for (SpellAbility s : spells) {
                Card host = s.getHostCard();
                String name = s.getParam("RoomName");
                String text = s.hasParam("SpellDescription") ? s.getParam("SpellDescription") : "";
                rooms.add(new Decisions.Opt(name + " — " + text, null, host == null ? -1 : host.getId(), name));
                if (host != null && !hosts.contains(host)) {
                    hosts.add(host);
                }
            }
            Integer i = Decisions.cardOption(getGame(), seat, title, rooms, hosts, null, "DungeonRoom", sa, false);
            return i != null ? spells.get(i) : super.chooseSingleSpellForEffect(spells, sa, title, params);
        }
        // je Zauber/Fähigkeit eine Kopie ihrer Karte mit nur ihrem Text (Karten-Optionen-Bühne)
        List<Decisions.Opt> opts = new ArrayList<>();
        List<Card> hosts = new ArrayList<>();
        spells.forEach(s -> addSpellOption(s, opts, hosts));
        Integer i = Decisions.cardOption(getGame(), seat, title, opts, hosts, null, "SpellForEffect", sa, false);
        return i != null ? spells.get(i) : super.chooseSingleSpellForEffect(spells, sa, title, params);
    }

    /** Mehrere Zauber bzw. Fähigkeiten für einen Effekt wählen ({@code ChooseOptions}). */
    @Override
    public List<SpellAbility> chooseSpellAbilitiesForEffect(List<SpellAbility> spells, SpellAbility sa, String title,
            int num, Map<String, Object> params) {
        int n = Math.min(num, spells.size());
        if (n <= 0) {
            return super.chooseSpellAbilitiesForEffect(spells, sa, title, num, params);
        }
        List<Decisions.Opt> opts = new ArrayList<>();
        List<Card> hosts = new ArrayList<>();
        spells.forEach(s -> addSpellOption(s, opts, hosts));
        List<Integer> chosen = Decisions.cardOptions(getGame(), seat, title, opts, hosts, n, n, "SpellsForEffect", sa);
        if (chosen == null) {
            return super.chooseSpellAbilitiesForEffect(spells, sa, title, num, params);
        }
        List<SpellAbility> result = new ArrayList<>();
        chosen.forEach(i -> result.add(spells.get(i)));
        return result;
    }

    /**
     * Option für die Karten-Optionen-Bühne: Text der Fähigkeit (ohne Kartennamen davor) und die
     * Karte, zu der sie gehört; ohne Karte der Text mit Kartennamen.
     */
    private static void addSpellOption(SpellAbility s, List<Decisions.Opt> opts, List<Card> hosts) {
        Card host = s.getHostCard();
        if (host == null) {
            opts.add(new Decisions.Opt(spellLabel(s), null));
            return;
        }
        String text = s.getStackDescription();
        if (text == null || text.isBlank()) {
            text = describe(s);
        }
        opts.add(new Decisions.Opt(Decisions.withName(text, host), null, host.getId()));
        if (!hosts.contains(host)) {
            hosts.add(host);
        }
    }

    private static String spellLabel(SpellAbility s) {
        Card host = s.getHostCard();
        String text = s.getStackDescription();
        if (text == null || text.isBlank()) {
            text = s.toUnsuppressedString();
        }
        return host == null ? text : host.getName() + ": " + text;
    }

    /**
     * Mögliche Ziele von {@code sa}: Spieler und Karten wie Forges {@code getAllCandidates}; zielt
     * die Fähigkeit auf den Stack (Gegenzauber, Kopieren, Umleiten), stattdessen die Zauber und
     * Fähigkeiten auf dem Stack selbst (wie Forges menschliche Zielwahl). {@code filter} = optionale
     * Zusatzbedingung (z. B. beim Umleiten).
     */
    private List<GameObject> targetCandidates(SpellAbility sa, java.util.function.Predicate<GameObject> filter) {
        List<GameObject> out = new ArrayList<>();
        boolean stack = sa.getTargetRestrictions().getZone().contains(ZoneType.Stack);
        for (GameEntity e : sa.getTargetRestrictions().getAllCandidates(sa)) {
            if (stack && e instanceof Card c && c.isInZone(ZoneType.Stack)) {
                continue; // dafür unten der Zauber selbst
            }
            if (filter == null || filter.test(e)) {
                out.add(e);
            }
        }
        if (stack) {
            for (SpellAbilityStackInstance si : getGame().getStack()) {
                SpellAbility s = si.getSpellAbility();
                if (s != sa && s.getRootAbility() != sa.getRootAbility() && sa.canTarget(s)
                        && (filter == null || filter.test(s))) {
                    out.add(s);
                }
            }
        }
        return out;
    }

    /**
     * Spielen durch einen Effekt (Kaskade, „you may cast“, Wunder …): über Forges menschlichen
     * Ablauf, Ziele, Modi und Kosten wählt also der Spieler (ob er spielen will, fragt der Effekt
     * vorher über {@code confirmAction}).
     */
    @Override
    public boolean playSaFromPlayEffect(SpellAbility tgtSA) {
        return playHuman(tgtSA);
    }

    /**
     * Kosten für Angriff oder Block (z. B. Propaganda, CR 508.1h): zahlen? → {@code Confirm}, dann
     * bezahlen wie beim Wirken.
     */
    @Override
    public boolean payCombatCost(Card c, Cost cost, SpellAbility sa, String prompt) {
        if (!ComputerUtilCost.canPayCost(cost, sa, getPlayer(), true)) {
            return false;
        }
        String text = (prompt == null || prompt.isBlank() ? "Kosten zahlen" : prompt) + ": " + cost.toSimpleString()
                + " für " + c.getName() + "?";
        Boolean pay = Decisions.confirm(getGame(), seat, text, sa, c);
        if (pay == null) {
            return super.payCombatCost(c, cost, sa, prompt);
        }
        return pay && human(() -> PlaySpellAbility.payCostDuringAbilityResolve(this, getPlayer(), cost, sa, null));
    }

    /** Einen Teil der Kosten wirklich zahlen? ({@code Confirm}, z. B. Leben oder Marken als Kosten). */
    @Override
    public boolean confirmPayment(forge.game.cost.CostPart costPart, String prompt, SpellAbility sa) {
        Boolean yes = Decisions.confirm(getGame(), seat, prompt == null || prompt.isBlank()
                ? "Kosten zahlen: " + costPart.toString() + "?" : prompt, sa, sa == null ? null : sa.getHostCard());
        return yes != null ? yes : super.confirmPayment(costPart, prompt, sa);
    }

    /**
     * Zwei Stapel (Fact or Fiction u. a.): welchen Stapel nimmt der Spieler? ({@code ChooseOption},
     * verdeckte Stapel nur mit Anzahl). true = Stapel 1.
     */
    @Override
    public boolean chooseCardsPile(SpellAbility sa, CardCollectionView pile1, CardCollectionView pile2, String faceUp) {
        // Forge übergibt den Parameter „FaceDown“: "False" = beide offen, "One" = Stapel 1
        // verdeckt, "True" = beide verdeckt
        boolean open1 = !"One".equals(faceUp) && !"True".equals(faceUp);
        boolean open2 = !"True".equals(faceUp);
        // Erweiterung „Piles“: je Stapel seine offenen Karten bzw. die Anzahl verdeckter
        List<Decisions.Pile> piles = List.of(new Decisions.Pile("Stapel 1: " + pileText(pile1, open1), pile1, open1),
                new Decisions.Pile("Stapel 2: " + pileText(pile2, open2), pile2, open2));
        Integer i = Decisions.piles(getGame(), seat, "Stapel wählen", sa, piles);
        return i != null ? i == 0 : super.chooseCardsPile(sa, pile1, pile2, faceUp);
    }

    private static String pileText(CardCollectionView pile, boolean open) {
        if (pile.isEmpty()) {
            return "leer";
        }
        if (!open) {
            return pile.size() + " verdeckte Karte(n)";
        }
        List<String> names = new ArrayList<>();
        pile.forEach(c -> names.add(c.getName()));
        return String.join(", ", names);
    }

    /**
     * Reihenfolge, in der mehrere Kosten bezahlt werden: wie Forges menschlicher Controller (der
     * nur im Modus „volle Kontrolle“ fragt) und wie Arena ohne Rückfrage in der Reihenfolge der
     * Karte. Vorher kam hier ein Reihenfolge-Fenster mit Forges Rohtexten („T“, „2“,
     * „Sacrifice CARDNAME“).
     */
    @Override
    public List<forge.game.cost.CostPart> orderCosts(List<forge.game.cost.CostPart> costs) {
        return costs;
    }

    /**
     * Karten für eine Kosten-Teilzahlung während des Auflösens (z. B. „… es sei denn, du wirfst
     * eine Karte ab“): {@code ChooseCards} mit dem passenden Grund.
     */
    @Override
    public CardCollectionView chooseCardsForCost(CardCollectionView optionList, SpellAbility sa,
            forge.game.cost.CostPartWithList cpl, int amount, boolean isOptional, String prompt) {
        String reason = switch (cpl.getClass().getSimpleName()) {
            case "CostDiscard" -> "DiscardCost";
            case "CostSacrifice" -> "SacrificeCost";
            case "CostExile" -> "ExileCost";
            case "CostTapType" -> "TapCost";
            case "CostReturn" -> "ReturnCost";
            default -> "Choose";
        };
        CardCollection r = pickCards(reason, optionList, isOptional ? 0 : amount, amount,
                prompt == null || prompt.isBlank() ? "Kosten: " + cpl.toString() : prompt, sa, isOptional);
        return r != null ? r : super.chooseCardsForCost(optionList, sa, cpl, amount, isOptional, prompt);
    }

    /**
     * „Wirf n Karten ab, es sei denn, du wirfst eine Karte vom Typ … ab“: entweder genau eine Karte
     * des Typs oder n Karten ({@code ChooseCards}, Grund {@code Discard}).
     */
    @Override
    public CardCollectionView chooseCardsToDiscardUnlessType(int num, CardCollectionView hand, String[] uTypes,
            SpellAbility sa) {
        int n = Math.min(num, hand.size());
        CardCollection r = pickCards("Discard", hand, 1, n, n + " Karte(n) abwerfen – oder eine Karte vom Typ "
                + String.join("/", uTypes), sa, false);
        if (r != null) {
            boolean oneOfType = r.size() == 1
                    && !CardLists.getValidCards(r, uTypes, getPlayer(), sa == null ? null : sa.getHostCard(), sa).isEmpty();
            if (r.size() == n || oneOfType) {
                return r;
            }
        }
        return super.chooseCardsToDiscardUnlessType(num, hand, uTypes, sa);
    }

    /** Fähigkeiten aus der Starthand (Leylines, Chancellor …): welche nutzen? ({@code ChooseOptions}). */
    @Override
    public List<SpellAbility> chooseSaToActivateFromOpeningHand(List<SpellAbility> usableFromOpeningHand) {
        if (usableFromOpeningHand.isEmpty()) {
            return usableFromOpeningHand;
        }
        // je Fähigkeit ihre Karte (Karten-Optionen-Bühne mit Mehrfachwahl, 0 bis alle)
        List<Decisions.Opt> opts = new ArrayList<>();
        List<Card> hosts = new ArrayList<>();
        usableFromOpeningHand.forEach(sa -> addSpellOption(sa, opts, hosts));
        List<Integer> chosen = Decisions.cardOptions(getGame(), seat, "Fähigkeiten aus der Starthand nutzen", opts, hosts,
                0, opts.size(), "OpeningHand", null);
        if (chosen == null) {
            return super.chooseSaToActivateFromOpeningHand(usableFromOpeningHand);
        }
        List<SpellAbility> result = new ArrayList<>();
        chosen.forEach(i -> result.add(usableFromOpeningHand.get(i)));
        return result;
    }

    /** Welche Fähigkeit einer Karte gespielt wird, wenn ein Effekt das offenlässt ({@code ChooseOption}). */
    @Override
    public SpellAbility getAbilityToPlay(Card hostCard, List<SpellAbility> abilities, forge.util.ITriggerEvent triggerEvent) {
        if (abilities.size() <= 1) {
            return abilities.isEmpty() ? null : abilities.get(0);
        }
        // je Spielweise eine Kopie der Karte mit nur ihrem Text (Karten-Optionen-Bühne)
        List<Decisions.Opt> opts = new ArrayList<>();
        for (SpellAbility sa : abilities) {
            opts.add(new Decisions.Opt(Decisions.withName(describe(sa), hostCard), null, hostCard.getId()));
        }
        Integer i = Decisions.cardOption(getGame(), seat, "Fähigkeit von " + hostCard.getName() + " wählen", opts,
                List.of(hostCard), null, "AbilityToPlay", null, true);
        return i != null ? abilities.get(i) : null;
    }

    /**
     * Welches schwebende Mana aus dem Pool bezahlt wird, wenn es mehrere Möglichkeiten gibt
     * ({@code ChooseOption}; das Basismodell ({@code PayAction}) kennt keine Pool-Zahlung).
     */
    @Override
    public forge.game.mana.Mana chooseManaFromPool(List<forge.game.mana.Mana> manaChoices) {
        // Welches schwebende Mana bezahlt, entscheidet wie in Arena immer Forge selbst: die Frage kam
        // bei jedem Symbol und jedem Zauber und hielt den Spieler nur auf
        if (manaChoices.size() <= 1 || autoPaying || !ASK_POOL_CHOICE) {
            return super.chooseManaFromPool(manaChoices);
        }
        List<Decisions.Opt> opts = new ArrayList<>();
        for (forge.game.mana.Mana m : manaChoices) {
            Card src = m.getSourceCard();
            opts.add(new Decisions.Opt(m + (src == null ? "" : " (von " + src.getName() + ")"),
                    String.valueOf(forge.card.MagicColor.toShortString(m.getColor()))));
        }
        Integer i = Decisions.option(getGame(), seat, "Mana aus dem Pool wählen", null, null, opts, false, "ManaPool");
        return i != null ? manaChoices.get(i) : super.chooseManaFromPool(manaChoices);
    }

    /** Spleißen (Splice onto Arcane, CR 702.47): welche Karten aus der Hand angespleißt werden. */
    @Override
    public List<Card> chooseCardsForSplice(SpellAbility sa, List<Card> cards) {
        if (cards.isEmpty()) {
            return super.chooseCardsForSplice(sa, cards);
        }
        CardCollection r = pickCards("Choose", new CardCollection(cards), 0, cards.size(),
                "Spleißen: Karten wählen, die an " + (sa.getHostCard() == null ? "den Zauber" : sa.getHostCard().getName())
                        + " angespleißt werden", sa, false);
        return r != null ? new ArrayList<>(r) : super.chooseCardsForSplice(sa, cards);
    }

    /**
     * Je eine Karte aus mehreren Gruppen wählen (z. B. „ein Artefakt, eine Kreatur, …“): nacheinander
     * {@code ChooseCards} je Gruppe, ohne schon gewählte Karten.
     */
    @Override
    public CardCollection chooseCardsForEffectMultiple(Map<String, CardCollection> validMap, SpellAbility sa,
            String title, boolean isOptional) {
        CardCollection chosen = new CardCollection();
        for (Map.Entry<String, CardCollection> e : validMap.entrySet()) {
            CardCollection options = new CardCollection(e.getValue());
            options.removeAll(chosen);
            if (options.isEmpty()) {
                continue;
            }
            CardCollection r = pickCards("Choose", options, isOptional ? 0 : 1, 1,
                    (title == null || title.isBlank() ? "Karte wählen" : title) + ": " + e.getKey(), sa, false);
            if (r == null) {
                return super.chooseCardsForEffectMultiple(validMap, sa, title, isOptional);
            }
            chosen.addAll(r);
        }
        return chosen;
    }

    /** Welches Ziel eines Zaubers geändert wird ({@code ChooseOption} „Zauber → Ziel“). */
    @Override
    public org.apache.commons.lang3.tuple.Pair<SpellAbilityStackInstance, GameObject> chooseTarget(SpellAbility sa,
            List<org.apache.commons.lang3.tuple.Pair<SpellAbilityStackInstance, GameObject>> allTargets) {
        if (allTargets.size() <= 1) {
            return super.chooseTarget(sa, allTargets);
        }
        List<Decisions.Opt> opts = new ArrayList<>();
        for (var p : allTargets) {
            Card host = p.getLeft().getSourceCard();
            opts.add(new Decisions.Opt((host == null ? "?" : host.getName()) + " → " + targetLabel(p.getRight()), null));
        }
        Integer i = Decisions.option(getGame(), seat, "Ziel wählen, das geändert wird", sa, null, opts, false, "Retarget");
        return i != null ? allTargets.get(i) : super.chooseTarget(sa, allTargets);
    }

    /**
     * Aufgedeckte Karten zeigen (CR 701.16): Kartenfächer ohne Auswahl ({@code ChooseCards} mit
     * {@code max: 0}, „Fertig“ = gesehen). Eigene Handkarten, die der Spieler ohnehin sieht, und
     * leere Listen werden übersprungen.
     */
    @Override
    public void reveal(CardCollectionView cards, ZoneType zone, Player owner, String messagePrefix, boolean addSuffix) {
        if (cards == null || cards.isEmpty() || (owner == getPlayer() && zone == ZoneType.Hand)) {
            return;
        }
        String who = owner == null ? "" : (owner == getPlayer() ? "Deine" : owner.getName() + ":");
        String prompt = (messagePrefix == null || messagePrefix.isBlank() ? "Aufgedeckt" : messagePrefix.trim())
                + " – " + who + " " + zoneName(zone);
        Decisions.cards(getGame(), seat, "Reveal", new ArrayList<>(cards), 0, 0, prompt, null, false);
    }

    private static String zoneName(ZoneType zone) {
        if (zone == null) {
            return "";
        }
        return switch (zone) {
            case Library -> "Bibliothek";
            case Hand -> "Hand";
            case Graveyard -> "Friedhof";
            case Battlefield -> "Spielfeld";
            case Exile -> "Exil";
            case Command -> "Kommandozone";
            case Stack -> "Stack";
            default -> zone.name();
        };
    }

    @Override
    public void reveal(List<forge.game.card.CardView> cards, ZoneType zone, forge.game.player.PlayerView owner,
            String messagePrefix, boolean addSuffix) {
        CardCollection real = new CardCollection();
        for (forge.game.card.CardView v : cards) {
            Card c = getGame().findById(v.getId());
            if (c != null) {
                real.add(c);
            }
        }
        Player p = owner == null ? null : getGame().getPlayer(owner);
        reveal(real, zone, p, messagePrefix, addSuffix);
    }

    /** Mitteilung (z. B. „Forge-KI wählt Rot“): als Eintrag im Spielprotokoll ({@code log}, Art INFORMATION). */
    @Override
    public void notifyOfValue(SpellAbility saSource, GameObject realtedTarget, String value) {
        com.google.gson.JsonObject m = BridgeCore.message("log");
        m.addProperty("kind", "INFORMATION");
        Card host = saSource == null ? null : saSource.getHostCard();
        m.addProperty("text", (host == null ? "" : host.getName() + ": ") + value);
        if (host != null) {
            m.addProperty("card", host.getId());
        }
        m.addProperty("turn", getGame().getPhaseHandler().getTurn());
        BridgeCore.send(m);
    }

    /**
     * Schutzschild auf mehrere betroffene Ziele verteilen (CR 615.7): {@code DivideAmount} mit der
     * Schildmenge (höchstens der gesamte Schaden), je Ziel ab 0.
     */
    @Override
    public Map<GameEntity, Integer> divideShield(Card effectSource, Map<GameEntity, Integer> affected, int shieldAmount) {
        List<GameEntity> targets = new ArrayList<>(affected.keySet());
        int damage = affected.values().stream().mapToInt(Integer::intValue).sum();
        int amount = Math.min(shieldAmount, damage);
        if (targets.size() <= 1 || amount <= 0) {
            return super.divideShield(effectSource, affected, shieldAmount);
        }
        List<String> labels = new ArrayList<>();
        targets.forEach(t -> labels.add(targetLabel(t) + " (" + affected.get(t) + " Schaden)"));
        List<Integer> parts = Decisions.divide(getGame(), seat, null, amount, targets, labels, 0);
        if (parts == null) {
            return super.divideShield(effectSource, affected, shieldAmount);
        }
        Map<GameEntity, Integer> result = new java.util.HashMap<>();
        for (int i = 0; i < targets.size(); i++) {
            result.put(targets.get(i), parts.get(i));
        }
        return result;
    }

    /** Welcher Würfelwurf getauscht wird ({@code ChooseOption}). */
    @Override
    public forge.game.ability.effects.RollDiceEffect.DieRollResult chooseRollToSwap(
            List<forge.game.ability.effects.RollDiceEffect.DieRollResult> rolls) {
        if (rolls.size() <= 1) {
            return super.chooseRollToSwap(rolls);
        }
        List<Decisions.Opt> opts = new ArrayList<>();
        rolls.forEach(r -> opts.add(new Decisions.Opt("Wurf " + r.getModifiedValue()
                + (r.getModifiedValue() != r.getNaturalValue() ? " (gewürfelt " + r.getNaturalValue() + ")" : ""), null)));
        Integer i = Decisions.option(getGame(), seat, "Würfelwurf zum Tauschen wählen", null, null, opts, false, "Dice");
        return i != null ? rolls.get(i) : super.chooseRollToSwap(rolls);
    }

    /** Mit welchem Wert getauscht wird (z. B. Stärke oder Widerstand, {@code ChooseOption}). */
    @Override
    public String chooseRollSwapValue(List<String> swapChoices, Integer currentResult, int power, int toughness) {
        if (swapChoices.size() <= 1) {
            return super.chooseRollSwapValue(swapChoices, currentResult, power, toughness);
        }
        List<Decisions.Opt> opts = new ArrayList<>();
        swapChoices.forEach(c -> opts.add(new Decisions.Opt(c, null)));
        Integer i = Decisions.option(getGame(), seat, "Wurf " + currentResult + " tauschen mit (Stärke " + power
                + ", Widerstand " + toughness + ")", null, null, opts, false, "Dice");
        return i != null ? swapChoices.get(i) : super.chooseRollSwapValue(swapChoices, currentResult, power, toughness);
    }

    /** Fähigkeit ohne Stack ausführen (z. B. Spezialaktionen): über Forges menschlichen Ablauf. */
    @Override
    public void playSpellAbilityNoStack(SpellAbility effectSA, boolean canSetupTargets) {
        human(() -> PlaySpellAbility.playSpellAbilityNoStack(this, getPlayer(), effectSA, !canSetupTargets));
    }

    /**
     * „n Mana in beliebiger Farbkombination“: je Mana eine Farbe wählen ({@code ChooseColor} über
     * {@link #chooseColor}); bei {@code different} jede Farbe höchstens einmal.
     */
    @Override
    public Map<Byte, Integer> specifyManaCombo(SpellAbility sa, ColorSet colorSet, int manaAmount, boolean different) {
        Map<Byte, Integer> result = new java.util.HashMap<>();
        ColorSet left = colorSet;
        for (int i = 0; i < manaAmount; i++) {
            byte chosen = chooseColor("Farbe für Mana " + (i + 1) + " von " + manaAmount, sa, left);
            result.merge(chosen, 1, Integer::sum);
            if (different) {
                left = ColorSet.fromMask(left.getColor() & ~chosen);
            }
        }
        return result;
    }

    /** Kosten zahlen, um einen Würfelwurf zu ändern: {@code Confirm}, dann Bezahlen wie beim Wirken. */
    @Override
    public boolean payCostDuringRoll(Cost cost, SpellAbility sa) {
        if (!ComputerUtilCost.canPayCost(cost, sa, getPlayer(), true)) {
            return false;
        }
        Boolean pay = Decisions.confirm(getGame(), seat, "Kosten zahlen, um den Wurf zu ändern: "
                + cost.toSimpleString() + "?", sa, sa == null ? null : sa.getHostCard());
        if (pay == null) {
            return super.payCostDuringRoll(cost, sa);
        }
        return pay && human(() -> PlaySpellAbility.payCostDuringAbilityResolve(this, getPlayer(), cost, sa, null));
    }

    /** Anzeige eines möglichen Ziels: Karte „Name (Besitzer)“ bzw. Spieler „Name“ / „Name (du)“. */
    private String targetLabel(GameObject e) {
        if (e instanceof SpellAbility s) {
            Card host = s.getHostCard();
            return (host == null ? "?" : host.getName()) + (s.isSpell() ? " (Zauber)" : " (Fähigkeit)");
        }
        if (e instanceof Card c) {
            String owner = c.getController() == getPlayer() ? "deine" : c.getController().getName();
            return c.getName() + " (" + owner + ")";
        }
        if (e instanceof Player p) {
            return p == getPlayer() ? p.getName() + " (du)" : p.getName();
        }
        return String.valueOf(e);
    }

    /** Gibt es für jede gezielte (Teil-)Fähigkeit mit Mindestzahl an Zielen mögliche Ziele? */
    private static boolean hasPossibleTargets(SpellAbility sa) {
        for (SpellAbility cur = sa; cur != null; cur = cur.getSubAbility()) {
            if (cur.usesTargeting() && cur.getMinTargets() > 0
                    && !cur.getTargetRestrictions().hasCandidates(cur)) {
                return false;
            }
        }
        return true;
    }

    /**
     * {@code Decision::AssignCombatDamage} (CR 510.1c–d): Kampfschaden auf mehrere Blocker bzw. mit
     * Trampelschaden verteilen. Ein Blocker ohne Trampeln: kein Fragen. Ungültig (Summe falsch,
     * Trampelschaden ohne tödlichen Schaden an allen Blockern) → KI verteilt.
     */
    @Override
    public Map<Card, Integer> assignCombatDamage(Card attacker, CardCollectionView blockers,
            CardCollectionView remaining, int damageDealt, GameEntity defender, boolean overrideOrder) {
        List<Card> bl = new ArrayList<>(blockers);
        boolean trample = attacker.hasKeyword(Keyword.TRAMPLE) && defender != null;
        if (bl.size() == 1 && !trample) {
            Map<Card, Integer> one = new HashMap<>();
            one.put(bl.get(0), damageDealt);
            return one;
        }
        boolean deathtouch = attacker.hasKeyword(Keyword.DEATHTOUCH);
        List<Integer> lethal = new ArrayList<>();
        bl.forEach(b -> lethal.add(deathtouch ? Math.min(1, b.getLethalDamage()) : Math.max(0, b.getLethalDamage())));
        List<Integer> values = Decisions.assignDamage(getGame(), seat, attacker, damageDealt, bl, lethal,
                trample ? defender : null);
        if (values != null) {
            int sum = values.stream().mapToInt(Integer::intValue).sum();
            int toDefender = values.size() > bl.size() ? values.get(bl.size()) : 0;
            boolean allLethal = true;
            for (int i = 0; i < bl.size(); i++) {
                allLethal &= values.get(i) >= lethal.get(i);
            }
            boolean ok = sum == damageDealt && values.stream().allMatch(v -> v >= 0) && (toDefender == 0 || allLethal);
            if (ok) {
                Map<Card, Integer> map = new HashMap<>();
                for (int i = 0; i < bl.size(); i++) {
                    if (values.get(i) > 0) {
                        map.put(bl.get(i), values.get(i));
                    }
                }
                if (toDefender > 0) {
                    map.put(null, toDefender);
                }
                return map;
            }
        }
        return super.assignCombatDamage(attacker, blockers, remaining, damageDealt, defender, overrideOrder);
    }

    /**
     * {@code Decision::PayUnlessCost} (CR 118.12): „…, es sei denn, du zahlst …“. Nur wenn die
     * Kosten bezahlbar sind; Ja → Bezahlen wie beim Wirken (Mana über {@code PayMana}).
     */
    @Override
    public boolean payCostToPreventEffect(Cost cost, SpellAbility sa, boolean alreadyPaid,
            FCollectionView<Player> allPayers) {
        if (!ComputerUtilCost.canPayCost(cost, sa, getPlayer(), true)) {
            return false;
        }
        Boolean pay = Decisions.payUnless(getGame(), seat, sa, cost.toSimpleString());
        if (pay == null) {
            return super.payCostToPreventEffect(cost, sa, alreadyPaid, allPayers);
        }
        return pay && human(() -> PlaySpellAbility.payCostDuringAbilityResolve(this, getPlayer(), cost, sa, null));
    }

    /** Höchstens so viele ungültige Kampf-Erklärungen, danach entscheidet die KI. */
    private static final int MAX_COMBAT_TRIES = 4;

    /**
     * {@code Decision::DeclareAttackers} (CR 508.1): Kandidaten = Kreaturen, die angreifen können,
     * Ziele = angreifbare Spieler, Planeswalker, Schlachten. Ungültige Erklärungen (z. B. eine
     * Kreatur muss angreifen) werden mit Hinweis erneut gefragt.
     */
    @Override
    public void declareAttackers(Player attacker, Combat combat) {
        List<GameEntity> defenders = new ArrayList<>(combat.getDefenders());
        List<Card> candidates = new ArrayList<>();
        for (Card c : attacker.getCreaturesInPlay()) {
            if (CombatUtil.canAttack(c) && defenders.stream().anyMatch(d -> CombatUtil.canAttack(c, d))) {
                candidates.add(c);
            }
        }
        if (candidates.isEmpty()) {
            return;
        }
        List<String> labels = new ArrayList<>();
        candidates.forEach(c -> labels.add(c.getName()));
        String error = null;
        for (int attempt = 0; attempt < MAX_COMBAT_TRIES; attempt++) {
            List<int[]> chosen = Decisions.attackers(getGame(), seat, candidates, defenders, labels, error);
            if (chosen == null) {
                break;
            }
            List<Card> added = new ArrayList<>();
            boolean ok = true;
            for (int[] pair : chosen) {
                Card c = candidates.get(pair[0]);
                GameEntity d = defenders.get(pair[1]);
                if (!CombatUtil.canAttack(c, d) || added.contains(c)) {
                    ok = false;
                    break;
                }
                combat.addAttacker(c, d);
                added.add(c);
            }
            if (ok && CombatUtil.validateAttackers(combat)) {
                return;
            }
            added.forEach(combat::removeFromCombat);
            error = "Ungültiger Angriff – bitte erneut wählen (z. B. muss eine Kreatur angreifen)";
        }
        super.declareAttackers(attacker, combat);
    }

    /**
     * {@code Decision::DeclareBlockers} (CR 509.1): Kandidaten = eigene Kreaturen, die blocken
     * können, Angreifer = alle angreifenden Kreaturen. Ungültige Blocks (Forges
     * {@code CombatUtil.validateBlocks}) werden mit Hinweis erneut gefragt.
     */
    @Override
    public void declareBlockers(Player defender, Combat combat) {
        List<Card> attackers = new ArrayList<>(combat.getAttackers());
        List<Card> candidates = new ArrayList<>();
        for (Card c : defender.getCreaturesInPlay()) {
            if (CombatUtil.canBlock(c, combat)) {
                candidates.add(c);
            }
        }
        if (candidates.isEmpty() || attackers.isEmpty()) {
            return;
        }
        List<String> labels = new ArrayList<>();
        candidates.forEach(c -> labels.add(c.getName()));
        String error = null;
        for (int attempt = 0; attempt < MAX_COMBAT_TRIES; attempt++) {
            List<int[]> chosen = Decisions.blocks(getGame(), seat, candidates, attackers, labels, error);
            if (chosen == null) {
                break;
            }
            List<int[]> added = new ArrayList<>();
            boolean ok = true;
            for (int[] pair : chosen) {
                Card b = candidates.get(pair[0]);
                Card a = attackers.get(pair[1]);
                if (!CombatUtil.canBlock(a, b, combat)) {
                    ok = false;
                    break;
                }
                combat.addBlocker(a, b);
                added.add(pair);
            }
            String problem = ok ? CombatUtil.validateBlocks(combat, defender) : "Diese Kreatur kann diesen Angreifer nicht blocken";
            if (problem == null) {
                return;
            }
            for (int[] pair : added) {
                combat.removeBlockAssignment(attackers.get(pair[1]), candidates.get(pair[0]));
            }
            error = "Ungültige Blocks: " + problem;
        }
        super.declareBlockers(defender, combat);
    }

    // ------------------------------------------------------------------ Karten wählen

    private CardCollection pickCards(String reason, CardCollectionView options, int min, int max, String prompt,
            SpellAbility sa, boolean cancelable) {
        List<Card> list = new ArrayList<>(options);
        List<Integer> chosen = Decisions.cards(getGame(), seat, reason, list, min, max, prompt, sa, cancelable);
        if (chosen == null) {
            return null;
        }
        CardCollection result = new CardCollection();
        chosen.forEach(i -> result.add(list.get(i)));
        return result;
    }

    @Override
    public CardCollectionView chooseCardsForEffect(CardCollectionView sourceList, SpellAbility sa, String title,
            int min, int max, boolean isOptional, Map<String, Object> params) {
        CardCollection r = pickCards("Choose", sourceList, isOptional ? 0 : min, max, title, sa, false);
        return r != null ? r : super.chooseCardsForEffect(sourceList, sa, title, min, max, isOptional, params);
    }

    @Override
    public List<Card> chooseCardsForZoneChange(ZoneType destination, List<ZoneType> origin, SpellAbility sa,
            CardCollection fetchList, int min, int max, DelayedReveal delayedReveal, String selectPrompt,
            Player decider) {
        String reason = origin != null && origin.contains(ZoneType.Library) ? "Search" : "Choose";
        CardCollection r = pickCards(reason, fetchList, min, max, selectPrompt, sa, false);
        return r != null ? r
                : super.chooseCardsForZoneChange(destination, origin, sa, fetchList, min, max, delayedReveal,
                        selectPrompt, decider);
    }

    @Override
    public Card chooseSingleCardForZoneChange(ZoneType destination, List<ZoneType> origin, SpellAbility sa,
            CardCollection fetchList, DelayedReveal delayedReveal, String selectPrompt, boolean isOptional,
            Player decider) {
        String reason = origin != null && origin.contains(ZoneType.Library) ? "Search" : "Choose";
        CardCollection r = pickCards(reason, fetchList, isOptional ? 0 : 1, 1, selectPrompt, sa, false);
        if (r == null) {
            return super.chooseSingleCardForZoneChange(destination, origin, sa, fetchList, delayedReveal,
                    selectPrompt, isOptional, decider);
        }
        return r.isEmpty() ? null : r.get(0);
    }

    @Override
    public CardCollection chooseCardsToDiscardFrom(Player playerDiscard, SpellAbility sa,
            CardCollection validCards, int min, int max, CardCollectionView visibleToChooser) {
        CardCollection r = pickCards("Discard", validCards, min, max, "Karten abwerfen", sa, false);
        return r != null ? r
                : super.chooseCardsToDiscardFrom(playerDiscard, sa, validCards, min, max, visibleToChooser);
    }

    @Override
    public CardCollectionView chooseCardsToDiscardToMaximumHandSize(int numDiscard) {
        CardCollection r = pickCards("DiscardToHandSize", getPlayer().getCardsIn(ZoneType.Hand), numDiscard,
                numDiscard, "Auf Handkartenlimit abwerfen", null, false);
        return r != null ? r : super.chooseCardsToDiscardToMaximumHandSize(numDiscard);
    }

    @Override
    public CardCollectionView choosePermanentsToSacrifice(SpellAbility sa, int min, int max,
            CardCollectionView validTargets, String message) {
        CardCollection r = pickCards("Sacrifice", validTargets, min, max, message, sa, false);
        return r != null ? r : super.choosePermanentsToSacrifice(sa, min, max, validTargets, message);
    }

    @Override
    public CardCollectionView choosePermanentsToDestroy(SpellAbility sa, int min, int max,
            CardCollectionView validTargets, String message) {
        CardCollection r = pickCards("Sacrifice", validTargets, min, max, message, sa, false);
        return r != null ? r : super.choosePermanentsToDestroy(sa, min, max, validTargets, message);
    }

    @Override
    public CardCollectionView chooseCardsToRevealFromHand(int min, int max, CardCollectionView valid) {
        CardCollection r = pickCards("Choose", valid, min, max, "Karten aufdecken", null, false);
        return r != null ? r : super.chooseCardsToRevealFromHand(min, max, valid);
    }

    /** Karten oder Spieler für einen Effekt: nur Karten → {@code ChooseCards}, sonst Erweiterung. */
    @Override
    public <T extends GameEntity> T chooseSingleEntityForEffect(FCollectionView<T> optionList,
            DelayedReveal delayedReveal, SpellAbility sa, String title, boolean isOptional, Player relatedPlayer,
            Map<String, Object> params) {
        List<T> list = new ArrayList<>(optionList);
        if (list.isEmpty()) {
            return null;
        }
        if (list.stream().allMatch(e -> e instanceof Card)) {
            CardCollection cards = new CardCollection();
            list.forEach(e -> cards.add((Card) e));
            CardCollection r = pickCards("Choose", cards, isOptional ? 0 : 1, 1, title, sa, false);
            if (r != null) {
                @SuppressWarnings("unchecked")
                T t = r.isEmpty() ? null : (T) r.get(0);
                return t;
            }
        } else {
            // Spieler (ggf. mit Karten): wie Ziele per Klick auf Medaillon bzw. Karte
            List<Integer> r = entities(list, isOptional ? 0 : 1, 1, title, sa);
            if (r != null) {
                return r.isEmpty() ? null : list.get(r.get(0));
            }
        }
        return super.chooseSingleEntityForEffect(optionList, delayedReveal, sa, title, isOptional, relatedPlayer,
                params);
    }

    @Override
    public <T extends GameEntity> List<T> chooseEntitiesForEffect(FCollectionView<T> optionList, int min, int max,
            DelayedReveal delayedReveal, SpellAbility sa, String title, Player relatedPlayer,
            Map<String, Object> params) {
        List<T> list = new ArrayList<>(optionList);
        List<Integer> chosen;
        if (list.stream().allMatch(e -> e instanceof Card)) {
            List<Card> cards = new ArrayList<>();
            list.forEach(e -> cards.add((Card) e));
            chosen = Decisions.cards(getGame(), seat, "Choose", cards, min, max, title, sa, false);
        } else {
            chosen = entities(list, min, max, title, sa);
        }
        if (chosen == null) {
            return super.chooseEntitiesForEffect(optionList, min, max, delayedReveal, sa, title, relatedPlayer,
                    params);
        }
        List<T> result = new ArrayList<>();
        chosen.forEach(i -> result.add(list.get(i)));
        return result;
    }

    /**
     * Spieler und/oder Karten für einen Effekt wählen: {@code ChooseTargets} mit
     * {@code purpose = "Choose"} (kein Ziel; Knöpfe je Spieler, Klick auf Medaillon bzw. Karte). Das Basismodell kennt dafür keine eigene Entscheidung (Erweiterung, siehe
     * Forge-Protocol.md). Liefert die gewählten Indizes oder {@code null} (Engine entscheidet).
     */
    private <T extends GameEntity> List<Integer> entities(List<T> list, int min, int max, String title,
            SpellAbility sa) {
        List<GameEntity> candidates = new ArrayList<>(list);
        List<String> labels = new ArrayList<>();
        list.forEach(e -> labels.add(targetLabel(e)));
        return Decisions.targets(getGame(), seat, sa, title, candidates, labels, min, max, "Choose");
    }

    @Override
    public ImmutablePair<CardCollection, CardCollection> arrangeForScry(CardCollection topN) {
        return arrange(topN, false);
    }

    @Override
    public ImmutablePair<CardCollection, CardCollection> arrangeForSurveil(CardCollection topN) {
        return arrange(topN, true);
    }

    /** {@code Decision::Scry}: oben (von oben nach unten) und unten bzw. Friedhof. */
    private ImmutablePair<CardCollection, CardCollection> arrange(CardCollection topN, boolean surveil) {
        List<Card> list = new ArrayList<>(topN);
        List<List<Integer>> r = Decisions.scry(getGame(), seat, list, surveil);
        if (r == null) {
            return surveil ? super.arrangeForSurveil(topN) : super.arrangeForScry(topN);
        }
        CardCollection top = new CardCollection();
        CardCollection bottom = new CardCollection();
        r.get(0).forEach(i -> top.add(list.get(i)));
        r.get(1).forEach(i -> bottom.add(list.get(i)));
        // oben gelassene Karten kennt der Spieler (Hellsicht/Überwachen)
        KnownLibrary.learn(getPlayer(), top);
        return ImmutablePair.of(top, bottom);
    }

    /**
     * Schadens-Reihenfolge der Blocker eines Angreifers (nur mit Forges Regel-Option
     * „Kampfteilnehmer ordnen“, alte CR 509.2): Reihenfolge-Fenster mit Kartennamen.
     */
    @Override
    public CardCollection orderBlockers(Card attacker, CardCollection blockers) {
        CardCollection r = orderCards(blockers, "Reihenfolge der Blocker für " + attacker.getName()
                + " (erster bekommt zuerst Schaden)");
        return r != null ? r : super.orderBlockers(attacker, blockers);
    }

    /** Neuer Blocker wird in eine bestehende Reihenfolge eingefügt (z. B. durch einen Effekt). */
    @Override
    public CardCollection orderBlocker(Card attacker, Card blocker, CardCollection oldBlockers) {
        CardCollection all = new CardCollection(oldBlockers);
        all.add(blocker);
        CardCollection r = orderCards(all, "Reihenfolge der Blocker für " + attacker.getName()
                + " (erster bekommt zuerst Schaden)");
        return r != null ? r : super.orderBlocker(attacker, blocker, oldBlockers);
    }

    /** Schadens-Reihenfolge der Angreifer, die ein Blocker blockt (alte Regel, siehe oben). */
    @Override
    public CardCollection orderAttackers(Card blocker, CardCollection attackers) {
        CardCollection r = orderCards(attackers, "Reihenfolge der Angreifer für " + blocker.getName()
                + " (erster bekommt zuerst Schaden)");
        return r != null ? r : super.orderAttackers(blocker, attackers);
    }

    /** Karten ordnen ({@code OrderTriggers} mit Kartennamen); {@code null} = Engine entscheidet. */
    private CardCollection orderCards(CardCollection cards, String title) {
        if (cards.size() <= 1) {
            return new CardCollection(cards);
        }
        List<Card> list = new ArrayList<>(cards);
        List<String> names = new ArrayList<>();
        list.forEach(c -> names.add(c.getName()));
        List<Integer> order = Decisions.order(getGame(), seat, names, title, list);
        if (order == null) {
            return null;
        }
        CardCollection result = new CardCollection();
        order.forEach(i -> result.add(list.get(i)));
        return result;
    }

    /** Reihenfolge von Karten beim Zonenwechsel ({@code OrderTriggers} mit Kartennamen). */
    @Override
    public CardCollectionView orderMoveToZoneList(CardCollectionView cards, ZoneType destinationZone,
            SpellAbility source) {
        List<Card> list = new ArrayList<>(cards);
        List<String> names = new ArrayList<>();
        list.forEach(c -> names.add(c.getName()));
        // In eine Bibliothek: die zuerst gewählte Karte liegt am Ende oben (bzw. ganz unten, wenn
        // die Karten unter die Bibliothek kommen) – wie Forges eigener Controller. Forge legt die
        // Karten einzeln in Listen-Reihenfolge ab, die zuletzt bewegte liegt also obenauf: die
        // gewählte Reihenfolge wird deshalb umgedreht.
        boolean deck = destinationZone.isDeck();
        boolean top = orderedMoveToTopOfLibrary(destinationZone, source);
        String title = !deck ? "Reihenfolge wählen (" + destinationZone + ", erste zuerst)"
                : top ? "Reihenfolge wählen: die erste Karte kommt ganz nach oben"
                : "Reihenfolge wählen: die erste Karte kommt ganz nach unten";
        List<Integer> order = Decisions.order(getGame(), seat, names, title, list);
        if (order == null) {
            return super.orderMoveToZoneList(cards, destinationZone, source);
        }
        CardCollection result = new CardCollection();
        order.forEach(i -> result.add(list.get(i)));
        if (deck) {
            java.util.Collections.reverse(result);
            if (top && destinationZone == ZoneType.Library) {
                // der Spieler kennt jetzt die Reihenfolge oben auf seiner Bibliothek
                KnownLibrary.learn(getPlayer(), result);
            }
        }
        return result;
    }

    /**
     * Eigene ausgelöste Fähigkeiten auf den Stapel legen wie Forges menschlicher Controller:
     * Reihenfolge ({@link #orderSimultaneousSa}), dann je Trigger
     * {@code PlaySpellAbility.playSpellAbility} – Modi, Ziele ({@code ChooseTargets}) und Kosten
     * wählt dabei der Spieler. Kopien kommen auf den Stack; neue Ziele (falls erlaubt) fragt
     * {@link #chooseNewTargetsFor}.
     */
    @Override
    public void orderAndPlaySimultaneousSa(List<SpellAbility> activePlayerSAs) {
        for (SpellAbility sa : orderSimultaneousSa(activePlayerSAs)) {
            if (sa.isTrigger() && !sa.isCopied()) {
                playHuman(sa);
            } else if (sa.isCopied()) {
                // Kopie (CR 707.10): wie Forges menschlicher Controller auf den Stack legen; darf
                // der Spieler neue Ziele wählen, fragt setupNewTargets → chooseNewTargetsFor
                if (sa.isSpell()) {
                    if (!sa.getHostCard().isInZone(ZoneType.Stack)) {
                        sa.setHostCard(getGame().getAction().moveToStack(sa.getHostCard(), sa));
                    } else {
                        getGame().getStackZone().add(sa.getHostCard());
                    }
                }
                if (sa.isMayChooseNewTargets()) {
                    sa.setupNewTargets(getPlayer());
                }
                getGame().getStack().add(sa);
            } else {
                List<SpellAbility> single = new ArrayList<>();
                single.add(sa);
                super.orderAndPlaySimultaneousSa(single);
            }
        }
    }

    /** Trigger, die ohne Stapel wirken: wie Forges menschlicher Controller (Ziele wählt der Spieler). */
    @Override
    public boolean playTrigger(Card host, WrappedAbility wrapperAbility, boolean isMandatory) {
        return human(() -> PlaySpellAbility.playSpellAbilityNoStack(this, getPlayer(), wrapperAbility, false));
    }

    /** {@code Decision::OrderTriggers} (CR 603.3b): eigene gleichzeitige Trigger ordnen. */
    @Override
    public List<SpellAbility> orderSimultaneousSa(List<SpellAbility> activePlayerSAs) {
        List<String> texts = new ArrayList<>();
        List<Card> hosts = new ArrayList<>();
        activePlayerSAs.forEach(sa -> {
            texts.add(sa.getHostCard().getName() + ": " + sa.toUnsuppressedString());
            hosts.add(sa.getHostCard());
        });
        List<Integer> order = Decisions.order(getGame(), seat, texts,
                "Reihenfolge der Fähigkeiten auf dem Stack (erste kommt zuerst darauf)", hosts);
        if (order == null) {
            return super.orderSimultaneousSa(activePlayerSAs);
        }
        List<SpellAbility> result = new ArrayList<>();
        order.forEach(i -> result.add(activePlayerSAs.get(i)));
        return result;
    }

    /** {@code Decision::MulliganBottom} (London-Mulligan): Karten unter die Bibliothek. */
    @Override
    public CardCollectionView tuckCardsViaMulligan(CardCollectionView hand, int cardsToReturn) {
        List<Card> list = new ArrayList<>(hand);
        List<Integer> chosen = Decisions.mulliganBottom(getGame(), seat, list, cardsToReturn);
        if (chosen == null) {
            return super.tuckCardsViaMulligan(hand, cardsToReturn);
        }
        CardCollection result = new CardCollection();
        chosen.forEach(i -> result.add(list.get(i)));
        return result;
    }

    /** {@code Decision::Mulligan}: {@code Keep} = behalten, {@code Mulligan} = neu ziehen. */
    @Override
    public boolean mulliganKeepHand(Player firstPlayer, int cardsToReturn) {
        Boolean keep = Decisions.mulligan(getGame(), seat, cardsToReturn,
                firstPlayer == null ? -1 : StateJson.indexOf(getGame(), firstPlayer));
        return keep == null || keep;
    }

    // ------------------------------------------------------------------ Auswahl-Dialoge

    private Game g() {
        return getGame();
    }

    private static String describe(SpellAbility sa) {
        String d = sa.getDescription();
        return d == null || d.isBlank() ? sa.toUnsuppressedString() : d;
    }

    private static <T> T pick(List<T> items, Integer index) {
        return index == null ? null : items.get(index);
    }

    /** {@code Decision::ChooseModes} (CR 700.2). */
    @Override
    public List<AbilitySub> chooseModeForAbility(SpellAbility sa, List<AbilitySub> possible, int min, int num,
            boolean allowRepeat) {
        List<String> texts = new ArrayList<>();
        // Zusatzkosten je Modus (Spree CR 702.172, Tiered: Parameter ModeCost)
        List<String> costs = new ArrayList<>();
        boolean anyCost = false;
        for (AbilitySub sub : possible) {
            texts.add(describe(sub));
            String c = sub.hasParam("ModeCost") ? new Cost(sub.getParam("ModeCost"), false).toSimpleString() : "";
            costs.add(c);
            anyCost |= !c.isEmpty();
        }
        String keyword = null;
        String baseCost = null;
        if (anyCost) {
            Card host = sa.getHostCard();
            keyword = host == null ? "" : host.hasKeyword(Keyword.SPREE) ? "Spree"
                    : host.hasKeyword(Keyword.TIERED) ? "Tiered" : "";
            baseCost = sa.getPayCosts() == null ? "" : sa.getPayCosts().toSimpleString();
        }
        List<Integer> chosen = Decisions.modes(g(), seat, sa, texts, min, num, allowRepeat, min == 0,
                anyCost ? costs : null, baseCost, keyword);
        if (chosen == null) {
            return super.chooseModeForAbility(sa, possible, min, num, allowRepeat);
        }
        List<AbilitySub> result = new ArrayList<>();
        for (int i : chosen) {
            result.add(possible.get(i));
        }
        return result;
    }

    /**
     * Schlüsselwort-Zusatzkosten beim Wirken (CR 601.2b): Casualty (CR 702.153), Conspire
     * (CR 702.78), Offspring (CR 702.175), Tappen bei Harmonize. Forge fragt sonst über
     * {@link #chooseNumberForKeywordCost} mit Höchstwert 1 (Zahl 0–1); hier stattdessen als
     * einzelne optionale Kosten wie Kicker ({@code ChooseOptions}, {@code purpose = "OptionalCost"}).
     */
    @Override
    public boolean addKeywordCost(SpellAbility sa, Cost cost, KeywordInterface keyword, String prompt) {
        String name = keyword != null && keyword.getTitle() != null ? keyword.getTitle() : "";
        if (name.isEmpty() && prompt != null) {
            // „Pay for Casualty? …“ → „Casualty“
            name = prompt.replaceFirst("^Pay for ", "").replaceFirst("\\?.*$", "");
        }
        // Knopftext kurz wie „Kicker – {R}“: reine Manakosten dahinter („X – {2}“), steht die
        // Angabe schon im Titel („Offspring {2}“) oder sind es andere Kosten (Opfern, Tappen –
        // erklärt der Erinnerungstext auf der Karte), nur das Schlüsselwort („Casualty“)
        String costText = cost != null && cost.isOnlyManaCost() ? cost.toSimpleString() : "";
        String text = costText.isEmpty() || name.endsWith(costText) ? name : name + " – " + costText;
        if (costText.isEmpty()) {
            // andere Kosten: nur das Schlüsselwort ohne Zahl („Casualty“ statt „Casualty 1“)
            text = text.replaceFirst("\\s+(\\d+|X)$", "");
        }
        boolean cancelable = humanFlow > 0;
        List<Integer> chosen = Decisions.options(g(), seat, prompt, sa, null,
                List.of(new Decisions.Opt(text, null)), 0, 1, cancelable, "OptionalCost");
        if (chosen == null) {
            if (cancelable) {
                // Abbrechen: die Karte liegt schon am Stack – die nächste Zielwahl bzw. das
                // Bezahlen lehnt ab, Forge nimmt das Wirken zurück
                cancelCasting = true;
                return false;
            }
            return super.addKeywordCost(sa, cost, keyword, prompt);
        }
        return !chosen.isEmpty();
    }

    /** Optionale Zusatzkosten (Kicker, Verstärkung …). */
    @Override
    public List<OptionalCostValue> chooseOptionalCosts(SpellAbility sa, List<OptionalCostValue> optionalCost) {
        boolean cancelable = humanFlow > 0;
        List<Integer> chosen = Decisions.options(g(), seat, "Optionale Kosten zahlen?", sa, null,
                Decisions.opts(optionalCost), 0, optionalCost.size(), cancelable, "OptionalCost");
        if (chosen == null) {
            if (cancelable) {
                // Abbrechen vor dem Stack: nichts ist passiert, das Wirken endet hier
                throw new CastCancelled();
            }
            return super.chooseOptionalCosts(sa, optionalCost);
        }
        List<OptionalCostValue> result = new ArrayList<>();
        for (int i : chosen) {
            result.add(optionalCost.get(i));
        }
        return result;
    }

    @Override
    public String chooseSomeType(String kindOfType, SpellAbility sa, Collection<String> validTypes,
            boolean isOptional) {
        List<String> types = new ArrayList<>(validTypes);
        String r = pick(types, Decisions.nameOrType(g(), seat, "ChooseType", "Wähle: " + kindOfType, sa, null,
                types, kindOfType, ownTypes(), isOptional));
        return r != null ? r : isOptional ? null : super.chooseSomeType(kindOfType, sa, validTypes, isOptional);
    }

    @Override
    public String chooseSector(Card assignee, String ai, List<String> sectors) {
        String r = pick(sectors, Decisions.option(g(), seat, "Wähle einen Sektor", null, assignee,
                Decisions.opts(sectors), false));
        return r != null ? r : super.chooseSector(assignee, ai, sectors);
    }

    @Override
    public String chooseKeywordForPump(List<String> options, SpellAbility sa, String prompt, Card tgtCard) {
        String r = pick(options, Decisions.option(g(), seat, prompt, sa, tgtCard,
                Decisions.opts(options), false, "Keyword"));
        return r != null ? r : super.chooseKeywordForPump(options, sa, prompt, tgtCard);
    }

    @Override
    public ReplacementEffect chooseSingleReplacementEffect(List<ReplacementEffect> possible) {
        // CR 616.1: der betroffene Spieler wählt, welcher Ersatzeffekt zuerst gilt; je Effekt
        // seine Quellkarte, damit die UI eine Kopie mit dem Effekttext zeigt
        List<Decisions.Opt> opts = new ArrayList<>();
        List<Card> hosts = new ArrayList<>();
        for (ReplacementEffect re : possible) {
            Card host = re.getHostCard();
            String text = re.getDescription();
            if (text == null || text.isBlank()) {
                text = String.valueOf(re);
            }
            opts.add(new Decisions.Opt(Decisions.withName(text, host), null, host == null ? -1 : host.getId()));
            if (host != null && !hosts.contains(host)) {
                hosts.add(host);
            }
        }
        ReplacementEffect r = pick(possible, Decisions.cardOption(g(), seat, "Welcher Ersatzeffekt gilt zuerst?",
                opts, hosts, "ReplacementEffect"));
        return r != null ? r : super.chooseSingleReplacementEffect(possible);
    }

    @Override
    public StaticAbility chooseSingleStaticAbility(List<StaticAbility> possible) {
        // je statische Fähigkeit ihre Quellkarte mit dem Text der Fähigkeit (Karten-Optionen-Bühne)
        List<Decisions.Opt> opts = new ArrayList<>();
        List<Card> hosts = new ArrayList<>();
        for (StaticAbility st : possible) {
            Card host = st.getHostCard();
            opts.add(new Decisions.Opt(Decisions.withName(String.valueOf(st), host), null, host == null ? -1 : host.getId()));
            if (host != null && !hosts.contains(host)) {
                hosts.add(host);
            }
        }
        boolean allCards = opts.stream().allMatch(o -> o.card() >= 0);
        StaticAbility r = pick(possible, allCards
                ? Decisions.cardOption(g(), seat, "Welche statische Fähigkeit gilt zuerst?", opts, hosts, "StaticAbility")
                : Decisions.option(g(), seat, "Welche statische Fähigkeit gilt zuerst?", null, null, opts, false));
        return r != null ? r : super.chooseSingleStaticAbility(possible);
    }

    @Override
    public String chooseProtectionType(SpellAbility sa, List<String> choices) {
        // Schutz vor einer Farbe: mit Farbbuchstaben, dann zeigt die UI das Farbrad
        List<Decisions.Opt> opts = new ArrayList<>();
        for (String ch : choices) {
            opts.add(new Decisions.Opt(ch, protectionColor(ch)));
        }
        String r = pick(choices, Decisions.option(g(), seat, "Schutz wovor?", sa, null, opts, false, "Protection"));
        return r != null ? r : super.chooseProtectionType(sa, choices);
    }

    @Override
    public CounterType chooseCounterType(List<CounterType> options, SpellAbility sa, String prompt,
            Map<String, Object> params) {
        // je Markenart ihr Engine-Name als Schlüssel (für die Münze in der UI, wie StateJson.counters)
        List<Decisions.Opt> opts = new ArrayList<>();
        for (CounterType c : options) {
            String key = c instanceof Enum<?> e ? e.name() : c.getName();
            opts.add(new Decisions.Opt(c.getName(), null, -1, key));
        }
        CounterType r = pick(options, Decisions.option(g(), seat, prompt, sa, null, opts, false, "CounterType"));
        return r != null ? r : super.chooseCounterType(options, sa, prompt, params);
    }

    @Override
    public ICardFace chooseSingleCardFace(SpellAbility sa, List<ICardFace> faces, String message) {
        // je Seite eine große Kopie mit Name, Typ, Kosten und Text dieser Seite (eigene IDs)
        List<Decisions.Opt> opts = new ArrayList<>();
        com.google.gson.JsonArray extra = new com.google.gson.JsonArray();
        // Dungeon wählen (Venture, CR 701.49): je Dungeon seine Räume für die Dungeon-Ansicht
        boolean dungeons = !faces.isEmpty() && faces.stream().allMatch(f -> f.getType() != null && f.getType().isDungeon());
        for (int k = 0; k < faces.size(); k++) {
            ICardFace f = faces.get(k);
            int id = FACE_ID_BASE + k;
            opts.add(new Decisions.Opt(f.getName(), null, id));
            com.google.gson.JsonObject fj = faceJson(id, f.getName(), String.valueOf(f.getType()),
                    f.getManaCost() == null ? "" : f.getManaCost().getShortString(), f.getOracleText(),
                    f.getPower(), f.getToughness());
            if (dungeons) {
                fj.add("dungeon", StateJson.dungeonJson(f.getOracleText(), ""));
                String dimg = CardImages.tokenImageByName(f.getName());
                if (!dimg.isEmpty()) {
                    fj.addProperty("image", dimg);
                }
            }
            String img = dungeons ? "" : CardImages.faceImage(sa == null ? null : sa.getHostCard(), f.getName());
            if (!img.isEmpty()) {
                fj.addProperty("image", img);
            }
            extra.add(fj);
        }
        ICardFace r = pick(faces, Decisions.cardOption(g(), seat, message, opts, null, extra, dungeons ? "Dungeon" : "CardFace", sa, false));
        return r != null ? r : super.chooseSingleCardFace(sa, faces, message);
    }

    @Override
    public CardState chooseSingleCardState(SpellAbility sa, List<CardState> states, String message,
            Map<String, Object> params) {
        List<Decisions.Opt> opts = new ArrayList<>();
        com.google.gson.JsonArray extra = new com.google.gson.JsonArray();
        for (int k = 0; k < states.size(); k++) {
            CardState st = states.get(k);
            int id = FACE_ID_BASE + k;
            opts.add(new Decisions.Opt(st.getName(), null, id));
            com.google.gson.JsonObject fj = faceJson(id, st.getName(), String.valueOf(st.getType()),
                    st.getManaCost() == null ? "" : st.getManaCost().getShortString(), st.getOracleText(),
                    st.getBasePowerString(), st.getBaseToughnessString());
            String img = CardImages.faceImage(st.getCard(), st.getName());
            if (!img.isEmpty()) {
                fj.addProperty("image", img);
            }
            extra.add(fj);
        }
        CardState r = pick(states, Decisions.cardOption(g(), seat, message, opts, null, extra, "CardFace", sa, false));
        return r != null ? r : super.chooseSingleCardState(sa, states, message, params);
    }

    /** Farbbuchstabe einer Schutz-Art („white“ → W …), sonst {@code null} (z. B. „artifacts“). */
    private static String protectionColor(String choice) {
        return switch (choice == null ? "" : choice.trim().toLowerCase()) {
            case "white" -> "W";
            case "blue" -> "U";
            case "black" -> "B";
            case "red" -> "R";
            case "green" -> "G";
            default -> null;
        };
    }

    /** Pool-Mana selbst wählen lassen ({@code ChooseOption} „ManaPool“)? Aus: Forge wählt. */
    private static final boolean ASK_POOL_CHOICE = false;

    /** Läuft gerade das automatische Bezahlen ({@code AutoPay})? Dann keine Pool-Rückfragen. */
    private boolean autoPaying;

    /** Erste ID der Kartendaten für Seiten einer Karte (weit über allen Forge-IDs). */
    private static final int FACE_ID_BASE = 1_900_000_000;

    /** Kartendaten einer Seite (Name, Typ, Kosten, Text, Werte) wie {@code StateJson.card}. */
    static com.google.gson.JsonObject faceJson(int id, String name, String type, String cost, String text,
            String power, String toughness) {
        com.google.gson.JsonObject o = new com.google.gson.JsonObject();
        o.addProperty("id", id);
        o.addProperty("name", name == null ? "" : name);
        o.addProperty("type", type == null ? "" : type);
        o.addProperty("cost", cost == null ? "" : cost);
        o.addProperty("text", text == null ? "" : text.replace("\\n", "\n"));
        try {
            if (power != null && !power.isBlank() && toughness != null && !toughness.isBlank()) {
                o.addProperty("power", Integer.parseInt(power.trim()));
                o.addProperty("toughness", Integer.parseInt(toughness.trim()));
            }
        } catch (NumberFormatException e) {
            // „*“ o. Ä.: ohne Werte
        }
        return o;
    }

    @Override
    public Player chooseStartingPlayer(boolean isFirstGame) {
        List<Player> players = new ArrayList<>(g().getPlayers());
        List<Integer> seats = new ArrayList<>();
        List<String> names = new ArrayList<>();
        for (Player p : players) {
            seats.add(StateJson.indexOf(g(), p));
            names.add(p.getName());
        }
        Player r = pick(players, Decisions.startingPlayer(g(), seat, seats, names));
        return r != null ? r : super.chooseStartingPlayer(isFirstGame);
    }

    @Override
    public Object vote(SpellAbility sa, String prompt, List<Object> options, ListMultimap<Object, Player> votes,
            Player forPlayer, boolean optional) {
        Object r = pick(options, Decisions.option(g(), seat, prompt, sa, null, Decisions.opts(options),
                optional, "Vote"));
        return r != null ? r : optional ? null : super.vote(sa, prompt, options, votes, forPlayer, optional);
    }

    @Override
    public Integer chooseRollToIgnore(List<Integer> rolls) {
        Integer r = pick(rolls, Decisions.option(g(), seat, "Welchen Wurf ignorieren?", null, null,
                Decisions.opts(rolls), false, "Dice"));
        return r != null ? r : super.chooseRollToIgnore(rolls);
    }

    @Override
    public Integer chooseRollToModify(List<Integer> rolls) {
        Integer r = pick(rolls, Decisions.option(g(), seat, "Welchen Wurf verändern?", null, null,
                Decisions.opts(rolls), true, "Dice"));
        return r;
    }

    @Override
    public List<Integer> chooseDiceToReroll(List<Integer> rolls) {
        List<Integer> chosen = Decisions.options(g(), seat, "Welche Würfel neu werfen?", null, null,
                Decisions.opts(rolls), 0, rolls.size(), false, "Dice");
        if (chosen == null) {
            return super.chooseDiceToReroll(rolls);
        }
        List<Integer> result = new ArrayList<>();
        for (int i : chosen) {
            result.add(rolls.get(i));
        }
        return result;
    }

    @Override
    public int chooseSprocket(Card assignee, List<Integer> sprockets) {
        Integer r = pick(sprockets, Decisions.option(g(), seat, "Wähle ein Zahnrad", null, assignee,
                Decisions.opts(sprockets), false));
        return r != null ? r : super.chooseSprocket(assignee, sprockets);
    }

    // ------------------------------------------------------------------ Ja/Nein

    @Override
    public boolean confirmAction(SpellAbility sa, PlayerActionConfirmMode mode, String message,
            List<String> options, Card cardToShow, Map<String, Object> params) {
        Boolean r = options != null && options.size() == 2
                ? Decisions.binary(g(), seat, message, sa, cardToShow, options.get(0), options.get(1))
                : Decisions.confirm(g(), seat, message, sa, cardToShow);
        return r != null ? r : super.confirmAction(sa, mode, message, options, cardToShow, params);
    }

    @Override
    public boolean confirmTrigger(WrappedAbility wrapper) {
        Boolean r = Decisions.optionalTrigger(g(), seat, wrapper.toUnsuppressedString(), wrapper,
                wrapper.getHostCard());
        return r != null ? r : super.confirmTrigger(wrapper);
    }

    @Override
    public boolean confirmReplacementEffect(ReplacementEffect replacementEffect, SpellAbility effectSA,
            GameEntity affected, String question) {
        Boolean r = Decisions.confirm(g(), seat, question, effectSA, replacementEffect.getHostCard());
        return r != null ? r : super.confirmReplacementEffect(replacementEffect, effectSA, affected, question);
    }

    @Override
    public boolean confirmStaticApplication(Card hostCard, PlayerActionConfirmMode mode, String message,
            String logic) {
        Boolean r = Decisions.confirm(g(), seat, message, null, hostCard);
        return r != null ? r : super.confirmStaticApplication(hostCard, mode, message, logic);
    }

    @Override
    public boolean chooseBinary(SpellAbility sa, String question, BinaryChoiceType kindOfChoice, Boolean defaultVal) {
        String[] labels = switch (kindOfChoice) {
            case HeadsOrTails -> new String[] {"Kopf", "Zahl"};
            case TapOrUntap -> new String[] {"Tappen", "Enttappen"};
            case PlayOrDraw -> new String[] {"Beginnen", "Ziehen"};
            case OddsOrEvens -> new String[] {"Ungerade", "Gerade"};
            case UntapOrLeaveTapped -> new String[] {"Enttappen", "Getappt lassen"};
            case LeftOrRight -> new String[] {"Links", "Rechts"};
            case AddOrRemove -> new String[] {"Hinzufügen", "Entfernen"};
            case IncreaseOrDecrease -> new String[] {"Erhöhen", "Verringern"};
        };
        Boolean r = Decisions.binary(g(), seat, question, sa, null, labels[0], labels[1]);
        return r != null ? r : super.chooseBinary(sa, question, kindOfChoice, defaultVal);
    }

    @Override
    public boolean chooseFlipResult(SpellAbility sa, Player flipper, boolean call) {
        Boolean r = call
                ? Decisions.binary(g(), seat, "Kopf oder Zahl?", sa, null, "Kopf", "Zahl")
                : Decisions.binary(g(), seat, "Ergebnis des Münzwurfs wählen", sa, null, "Gewinnen", "Verlieren");
        return r != null ? r : super.chooseFlipResult(sa, flipper, call);
    }

    @Override
    public boolean willPutCardOnTop(Card c) {
        Boolean r = Decisions.binary(g(), seat, "Karte oben auf die Bibliothek legen?", null, c, "Oben",
                "Unten");
        return r != null ? r : super.willPutCardOnTop(c);
    }

    // ------------------------------------------------------------------ Zahlen

    @Override
    public int chooseNumber(SpellAbility sa, String title, int min, int max) {
        Integer r = Decisions.number(g(), seat, title, sa, null, min, max, false);
        return r != null ? r : super.chooseNumber(sa, title, min, max);
    }

    @Override
    public int chooseNumber(SpellAbility sa, String title, List<Integer> values, Player relatedPlayer) {
        Integer r = pick(values, Decisions.option(g(), seat, title, sa, null, Decisions.opts(values),
                false, "NumberList"));
        return r != null ? r : super.chooseNumber(sa, title, values, relatedPlayer);
    }

    /** X-Kosten und andere angesagte Werte ({@code ChooseX}). Abbrechen = Zauber nicht spielen. */
    @Override
    public Integer announceRequirements(SpellAbility ability, int min, int max, String announce) {
        return Decisions.chooseX(g(), seat, ability, announce, min, max);
    }

    @Override
    public int chooseNumberForCostReduction(SpellAbility sa, int min, int max) {
        Integer r = Decisions.number(g(), seat, "Um wie viel die Kosten verringern?", sa, null, min, max, false);
        return r != null ? r : super.chooseNumberForCostReduction(sa, min, max);
    }

    @Override
    public int chooseNumberForKeywordCost(SpellAbility sa, Cost cost, KeywordInterface keyword, String prompt,
            int max) {
        // Multikicker, Replicate, Squad: „beliebig oft“ → so oft, wie das verfügbare Mana nach den
        // Grundkosten reicht (Schätzung der Engine: Manaquellen + Pool), höchstens 20
        if (max == Integer.MAX_VALUE && cost != null && cost.getTotalMana() != null) {
            int per = Math.max(1, cost.getTotalMana().getCMC());
            int base = sa.getPayCosts() == null || sa.getPayCosts().getTotalMana() == null ? 0
                    : sa.getPayCosts().getTotalMana().getCMC();
            int avail = forge.ai.ComputerUtilMana.getAvailableManaEstimate(getPlayer(), false)
                    + getPlayer().getManaPool().totalMana();
            max = Math.max(0, Math.min(20, (avail - base) / per));
        }
        Integer r = Decisions.number(g(), seat, prompt, sa, null, 0, max, false);
        return r != null ? r : super.chooseNumberForKeywordCost(sa, cost, keyword, prompt, max);
    }

    // ------------------------------------------------------------------ Farben

    private static List<Decisions.Opt> colorOptions(List<MagicColor.Color> colors) {
        List<Decisions.Opt> opts = new ArrayList<>();
        for (MagicColor.Color c : colors) {
            opts.add(new Decisions.Opt(c.getTranslatedName(), c.getShortName().toUpperCase()));
        }
        return opts;
    }

    @Override
    public byte chooseColor(String message, SpellAbility sa, ColorSet colors) {
        List<MagicColor.Color> list = new ArrayList<>(colors.getOrderedColors());
        List<String> letters = new ArrayList<>();
        for (MagicColor.Color c : list) {
            letters.add(c.getShortName().toUpperCase());
        }
        MagicColor.Color r = pick(list, Decisions.color(g(), seat, message, sa, letters));
        return r != null ? r.getColorMask() : super.chooseColor(message, sa, colors);
    }

    @Override
    public byte chooseColorAllowColorless(String message, Card c, ColorSet colors) {
        List<MagicColor.Color> list = new ArrayList<>(colors.getOrderedColors());
        List<Decisions.Opt> opts = colorOptions(list);
        opts.add(new Decisions.Opt("Farblos", "C"));
        Integer i = Decisions.option(g(), seat, message, null, c, opts, false);
        if (i == null) {
            return super.chooseColorAllowColorless(message, c, colors);
        }
        return i == list.size() ? MagicColor.COLORLESS : list.get(i).getColorMask();
    }

    @Override
    public ColorSet chooseColors(String message, SpellAbility sa, int min, int max, ColorSet options) {
        List<MagicColor.Color> list = new ArrayList<>(options.getOrderedColors());
        if (min == 1 && max == 1) {
            // genau eine Farbe: ChooseColor des Basismodells
            byte c = chooseColor(message, sa, options);
            return ColorSet.fromMask(c);
        }
        List<Integer> chosen = Decisions.options(g(), seat, message, sa, null, colorOptions(list), min, max, false);
        if (chosen == null) {
            return super.chooseColors(message, sa, min, max, options);
        }
        int mask = 0;
        for (int i : chosen) {
            mask |= list.get(i).getColorMask();
        }
        return ColorSet.fromMask(mask);
    }

    // ------------------------------------------------------------------ Kartennamen

    /** Typen auf den eigenen Karten (alle Zonen, auch Bibliothek: die Deckliste kennt der Spieler):
     *  jedes Wort der Typzeile, z. B. Kreaturtypen für „Wähle einen Kreaturtyp“ (CR 205.3m). */
    private java.util.Set<String> ownTypes() {
        java.util.Set<String> out = new java.util.HashSet<>();
        for (Card c : getPlayer().getAllCards()) {
            out.addAll(c.getType().getSubtypes());
            for (String w : c.getType().toString().split("[\\s—-]+")) {
                if (!w.isEmpty()) {
                    out.add(w);
                }
            }
        }
        return out;
    }

    /** Kartennamen, die der Spieler kennt: eigene Karten (alle Zonen) und offen liegende Karten der
     *  Gegner (Spielfeld, Friedhof, Exil, Stack, Kommandozone). */
    private java.util.Set<String> seenNames() {
        java.util.Set<String> out = new java.util.HashSet<>();
        for (Card c : getPlayer().getAllCards()) {
            out.add(c.getName());
        }
        for (Player p : getPlayer().getOpponents()) {
            for (Card c : p.getCardsIn(ZoneType.Battlefield, ZoneType.Graveyard, ZoneType.Exile, ZoneType.Stack,
                    ZoneType.Command)) {
                if (!c.isFaceDown()) {
                    out.add(c.getName());
                }
            }
        }
        return out;
    }

    @Override
    public String chooseCardName(SpellAbility sa, List<ICardFace> faces, String message) {
        List<String> names = new ArrayList<>();
        for (ICardFace f : faces) {
            names.add(f.getName());
        }
        String r = pick(names, Decisions.nameOrType(g(), seat, "NameCard", message, sa, null, names, null, seenNames(), false));
        return r != null ? r : super.chooseCardName(sa, faces, message);
    }

    /** Kartenname mit Bedingung: {@code NameCard} mit allen erlaubten Namen (sortiert, eindeutig). */
    @Override
    public String chooseCardName(SpellAbility sa, Predicate<ICardFace> cpp, String valid, String message) {
        TreeSet<String> names = new TreeSet<>();
        for (ICardFace f : StaticData.instance().getCommonCards().getAllFaces()) {
            if (cpp.test(f)) {
                names.add(f.getName());
            }
        }
        List<String> list = new ArrayList<>(names);
        String r = pick(list, Decisions.nameOrType(g(), seat, "NameCard", message, sa, null, list, null, seenNames(), false));
        return r != null ? r : super.chooseCardName(sa, cpp, valid, message);
    }
}
