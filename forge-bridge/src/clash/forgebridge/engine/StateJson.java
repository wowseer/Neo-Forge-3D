package clash.forgebridge.engine;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;

import forge.card.MagicColor;
import forge.game.Game;
import forge.game.GameEntity;
import forge.game.card.Card;
import forge.game.card.CounterType;
import forge.game.combat.Combat;
import forge.game.phase.PhaseHandler;
import forge.game.player.Player;
import forge.game.spellability.SpellAbility;
import forge.game.spellability.SpellAbilityStackInstance;
import forge.game.zone.ZoneType;

/** Wandelt Forge-Spielzustand in JSON für den Client um. */
final class StateJson {
    private StateJson() {}

    /**
     * Momentaufnahme des Spiels. {@code viewer} = Sitzplatz, dessen Handkarten offen
     * mitgeschickt werden (-1 = keine Hand offen; Gegnerhände nur als Anzahl).
     */
    static JsonObject snapshot(Game game, int viewer) {
        JsonObject s = new JsonObject();
        PhaseHandler ph = game.getPhaseHandler();
        s.addProperty("turn", ph.getTurn());
        s.addProperty("phase", ph.getPhase() == null ? null : ph.getPhase().name());
        Player active = ph.getPlayerTurn();
        s.addProperty("active_player", active == null ? -1 : indexOf(game, active));

        JsonArray players = new JsonArray();
        int i = 0;
        for (Player p : game.getRegisteredPlayers()) {
            JsonObject po = new JsonObject();
            po.addProperty("seat", i);
            po.addProperty("name", p.getName());
            po.addProperty("life", p.getLife());
            po.addProperty("library", p.getCardsIn(ZoneType.Library).size());
            // oberste Karte der Bibliothek, wenn der Betrachter sie sehen darf (Mystic Forge, Future
            // Sight, Courser of Kruphix, Vizier of the Menagerie, Bolas's Citadel …: Forges
            // MayLookAt bzw. aufgedeckt): die UI zeigt sie offen neben der Hand
            Card top = libraryTopVisibleTo(game, p, viewer);
            if (top != null) {
                po.add("library_top", card(top));
            }
            po.addProperty("hand_count", p.getCardsIn(ZoneType.Hand).size());
            // Manapool: Menge je Farbe in der Reihenfolge W U B R G C
            JsonArray pool = new JsonArray();
            for (byte color : MagicColor.WUBRGC) {
                pool.add(p.getManaPool().getAmountOfColor(color));
            }
            po.add("mana_pool", pool);
            // Spieler-Marken (Gift, Energie, Erfahrung …), CR 122.1
            po.add("counters", counters(p));
            if (i == viewer) {
                po.add("hand", cards(p, ZoneType.Hand));
            }
            po.add("battlefield", cards(p, ZoneType.Battlefield));
            po.add("graveyard", cards(p, ZoneType.Graveyard));
            po.add("exile", cards(p, ZoneType.Exile));
            // Kommandozone (CR 408): Commander, Embleme, Dungeons, „The Ring“ u. a.
            po.add("command", cards(p, ZoneType.Command));
            // Spieler-Kennzeichen: Stadtsegen (CR 702.131), Versuchungen durch den Ring (CR 701.52)
            po.addProperty("blessing", p.hasBlessing());
            po.addProperty("ring_level", p.getNumRingTemptedYou());
            Card bearer = p.getRingBearer();
            if (bearer != null) {
                po.addProperty("ring_bearer", bearer.getId());
            }
            players.add(po);
            i++;
        }
        s.add("players", players);
        // Monarch (CR 724), Initiative (CR 725), Tag/Nacht (CR 726; null = noch keins)
        Player monarch = game.getMonarch();
        s.addProperty("monarch", monarch == null ? -1 : indexOf(game, monarch));
        Player initiative = game.getHasInitiative();
        s.addProperty("initiative", initiative == null ? -1 : indexOf(game, initiative));
        Boolean day = game.getDayTime();
        if (day != null) {
            s.addProperty("day", day.booleanValue());
        }
        s.add("combat", combat(game));
        s.add("stack", stack(game));
        return s;
    }

    /**
     * Oberste Karte der Bibliothek von {@code owner}, wenn der Spieler am Sitzplatz {@code viewer}
     * sie gerade sehen darf ({@code CardView.canBeShownTo}: in der Bibliothek nur mit einer
     * Erlaubnis zum Ansehen bzw. Aufdecken); sonst null.
     */
    private static Card libraryTopVisibleTo(Game game, Player owner, int viewer) {
        java.util.List<Player> seats = game.getRegisteredPlayers();
        if (viewer < 0 || viewer >= seats.size()) {
            return null;
        }
        forge.game.card.CardCollectionView library = owner.getCardsIn(ZoneType.Library);
        if (library.isEmpty()) {
            return null;
        }
        Card top = library.get(0);
        return top.getView().canBeShownTo(seats.get(viewer).getView()) ? top : null;
    }

    /**
     * Wie {@link #snapshot}, aber für Abfragen außerhalb des Spiel-Threads (Befehl {@code state}):
     * Ändert der Spiel-Thread gleichzeitig Forges Listen (z. B. Marken), wirft Forge eine
     * {@link java.util.ConcurrentModificationException}; dann kurz warten und neu lesen.
     */
    static JsonObject snapshotFromOtherThread(Game game, int viewer) {
        for (int attempt = 0; ; attempt++) {
            try {
                return snapshot(game, viewer);
            } catch (java.util.ConcurrentModificationException | IndexOutOfBoundsException
                     | NullPointerException e) {
                if (attempt >= 9) {
                    throw e;
                }
                try {
                    Thread.sleep(5);
                } catch (InterruptedException ie) {
                    Thread.currentThread().interrupt();
                    throw e;
                }
            }
        }
    }

    /**
     * Räume eines Dungeons (CR 309.4) aus seinem Oracle-Text: je Zeile „Raum — Text (Leads to: A,
     * B)“ ein Eintrag {@code {name, text, next: [..]}} in Reihenfolge des Textes (erster =
     * Eingang); Zeilen ohne „—“ (z. B. Hinweise) werden übersprungen. {@code current} = aktueller
     * Raum (leer = noch keiner).
     */
    static JsonObject dungeonJson(String oracle, String current) {
        JsonObject d = new JsonObject();
        JsonArray rooms = new JsonArray();
        for (String line : (oracle == null ? "" : oracle).split("\\\\n|\\n")) {
            int dash = line.indexOf(" — ");
            if (dash <= 0) {
                continue;
            }
            String name = line.substring(0, dash).trim();
            String text = line.substring(dash + 3).trim();
            JsonArray next = new JsonArray();
            int leads = text.lastIndexOf("(Leads to:");
            if (leads >= 0) {
                String list = text.substring(leads + "(Leads to:".length()).replace(")", "");
                for (String n : list.split(",")) {
                    if (!n.isBlank()) {
                        next.add(n.trim());
                    }
                }
                text = text.substring(0, leads).trim();
            }
            JsonObject r = new JsonObject();
            r.addProperty("name", name);
            r.addProperty("text", text);
            r.add("next", next);
            rooms.add(r);
        }
        d.add("rooms", rooms);
        d.addProperty("current", current == null ? "" : current);
        return d;
    }

    /**
     * Stack (CR 405) wie {@code GameState.stack} in forge-rules-rs: unterster Eintrag zuerst, oberster
     * zuletzt. Je Eintrag {@code id}, {@code kind} ({@code Spell} / {@code Ability} /
     * {@code Trigger}), {@code source} (Karten-ID der Quelle), {@code controller} (Sitzplatz),
     * {@code text} (Forges Stack-Beschreibung), {@code targets} (Ziele je Fähigkeit der
     * Sub-Ability-Kette als Liste von Listen) und bei Zaubern {@code card} (Kartendaten der Karte
     * in der Stack-Zone). Ziele, die Zauber auf dem Stack sind, erscheinen als deren Karte.
     * Zuoberst folgen Karten, die gerade gewirkt werden ({@code kind} {@code Casting}, {@code id} =
     * minus Karten-ID, ohne Text und Ziele).
     */
    static JsonArray stack(Game game) {
        java.util.List<SpellAbilityStackInstance> items = new java.util.ArrayList<>();
        game.getStack().reverseIterator().forEachRemaining(items::add);
        JsonArray arr = new JsonArray();
        for (SpellAbilityStackInstance si : items) {
            JsonObject o = new JsonObject();
            o.addProperty("id", si.getId());
            o.addProperty("kind", si.isSpell() ? "Spell" : si.isTrigger() ? "Trigger" : "Ability");
            Card src = si.getSourceCard();
            o.addProperty("source", src == null ? -1 : src.getId());
            Player who = si.getActivatingPlayer();
            o.addProperty("controller", who == null ? -1 : indexOf(game, who));
            o.addProperty("text", si.getStackDescription());
            if (si.isSpell() && src != null) {
                o.add("card", card(src));
            }
            // modale Zauber (CR 700.2): Texte der gewählten Modi, damit die Karte nur diese zeigt
            // Forge hängt die gewählten Modi als Unterfähigkeiten mit SVar „CharmOrder“ an
            JsonArray modes = new JsonArray();
            for (SpellAbility m = si.getSpellAbility(); m != null; m = m.getSubAbility()) {
                if (m.hasSVar("CharmOrder")) {
                    String d = m.getParamOrDefault("SpellDescription", m.getDescription());
                    modes.add(d == null || d.isBlank() ? m.toUnsuppressedString() : d);
                }
            }
            if (!modes.isEmpty()) {
                o.add("modes", modes);
            }
            JsonArray targets = new JsonArray();
            for (SpellAbility sa = si.getSpellAbility(); sa != null; sa = sa.getSubAbility()) {
                JsonArray ts = new JsonArray();
                if (sa.getTargets() != null) {
                    for (Object t : sa.getTargets()) {
                        if (t instanceof GameEntity e) {
                            ts.add(Decisions.targetJson(game, e));
                        } else if (t instanceof SpellAbility tsa && tsa.getHostCard() != null) {
                            ts.add(Decisions.targetJson(game, tsa.getHostCard()));
                        }
                    }
                }
                targets.add(ts);
            }
            o.add("targets", targets);
            arr.add(o);
        }
        // Karten, die gerade gewirkt werden (CR 601.2a: schon in der Stack-Zone, aber noch kein
        // Stack-Eintrag – Ziele, Kosten …): oben als Eintrag "Casting", damit die UI sie zeigt
        java.util.Set<Integer> onStack = new java.util.HashSet<>();
        for (SpellAbilityStackInstance si : items) {
            if (si.getSourceCard() != null) {
                onStack.add(si.getSourceCard().getId());
            }
        }
        for (Card c : game.getCardsIn(ZoneType.Stack)) {
            if (onStack.contains(c.getId())) {
                continue;
            }
            JsonObject o = new JsonObject();
            o.addProperty("id", -c.getId());
            o.addProperty("kind", "Casting");
            o.addProperty("source", c.getId());
            o.addProperty("controller", c.getController() == null ? -1 : indexOf(game, c.getController()));
            o.addProperty("text", "");
            o.add("card", card(c));
            o.add("targets", new JsonArray());
            arr.add(o);
        }
        return arr;
    }

    /**
     * Laufender Kampf: {@code attackers} = Liste {@code {card, target}} (target = {@code {"card": id}}
     * für Planeswalker/Schlachten oder {@code {"player": sitz}}), {@code blocks} = Liste
     * {@code [blocker, angreifer]}. Ohne Kampf leere Listen.
     */
    static JsonObject combat(Game game) {
        JsonObject o = new JsonObject();
        JsonArray attackers = new JsonArray();
        JsonArray blocks = new JsonArray();
        Combat combat = game.getPhaseHandler().getCombat();
        if (combat != null) {
            for (Card a : combat.getAttackers()) {
                JsonObject ao = new JsonObject();
                ao.addProperty("card", a.getId());
                GameEntity def = combat.getDefenderByAttacker(a);
                if (def != null) {
                    ao.add("target", Decisions.targetJson(game, def));
                }
                attackers.add(ao);
                for (Card b : combat.getBlockers(a)) {
                    JsonArray pair = new JsonArray();
                    pair.add(b.getId());
                    pair.add(a.getId());
                    blocks.add(pair);
                }
            }
        }
        o.add("attackers", attackers);
        o.add("blocks", blocks);
        return o;
    }

    static JsonArray cards(Player p, ZoneType zone) {
        JsonArray arr = new JsonArray();
        for (Card c : p.getCardsIn(zone)) {
            if (isRulesPlumbing(c)) {
                continue;
            }
            arr.add(card(c));
        }
        return arr;
    }

    /**
     * Interne Effektkarte ohne eigene Bedeutung für den Spieler: Forges „Commander Effect“, den
     * jeder Commander-Spieler fest in der Kommandozone hat (trägt die Commander-Regeln wie
     * „aus der Kommandozone wirken“, CR 903.8). Andere Effekte (Dauer-Effekte von Zaubern,
     * Embleme) bleiben sichtbar.
     */
    private static boolean isRulesPlumbing(Card c) {
        return c.isImmutable() && !c.isEmblem() && "Commander Effect".equals(c.getName());
    }

    static JsonObject card(Card c) {
        JsonObject o = new JsonObject();
        o.addProperty("id", c.getId());
        o.addProperty("name", c.getName());
        o.addProperty("type", String.valueOf(c.getType()));
        o.addProperty("tapped", c.isTapped());
        // Kreaturen: Einsatzbereitschaft (CR 302.6, „summoning sick“) und Angreifer im laufenden Kampf
        if (c.isCreature() && c.isInPlay()) {
            if (c.isSick()) {
                o.addProperty("sick", true);
            }
            forge.game.combat.Combat combat = c.getGame().getCombat();
            if (combat != null && combat.isAttacking(c)) {
                o.addProperty("attacking", true);
            }
        }
        o.addProperty("cost", c.getManaCost() == null ? "" : c.getManaCost().getShortString());
        if (c.isCreature()) {
            o.addProperty("power", c.getNetPower());
            o.addProperty("toughness", c.getNetToughness());
            o.addProperty("base_power", c.getBasePower());
            o.addProperty("base_toughness", c.getBaseToughness());
        }
        // Loyalität / Verteidigung: im Spiel die aktuellen Marken, sonst der aufgedruckte Wert
        if (c.isPlaneswalker()) {
            Integer v = c.isInPlay() ? Integer.valueOf(c.getCurrentLoyalty()) : printed(c.getCurrentState().getBaseLoyalty());
            if (v != null) o.addProperty("loyalty", v);
        }
        if (c.isBattle()) {
            Integer v = c.isInPlay() ? Integer.valueOf(c.getCurrentDefense()) : printed(c.getCurrentState().getBaseDefense());
            if (v != null) o.addProperty("defense", v);
        }
        if (c.getDamage() > 0) {
            o.addProperty("damage", c.getDamage());
        }
        // Farben (CR 105) als Buchstaben WUBRG, leer = farblos
        forge.card.ColorSet col = c.getColor();
        StringBuilder colors = new StringBuilder();
        if (col.hasWhite()) colors.append('W');
        if (col.hasBlue()) colors.append('U');
        if (col.hasBlack()) colors.append('B');
        if (col.hasRed()) colors.append('R');
        if (col.hasGreen()) colors.append('G');
        o.addProperty("colors", colors.toString());
        // Schlüsselwörter mit Erinnerungstext (für die Kartenansicht beim Hovern), ohne Dubletten
        JsonArray keywords = new JsonArray();
        java.util.Set<String> seenKw = new java.util.HashSet<>();
        try {
            for (forge.game.keyword.KeywordInterface kw : c.getUnhiddenKeywords()) {
                String title = kw.getTitle();
                if (title == null || title.isBlank() || !seenKw.add(title)) {
                    continue;
                }
                JsonObject k = new JsonObject();
                k.addProperty("title", title);
                String reminder = kw.getReminderText();
                k.addProperty("reminder", reminder == null ? "" : reminder);
                keywords.add(k);
            }
        } catch (RuntimeException e) {
            // einzelne Schlüsselwörter ohne Text dürfen den Zustand nicht verhindern
        }
        o.add("keywords", keywords);
        // Regeltext mit den aktuellen Änderungen (wie Forges Kartendetail), sonst Oracle-Text
        String text = c.getAbilityText();
        if (text == null || text.isBlank()) {
            text = c.getOracleText();
        }
        o.addProperty("text", text == null ? "" : text.trim());
        if (c.isToken()) {
            o.addProperty("token", true);
        }
        if (c.isCommander()) {
            o.addProperty("commander", true);
            // Commander-Steuer (CR 903.8): {2} je bisherigem Wirken aus der Kommandozone; in der
            // Kommandozone zusätzlich die Kosten, mit denen er von dort gewirkt wird
            o.addProperty("commander_tax", commanderTax(c));
            if (c.isInZone(ZoneType.Command) && c.getManaCost() != null) {
                o.addProperty("cast_cost", withCommanderTax(c, c.getManaCost()).getShortString());
            }
        }
        if (c.isEmblem()) {
            o.addProperty("emblem", true);
        }
        String room = c.getCurrentRoom();
        if (room != null && !room.isEmpty()) {
            o.addProperty("room", room);
        }
        // Dungeon (CR 309): Räume mit Text und Folgeräumen, aktueller Raum
        if (c.getType().isDungeon()) {
            o.add("dungeon", dungeonJson(c.getOracleText(), room));
        }
        // Kartenbild (JPG von Scryfall im Forge-Bildordner); fehlt es, wird es im Hintergrund geladen
        CardImages.Info images = CardImages.info(c);
        if (!images.image().isEmpty()) {
            o.addProperty("image", images.image());
        }
        // nur bei Karten ohne Druck mit normalem Rahmen: Bildausschnitt für eigene Rahmen
        if (!images.art().isEmpty()) {
            o.addProperty("art", images.art());
        }
        // Bildausschnitt für die Kachel auf dem Spielfeld (Rahmen, Name, Bild, Werte)
        if (!images.crop().isEmpty()) {
            o.addProperty("crop", images.crop());
        }
        // Verdeckt (Morph, Manifest …, CR 708): Forge liefert dann die Werte der verdeckten Seite
        if (c.isFaceDown()) {
            o.addProperty("face_down", true);
        }
        // Doppelseitige / Flip-Karten (CR 711, 712): angezeigte Seite und die andere Seite
        if (c.isDoubleFaced() || c.isFlipped() || c.isModal()) {
            o.addProperty("face", c.isBackSide() || c.isFlipped() ? 1 : 0);
            forge.game.card.CardState other = c.getAlternateState();
            if (other != null && !c.isFaceDown()) {
                JsonObject oo = new JsonObject();
                oo.addProperty("name", other.getName());
                oo.addProperty("type", String.valueOf(other.getType()));
                o.add("other_face", oo);
            }
        }
        // Klassen (CR 716): aktuelle Stufe
        if (c.getType().hasSubtype("Class")) {
            o.addProperty("class_level", c.getClassLevel());
        }
        // Planeswalker (CR 606): alle Loyalitätsfähigkeiten mit Kosten und Text, auch gerade nicht
        // aktivierbare (die UI graut sie aus); "label" = Beschriftung wie in Priority.labels
        if (c.isPlaneswalker() && c.isInPlay() && !c.isFaceDown()) {
            JsonArray loy = new JsonArray();
            for (forge.game.spellability.SpellAbility sa : c.getSpellAbilities()) {
                if (!sa.isPwAbility()) {
                    continue;
                }
                String label = sa.toUnsuppressedString();
                int colon = label.indexOf(':');
                JsonObject a = new JsonObject();
                a.addProperty("label", label.trim());
                a.addProperty("cost", colon > 0 ? label.substring(0, colon).trim() : "");
                a.addProperty("text", colon > 0 ? label.substring(colon + 1).trim() : label.trim());
                loy.add(a);
            }
            if (!loy.isEmpty()) {
                o.add("loyalty_abilities", loy);
            }
        }
        // Vorbereitet (Prepare): eine Kopie ihres Zaubers kann gewirkt werden
        if (c.isPrepared()) {
            o.addProperty("prepared", true);
        }
        // Räume (CR 709.5): aufgeschlossene Türen
        if (c.isRoom() && c.isInPlay()) {
            JsonArray doors = new JsonArray();
            c.getUnlockedRoomNames().forEach(doors::add);
            o.add("unlocked_doors", doors);
        }
        Card host = c.getAttachedTo();
        if (host != null) {
            o.addProperty("attached_to", host.getId());
        }
        if (c.hasCounters()) {
            o.add("counters", counters(c));
        }
        return o;
    }

    /**
     * Marken wie {@code counters} in forge-rules-rs (Name → Anzahl), z. B. {@code {"P1P1": 2}}. Name =
     * Forges {@code CounterEnumType}-Name (P1P1, M1M1, LOYALTY, POISON …).
     */
    static JsonObject counters(GameEntity e) {
        JsonObject o = new JsonObject();
        for (com.google.common.collect.Multiset.Entry<CounterType> en : e.getCounters().entrySet()) {
            if (en.getCount() > 0) {
                CounterType t = en.getElement();
                o.addProperty(t instanceof Enum<?> en2 ? en2.name() : t.getName(), en.getCount());
            }
        }
        return o;
    }

    /** Aufgedruckter Zahlenwert ({@code null} bei leer oder „X“). */
    private static Integer printed(String v) {
        try {
            return v == null || v.isBlank() ? null : Integer.valueOf(v.trim());
        } catch (NumberFormatException e) {
            return null;
        }
    }

    static JsonObject ability(int index, SpellAbility sa) {
        JsonObject o = new JsonObject();
        o.addProperty("index", index);
        Card host = sa.getHostCard();
        o.addProperty("card_id", host == null ? -1 : host.getId());
        o.addProperty("card", host == null ? null : host.getName());
        o.addProperty("land", sa.isLandAbility());
        o.addProperty("spell", sa.isSpell());
        o.addProperty("text", sa.toUnsuppressedString());
        return o;
    }

    /**
     * Sitzplatz eines Spielers: Position in {@code getRegisteredPlayers()} – bleibt fest, auch
     * wenn Spieler ausscheiden ({@code getPlayers()} lässt Verlierer weg und würde die Sitze
     * verschieben).
     */
    static int indexOf(Game game, Player player) {
        int i = 0;
        for (Player p : game.getRegisteredPlayers()) {
            if (p == player) {
                return i;
            }
            i++;
        }
        return -1;
    }

    /** Commander-Steuer der Karte {@code c} (CR 903.8): {2} je bisherigem Wirken aus der Kommandozone. */
    static int commanderTax(Card c) {
        return c.isCommander() && c.getOwner() != null ? 2 * c.getOwner().getCommanderCast(c) : 0;
    }

    /** Manakosten {@code base} plus Commander-Steuer von {@code c}. */
    static forge.card.mana.ManaCost withCommanderTax(Card c, forge.card.mana.ManaCost base) {
        int tax = commanderTax(c);
        if (tax <= 0) {
            return base;
        }
        forge.game.mana.ManaCostBeingPaid withTax = new forge.game.mana.ManaCostBeingPaid(base);
        withTax.increaseGenericMana(tax);
        return withTax.toManaCost();
    }
}
