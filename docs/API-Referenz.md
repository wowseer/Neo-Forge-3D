# API-Referenz (automatisch erzeugt)

Alle Godot-Klassen der Rust-Erweiterung mit Eigenschaften, Funktionen und Signalen.
Erzeugt mit `python3 tools/gen_api_doc.py` aus den Rust-Doku-Kommentaren – dieselben
Texte zeigt der Godot-Editor unter **Hilfe → Hilfe durchsuchen (F1)**.

Sitzplätze werden als Zahl übergeben: `0` = Spieler (unten), `1` = Gegner (oben).

## Inhalt

- [ForgeBridge](#forgebridge) – erbt von `Node`
- [ForgeTable](#forgetable) – erbt von `Node`
- [CardCutout](#cardcutout) – erbt von `Control`
- [DebugPanel](#debugpanel) – erbt von `CanvasLayer` *(nur Test-Build)*
- [HourglassIcon](#hourglassicon) – erbt von `Control`
- [TurnHud](#turnhud) – erbt von `CanvasLayer`
- [ManaCost](#manacost) – erbt von `Resource`
- [ManaPool](#manapool) – erbt von `Node`
- [ManaPoolView](#manapoolview) – erbt von `Control`
- [PhaseTimeline](#phasetimeline) – erbt von `Control`
- [PlayerSeat](#playerseat) – erbt von `Node3D`
- [SeatBadge](#seatbadge) – erbt von `Control`
- [Hello](#hello) – erbt von `Node2D`
- [Arena](#arena) – erbt von `Node3D`
- [Button3D](#button3d) – erbt von `Area3D`
- [Card3D](#card3d) – erbt von `Area3D`
- [CardInspector](#cardinspector) – erbt von `Node3D`
- [CostSymbolCanvas](#costsymbolcanvas) – erbt von `Control`
- [DungeonCanvas](#dungeoncanvas) – erbt von `Control`
- [DungeonView](#dungeonview) – erbt von `Node3D`
- [FramePacer](#framepacer) – erbt von `Node`
- [ManaFx](#manafx) – erbt von `Node3D`
- [LoyaltyView](#loyaltyview) – erbt von `Node3D`
- [ManaWheel](#manawheel) – erbt von `Node3D`
- [SearchList](#searchlist) – erbt von `Node3D`
- [Showcase](#showcase) – erbt von `Node3D`
- [Slider3D](#slider3d) – erbt von `Area3D`
- [TableCamera](#tablecamera) – erbt von `Camera3D`
- [TableController](#tablecontroller) – erbt von `Node3D`
- [TargetArrows](#targetarrows) – erbt von `Node3D`
- [FlickerLight](#flickerlight) – erbt von `OmniLight3D`
- [Tavern](#tavern) – erbt von `Node3D`
- [Zone](#zone) – erbt von `Area3D`

## ForgeBridge

*Erbt von `Node` · Quelle: `rust/src/forge/bridge.rs`*

Verbindung zur Forge-Regel-Engine: startet die Java-Brücke (`forge-bridge/`) als eigenen Prozess bzw. verbindet sich mit einem Gastgeber und spricht mit ihr über JSON-Zeilen (Protokoll: `docs/Forge-Protocol.md`). Ablauf: `start_bridge()` → Signal [signal bridge_ready] → `start_match()` → Signale [signal forge_event] (jedes Spielereignis) und [signal decision_requested] (ferngesteuerter Spieler hat Priorität / Mulligan) → Antwort mit `answer()` → [signal game_over]. Ferngesteuert sind die Sitzplätze aus `remote_seats`; alle anderen spielt die Forge-KI. Zahlen in empfangenen Dictionaries sind ganzzahlig (int), wenn sie keinen Nachkommaanteil haben.

### Eigenschaften

| Name | Typ | Beschreibung |
|---|---|---|
| `java_path` | `String` | Java-Programm (Name im PATH oder absoluter Pfad). Java 17 oder neuer. |
| `forge_dir` | `String` | Forge-Installationsordner (enthält `res/`). Leer = Umgebungsvariable `FORGE_DIR`, sonst die Projektkopie `forge-bridge/forge/` (falls vorhanden), sonst `~/Games/Forge`. |
| `forge_jar` | `String` | Forge-Jar mit allen Abhängigkeiten. Leer = erste Datei `forge-gui-desktop-*-jar-with-dependencies.jar` in `forge_dir`. |
| `bridge_jar` | `String` | Gebaute Brücke (`forge-bridge/build.sh`). `res://`-Pfade werden umgerechnet. |
| `java_max_memory_mb` | `int` | Maximaler Java-Heap in MB (`-Xmx`). |
| `ai_pace` | `float` | Tempo der Forge-KI: Faktor für die Pausen nach ihren Aktionen (Land, Zauber, Angriff, Blocker, Phasenwechsel), damit man ihrem Zug zusehen kann. 1 = normal, 2 = doppelt so langsam, 0 = ohne Pausen. Wird bei `start_match()` an die Brücke geschickt. |
| `game_type` | `String` | Spielart für `start_match()`: leer = Constructed, `Commander` = Commander-Partie (Decks mit Abschnitt ``Commander``, CR 903). Eingebettet in NeoForge legt der Gastgeber die Spielart fest. |
| `autostart` | `bool` | Brücke automatisch in `ready()` starten. |
| `log_messages` | `bool` | Jede empfangene Nachricht in die Godot-Konsole schreiben (zum Debuggen). |

### Funktionen

| Funktion | Rückgabe | Beschreibung |
|---|---|---|
| `start_bridge()` | `bool` | Startet den Java-Prozess. Gibt false zurück, wenn er schon läuft oder nicht starten konnte (dann zusätzlich [signal bridge_error]). Fertig ist er erst mit [signal bridge_ready] (Forge braucht einige Sekunden zum Laden). |
| `is_hosted()` | `bool` | Läuft die Brücke eingebettet in einem Gastgeber (NeoForge startet die Partie)? |
| `stop_bridge()` | `void` | Beendet die Brücke (Befehl "quit", nach 1 s hart beendet). Sendet [signal bridge_stopped] mit -1, falls sie lief. |
| `is_running()` | `bool` | Läuft der Java-Prozess bzw. die Verbindung zum Gastgeber? |
| `is_ready()` | `bool` | Hat Forge fertig geladen ([signal bridge_ready] empfangen)? |
| `send_command(command: Dictionary)` | `bool` | Schickt einen beliebigen Befehl (Dictionary mit "cmd", siehe Protokoll). false, wenn die Brücke nicht läuft. |
| `ping(nonce: int)` | `bool` | Verbindungstest; Antwort kommt als Nachricht `{type: "pong", nonce}`. |
| `start_match(decks: PackedStringArray, remote_seats: PackedInt32Array, names: PackedStringArray, seed: int)` | `bool` | Startet eine Partie. `decks`: Pfade zu Forge-`.dck`-Dateien (`res://` erlaubt) oder Namen von Decks aus Forges Constructed-Sammlung, einer je Sitzplatz (mind. 2). `remote_seats`: Sitzplätze, die Neo Forge 3D steuert (Rest = Forge-KI). `names`: Spielernamen (darf kürzer sein). `seed` < 0 = zufällig. |
| `answer(request_id: int, choice: Variant)` | `bool` | Beantwortet eine Anfrage aus [signal decision_requested]. `choice` ist eine Antwort im Format des Protokoll-Modells (Protokoll v3), z. B. `{"type": "Action", "action": {"type": "Pass"}}`; Aufbau siehe `docs/Forge-Protocol.md`. |
| `request_state(viewer: int)` | `bool` | Fragt den aktuellen Spielzustand ab → [signal state_received]. `viewer` = Sitzplatz, dessen Handkarten mitgeschickt werden (-1 = keiner). |
| `request_card_info(names: PackedStringArray)` | `bool` | Kartendaten zu Kartennamen anfordern → [signal card_info_received] (z. B. für Treffer bei „Kartennamen nennen“). |
| `debug_request_card_names()` | `bool` | Debug: alle Kartennamen anfordern → [signal debug_card_names_received]. |
| `debug_add_card(seat: int, name: String)` | `bool` | Debug: Karte `name` neu erzeugen und Sitzplatz `seat` auf die Hand legen. Danach kommt der neue Zustand; eine offene Priorität wird neu gestellt. |
| `debug_infinite_mana(seat: int, on: bool)` | `bool` | Debug: unendliches Mana für Sitzplatz `seat` an/aus (Pool wird bei jedem Phasenwechsel und Zauber auf 50 je Manaart aufgefüllt). |

### Signale

| Signal | Beschreibung |
|---|---|
| `bridge_ready(protocol: int)` | Die Brücke ist gestartet und Forge geladen; `protocol` = Protokollversion. |
| `message_received(message: Dictionary)` | Jede empfangene Nachricht (vor den speziellen Signalen), z. B. für eigene Auswertung. |
| `forge_event(event: Dictionary)` | Ein Forge-Spielereignis: `{event, text, turn, phase}` (`event` = Forge-Klassenname, z. B. "GameEventCardChangeZone"). |
| `log_entry(entry: Dictionary)` | Neuer Eintrag im Spielprotokoll der Engine: `{kind, text, card, turn}` (`kind` z. B. "ZONE_CHANGE", "DAMAGE", "STACK_ADD", "TURN"; `card` = Karten-ID der Quelle, falls bekannt). |
| `decision_requested(request: Dictionary)` | Ein ferngesteuerter Spieler muss entscheiden (Protokoll v3): `{id, player, decision, prompt, source, ability, cancelable, labels, state}`; `decision` ist eine `Decision` (z. B. `{"type": "Priority", "actions": […]}`). Antwort mit `answer()` und einer `Choice`. Aufbau siehe `docs/Forge-Protocol.md`. |
| `decision_rejected(info: Dictionary)` | Die gewählte Option wurde von Forge abgelehnt (`{player, option, reason}`); es folgt eine neue Anfrage. |
| `match_started(state: Dictionary)` | Die Partie hat begonnen; `state` = Spielzustand (siehe Protokoll). |
| `state_received(state: Dictionary)` | Antwort auf `request_state()`. |
| `debug_card_names_received(names: PackedStringArray)` | Antwort auf `debug_request_card_names()`: alle Kartennamen der Kartendatenbank, sortiert (Debug-Fenster). |
| `card_info_received(cards: VarArray)` | Antwort auf `request_card_info()`: Kartendaten (`{name, type, cost, text, power, toughness, image}`) je bekanntem Namen. |
| `game_over(result: Dictionary)` | Die Partie ist vorbei: `{winner, draw, reason}`. |
| `look_received(look: Dictionary)` | Aussehen eines Sitzplatzes vom Gastgeber (eingebettet, z. B. NeoForges „Anpassen“): `{seat, portrait, sleeve}` mit Pfaden zu Bilddateien (fehlend = eigenes Aussehen). |
| `bridge_error(code: String, message: String)` | Fehler der Brücke oder beim Start (`code` z. B. "deck_not_found", "spawn_failed"). |
| `bridge_stopped(exit_code: int)` | Der Java-Prozess ist beendet (`exit_code` -1 = unbekannt / von uns beendet). |

## ForgeTable

*Erbt von `Node` · Quelle: `rust/src/forge/table.rs`*

Spielt eine Partie mit der Forge-Regel-Engine auf dem 3D-Tisch: Karten, Zonen, Kamera, Drag-and-Drop und HUD sind dieselben wie im normalen Spiel, nur die Regeln kommen von Forge (über `ForgeBridge`). Forge-Sitzplatz 0 ist der Spieler (unten, von dir gesteuert), Sitzplatz 1 der Gegner (oben, Forge-KI). Länder liegen in der Ressourcen-Reihe, alle anderen bleibenden Karten auf dem Spielfeld, der Friedhof im Ablagestapel; die Gegnerhand wird verdeckt angezeigt. Bedienung: Hat der Spieler Priorität, leuchten alle Handkarten, die er jetzt spielen kann. Eine solche Karte aus der Hand auf den Tisch ziehen = spielen (Mana zahlt Forge automatisch). Hauptbutton / Leertaste = passen. Mulligan und alle allgemeinen Entscheidungen (Optionen, Modi, Ja/Nein, Zahlen, Farben, Kartennamen) im Auswahlfenster des HUD; die Karte, um die es geht, wird dabei golden markiert. Ziele, Angreifer, Blocker und Manabezahlung wählt vorerst die Forge-KI für dich.

### Eigenschaften

| Name | Typ | Beschreibung |
|---|---|---|
| `bridge` | `ForgeBridge` | Verbindung zu Forge. |
| `controller` | `TableController` | Tischsteuerung (Drag-and-Drop, Kamera). Sie kennt keine Regeln; dieser Knoten prüft die Züge über Forge. |
| `player_seat` | `PlayerSeat` | Sitzplatz des Spielers (unten) = Forge-Sitzplatz 0. |
| `opponent_seat` | `PlayerSeat` | Sitzplatz des Gegners (oben) = Forge-Sitzplatz 1. |
| `stack_zone` | `Zone` | Stack-Zone (Art 4) in der Tischmitte: Zauber liegen dort als ihre Karte, aktivierte und ausgelöste Fähigkeiten als Karte ihrer Quelle; der oberste Eintrag liegt rechts oben. Pfeile zeigen die Ziele jedes Eintrags. Leer = Stack nur in der Liste im HUD. |
| `player_deck` | `String` | Deck des Spielers: Forge-`.dck`-Datei (`res://` erlaubt) oder Forge-Deckname. |
| `opponent_deck` | `String` | Deck des Gegners: Forge-`.dck`-Datei (`res://` erlaubt) oder Forge-Deckname. |
| `seed` | `int` | Zufallsstartwert der Partie (< 0 = zufällig). |
| `autostart` | `bool` | Brücke und Partie automatisch beim Start der Szene starten. |
| `refresh_interval` | `float` | Mindestabstand (Sekunden) zwischen zwei Zustandsabfragen, während Forge Ereignisse sendet. |

### Funktionen

| Funktion | Rückgabe | Beschreibung |
|---|---|---|
| `start_match()` | `bool` | Startet die Partie mit `player_deck` gegen `opponent_deck` (Brücke muss bereit sein). Gibt false zurück, wenn das nicht ging. |
| `pass_priority()` | `void` | Phasen-Knopf / Leertaste: beantwortet eine offene `Priority`-Entscheidung mit `Action::Pass`. Im Kampf bestätigt er den Plan auf dem Tisch; ist bei der Angriffserklärung noch keine Kreatur gewählt, wählt er alle möglichen Angreifer („Alle greifen an“), bestätigt wird dann mit dem nächsten Druck („2 Angreifer“). |
| `cancel_cast()` | `void` | Wirken bzw. Aktivieren abbrechen (✕ an der Karte, Escape, Karte zurück auf die Hand ziehen): beantwortet die offene Frage mit Abbrechen, wenn die Engine das erlaubt und die Frage zum Wirken einer eigenen Karte bzw. Aktivieren einer eigenen Fähigkeit gehört. Bei einer offenen Bühne (z. B. Kicker) wirkt Escape wie ihr „Abbrechen“, wo erlaubt. |
| `on_zone_clicked(zone: Zone)` | `void` | Anklickbare Zonen-Fläche angeklickt (vom `TableController`): Exil-Strudel → Exil-Ansicht. |
| `close_zone_viewer()` | `void` | Friedhof-/Exil-Ansicht schließen; die offene Entscheidung zeigt wieder ihre Bedienung. |
| `on_wheel_multi_done()` | `void` | „Fertig“ der Mehrfachwahl im Farbrad: gewählte Farben an Forge (`Modes`). |
| `on_wheel_picked(index: int)` | `void` | Segment `index` des Farbrads angeklickt: die zugehörige Antwort an Forge. |
| `end_turn()` | `void` | Knopf „»»“ (Sofort beenden, nur im eigenen Zug): bis zum Ende dieser Runde automatisch passen; es wird ohne Angreifer weitergegangen. Andere Entscheidungen (Blocker, Auswahl, Ziele) fragt die UI weiter. Legt ein anderer Spieler etwas auf den Stack, endet das automatische Passen, damit man antworten kann. |
| `is_ending_turn()` | `bool` | Ist „Sofort beenden“ gerade aktiv? |
| `press_secondary()` | `void` | Zweiter Knopf über dem Phasen-Knopf (nur im Kampf): bei der Angriffserklärung ohne gewählte Angreifer „Kein Angriff“ (ohne Angreifer weiter), sonst „Angriff abbrechen“ (Auswahl leeren, zurück zu „Kein Angriff“ / „Alle greifen an“); bei der Blockerklärung „Blocks abbrechen“. Ohne Wirkung, wenn gerade kein zweiter Knopf angeboten wird. |
| `submit_choice(choice: Variant)` | `void` | Eingabe aus dem Auswahlfenster des HUD für die offene Entscheidung: eine Option = Index, mehrere = Array von Indizes, Ja/Nein und Mulligan = bool (true = erste Option / behalten), Zahl = int, Abbrechen = null. Wird in die passende `Choice` übersetzt; unpassende Eingaben werden ignoriert. Ist das Fähigkeiten-Menü offen, wählt die Eingabe eine Aktion daraus (null = Menü schließen). |
| `pool_clicked(mana: int)` | `void` | Ein Mana-Symbol der Manapool-Anzeige wurde angeklickt (`mana` = Manaart 0 = Weiß … 5 = Farblos): beim Bezahlen dieses schwebende Mana einsetzen (`PayAction::UsePool`), sonst nichts. |
| `on_hand_pick_done()` | `void` | „Fertig“ auf der Hand-Bühne: gewählte Karten an die Engine. |
| `on_stage_choice(index: int)` | `void` | Knopf `index` einer 3D-Bühne (Ja/Nein, Kicker/Normal …): die hinterlegte Antwort an Forge. |
| `on_modes_done()` | `void` | Modus-Bühne: „Fertig“ (mehrere Modi) – gewählte Modi an Forge. |
| `on_modes_reset()` | `void` | Modus-Bühne: Wahl zurücksetzen (bei wiederholbaren Modi). |
| `on_option_toggle(index: int)` | `void` | Optionale Kosten mit mehreren Möglichkeiten: Option `index` an/aus. |
| `on_options_done()` | `void` | Optionale Kosten mit mehreren Möglichkeiten: „Fertig“ – eingeschaltete Optionen an Forge. |
| `on_x_step(delta: int)` | `void` | X-Knöpfe: Wert um `delta` ändern (−5, −1, +1, +5), begrenzt auf 0 … Maximum. |
| `on_x_pay()` | `void` | „Bezahlen X = n“ bzw. „Bestätigen: n“: Wert an Forge. |
| `on_x_cancel()` | `void` | „Abbrechen“ auf der Zahlen-Bühne: Wirken zurücknehmen (nur wo Forge es erlaubt). |
| `on_scry_done()` | `void` | Hellsicht/Überwachen: „Fertig“ – Karten im Feld „oben“ (die rechte Karte zuoberst) und „unten“ bzw. Friedhof an Forge. |
| `on_order_done()` | `void` | Reihenfolge-Bühne: „Fertig“ – vergebene Reihenfolge an Forge. |
| `on_order_reset()` | `void` | Reihenfolge-Bühne: alle Nummern löschen. |
| `on_pick_done(decline: bool)` | `void` | Kartenwahl auf der Bühne: „Fertig“ (mehrere Karten) bzw. „Ablehnen“ (keine). |
| `inspect_card(card: Card3D)` | `void` | Kartenansicht für `card` zeigen wie beim Hovern (für Skripte und Tests; null = wieder der Karte unter der Maus folgen). |
| `on_hand_pick_cancel()` | `void` | „Abbrechen“ auf der Hand-Bühne (nur wo Forge das erlaubt). |
| `on_mulligan_pressed(keep: bool)` | `void` | Ein Mulligan-Knopf wurde gedrückt: `keep` = Starthand behalten, sonst Mulligan. |
| `on_starting_player_pressed(index: int)` | `void` | Ein Startspieler-Knopf wurde gedrückt (`index` = Eintrag in `players` der Entscheidung `ChooseStartingPlayer`). |
| `player_clicked(seat: int)` | `void` | Ein Spieler-Medaillon wurde angeklickt (`seat` = Sitzplatz): ist der Spieler ein gültiges Ziel der offenen Zielwahl, wird er gewählt bzw. abgewählt. |
| `set_choice_selection(selection: PackedInt64Array)` | `void` | Aktuelle Auswahl im Auswahlfenster (vom HUD jedes Frame gemeldet): gewählte Ziele werden auf dem Tisch golden markiert und bekommen einen Pfeil. |
| `get_card(forge_id: int)` | `Card3D` | Karte auf dem Tisch zur Forge-Karten-ID (null, wenn nicht sichtbar). |
| `get_state()` | `Dictionary` | Zuletzt von Forge empfangener Spielzustand (Aufbau siehe `docs/Forge-Protocol.md`). |
| `get_pending_kind()` | `String` | Hat der Spieler gerade eine offene Entscheidung? Liefert den Namen der `Decision` (z. B. "Priority", "Mulligan", "ChooseTargets", "ChooseOption") oder einen leeren Text. |
| `on_bridge_ready(protocol: int)` | `void` | Brücke bereit → Partie starten (bei `autostart`). |
| `on_state(state: Dictionary)` | `void` | Neuer Spielzustand von Forge (Spielstart oder Abfrage): Tisch abgleichen. |
| `on_forge_event(event: Dictionary)` | `void` | Forge-Ereignis: Zustand bald neu abfragen (gebündelt, siehe `refresh_interval`). |
| `on_decision_requested(request: Dictionary)` | `void` | Entscheidung des Spielers nötig (Protokoll v3): Zustand übernehmen, Quellkarte markieren, spielbare Karten leuchten lassen. Unbekannte Entscheidungen werden mit `Cancel` beantwortet (die Engine entscheidet dann selbst). |
| `on_game_over(result: Dictionary)` | `void` | Partie vorbei: Sieger merken (HUD zeigt ihn an). |
| `on_bridge_error(code: String, message: String)` | `void` | Fehler der Brücke: als Warnung in die Godot-Konsole schreiben. |
| `on_card_clicked(card: Card3D)` | `void` | Karte angeklickt: bei der Zielwahl = Ziel wählen/abwählen; mit Priorität = Aktion der Karte (nicht aus der Hand – die spielt man durch Ziehen): eine Aktion sofort, mehrere über das Fähigkeiten-Menü. |
| `on_look(look: Dictionary)` | `void` | Aussehen eines Sitzplatzes vom Gastgeber (NeoForge: Gesicht und Kartenhülle aus „Anpassen“): das Porträt kommt ins Medaillon, die Hülle auf die Rückseite jeder Karte des Sitzplatzes (auch der schon liegenden). |
| `on_card_info(cards: VarArray)` | `void` | Antwort auf `card_info`: Kartendaten merken; die Vorschau der Suche neu aufbauen, wenn sich bei ihrer Karte etwas Sichtbares ändert (z. B. das Bild kam). |
| `on_search_selected(row: int)` | `void` | Zeile `row` der Suchliste gewählt: „Fertig“ wird anklickbar. |
| `on_search_confirmed(row: int)` | `void` | Zweiter Klick auf die gewählte Zeile: bestätigen. |
| `on_search_done()` | `void` | „Fertig“: die gewählte Zeile bestätigen. |
| `on_search_toggle()` | `void` | „Alle zeigen“ / „Nur Vorschläge“ umschalten. |
| `on_search_pick(index: int)` | `void` | Option `index` gewählt: an die Engine. |
| `on_search_cancel()` | `void` | „Abbrechen“ in der Suche. |
| `on_divide_step(index: int, delta: int)` | `void` | „−“ / Zahl / „+“ an Ziel `index`: Menge ändern (nie unter das Mindestmaß, nie über den Rest). |
| `on_divide_done()` | `void` | „Fertig“ auf der Kampfschaden-Bühne: wie der Phasen-Knopf. |
| `on_dungeon_picked(index: int)` | `void` | Dungeon bzw. Raum gewählt: Nummer der Option an die Engine. |
| `on_dungeon_dismissed()` | `void` | Reine Dungeon-Ansicht geschlossen. |
| `on_loyalty_dismissed()` | `void` | Klick neben die Planeswalker-Ansicht: schließen, die Priorität bleibt offen. |
| `on_loyalty_picked(index: int)` | `void` | Feld der Planeswalker-Ansicht angeklickt: Fähigkeit aktivieren (Kosten, Ziele usw. fragt die Engine danach). |
| `on_action_choice_cancel()` | `void` | Knopf „Abbrechen“ bei „Wähle eins“: Bühne schließen, die Priorität bleibt offen. |
| `on_aim_released(card: Card3D, target: Card3D)` | `void` | Pfeil losgelassen (vom `TableController`): Angreifer → Spieler/Planeswalker = angreifen, Blocker → Angreifer = blocken, Quelle der Zielwahl → gültiges Ziel = Ziel wählen. Ins Leere losgelassen nimmt den Angriff bzw. Block dieser Kreatur zurück. |
| `on_card_dropped(card: Card3D, from_zone: Zone, to_zone: Zone)` | `void` | Karte wurde auf dem Tisch abgelegt: Hat sie eine Aktion in der offenen `Priority`- Entscheidung (aus der Hand: Land spielen / wirken; auf dem Spielfeld: Fähigkeit aktivieren), wird diese gewählt. Aus der Hand zurück auf die Hand = abbrechen. Alles andere wird auf den Engine-Stand zurückgesetzt. |

### Signale

| Signal | Beschreibung |
|---|---|
| `choice_option_clicked(index: int)` | Eine Option wurde auf dem Tisch gewählt (Klick auf ein gültiges Ziel – Karte oder Medaillon): `index` = Option im Auswahlfenster. Das HUD behandelt das wie einen Klick auf diese Option (zurückgestellt verbunden). |

## CardCutout

*Erbt von `Control` · Quelle: `rust/src/game/card_cutout.rs`*

Vollbild-Hülle für HUD-Elemente: Ihre Nachfahren (mit `use_parent_material = true`) werden überall gezeichnet, außer innerhalb des konvexen Vierecks `cutout` (Bildschirmkoordinaten, 4 Punkte). So wirkt eine 3D-Karte, die vor einem Medaillon schwebt, als läge sie darüber. Leeres `cutout` = nichts ausgespart. Ist eine Aussparung gesetzt (Karte gehovert/gezogen), wird der Inhalt zusätzlich weich auf 30 % Deckkraft ausgeblendet. `greyed` graut den Inhalt aus. Der `TurnHud` setzt beides jedes Frame.

### Eigenschaften

| Name | Typ | Beschreibung |
|---|---|---|
| `cutout` | `PackedVector2Array` | Konvexes Viereck (4 Punkte, Bildschirmkoordinaten), in dem die Nachfahren unsichtbar sind. Leer = keine Aussparung. |
| `greyed` | `bool` | Inhalt ausgrauen (graustufig und abgedunkelt), z. B. während die Stack-Anzeige darüber liegt. |

### Funktionen

| Funktion | Rückgabe | Beschreibung |
|---|---|---|
| `set_cutout(quad: PackedVector2Array)` | `void` | Setzt die Aussparung (4 Punkte oder leer). |
| `set_greyed(on: bool)` | `void` | Schaltet das Ausgrauen ein oder aus. |

## DebugPanel

*Erbt von `CanvasLayer` · Quelle: `rust/src/game/debug_panel.rs`*

> Existiert nur in Test-Builds (`cargo build`), nicht in `cargo build --release`.

Debug-Fenster (nur in Debug-Builds): Karten auf die Hand holen, unendliches Mana für mich und die KI. F10 blendet es ein und aus.

### Eigenschaften

| Name | Typ | Beschreibung |
|---|---|---|
| `bridge` | `ForgeBridge` | Brücke zu Forge, an die die Debug-Befehle gehen. |

### Funktionen

| Funktion | Rückgabe | Beschreibung |
|---|---|---|
| `toggle()` | `void` | Fenster ein- bzw. ausblenden (F10); beim ersten Öffnen die Kartennamen anfordern. |
| `on_card_names(names: PackedStringArray)` | `void` | Kartennamen von Forge erhalten. |
| `on_search(text: String)` | `void` | Suchtext geändert: Treffer (Teilwort, ohne Groß-/Kleinschreibung) in die Liste. |
| `on_add_mine()` | `void` | Ausgewählte Karte auf meine Hand. |
| `on_add_ai()` | `void` | Ausgewählte Karte auf die Hand der KI. |
| `on_item_activated(index: int)` | `void` | Doppelklick in der Liste: Karte auf meine Hand. |
| `on_mana_toggled(on: bool, seat: int)` | `void` | Schalter „Unendlich Mana“ für `seat` umgelegt. |

## HourglassIcon

*Erbt von `Control` · Quelle: `rust/src/game/hourglass.rs`*

Sanduhr-Symbol (HUD), z. B. neben der Rundenzahl am linken Bildrand. Größe = `icon_size` (Breite × Höhe in Pixeln; die Mindestgröße des Controls).

### Eigenschaften

| Name | Typ | Beschreibung |
|---|---|---|
| `icon_size` | `Vector2` | Breite × Höhe des Symbols in Pixeln. |

## TurnHud

*Erbt von `CanvasLayer` · Quelle: `rust/src/game/hud.rs`*

Bildschirm-Overlay für den Rundenablauf: Zeitstrahl der Phasen (`PhaseTimeline`, unten rechts über dem Hauptbutton), Spielende-Meldung in der Bildmitte, je ein `SeatBadge`-Medaillon mit Leben (an `player_hero_anchor` / `opponent_hero_anchor`, sonst links oben / unten), am linken Bildrand Spielername und darüber Rundenzahl mit Sanduhr (`HourglassIcon`; Gegner oben, Spieler unten), unten rechts der Phasen-Knopf „Passen“ (auch Leertaste; im Kampf „Alle greifen an“ / „2 Angreifer“, „Kein Block“ / „1 Blocker“) und darunter „»»“ (Sofort beenden) (beide orange animierte `Button3D`), im Kampf darüber ein zweiter Knopf („Kein Angriff“, „Angriff abbrechen“, „Blocks abbrechen“), und ein Auswahlfenster (z. B. Mulligan). Liest den Zustand jedes Frame vom `ForgeTable` (Spiel mit der Forge-Regel-Engine).

### Eigenschaften

| Name | Typ | Beschreibung |
|---|---|---|
| `forge_table` | `ForgeTable` | Das Forge-Spiel (`ForgeTable`), dessen Zustand angezeigt wird. |
| `player_hero_anchor` | `Node3D` | 3D-Punkt, an dem das Spieler-Medaillon sitzt (Unterkante mittig, z. B. direkt über der Hand). Leer = Medaillon unten links. Gezoomte oder gezogene Karten liegen sichtbar über dem Medaillon (Aussparung per `CardCutout`). |
| `opponent_hero_anchor` | `Node3D` | 3D-Punkt, an dem das Gegner-Medaillon sitzt (Oberkante mittig, z. B. direkt unter dessen Hand). Leer = Medaillon oben links. |
| `player_mana_anchor` | `Node3D` | 3D-Punkt, an dem die Mana-Symbole des Spielers enden (rechte obere Ecke, z. B. direkt unter der Ressourcen-Reihe). Leer = rechts neben dem Spieler-Medaillon. |

### Funktionen

| Funktion | Rückgabe | Beschreibung |
|---|---|---|
| `on_choice_option(index: int)` | `void` | Option `index` im Auswahlfenster angeklickt (eine Option / Ja-Nein: Antwort senden; Mehrfachauswahl: an-/abwählen). |
| `on_choice_done()` | `void` | Knopf „Fertig“ / „OK“ im Auswahlfenster (Mehrfachauswahl, Zahl, Text). |
| `on_pool_clicked(mana: int)` | `void` | Mana-Symbol im Manapool angeklickt → an den `ForgeTable` (beim Bezahlen einsetzen). |
| `badge_center(seat: int)` | `Vector2` | Mitte des Medaillons von Sitzplatz `seat` auf dem Bildschirm (Pixel); (−1, −1), wenn es (noch) keins gibt. Für 3D-Objekte, die sich am Medaillon ausrichten (Spielsteine der Kennzeichen, Knöpfe beim Verteilen). |
| `on_badge_clicked(seat: int)` | `void` | Ein Medaillon (Spieler als Ziel) wurde angeklickt: an den `ForgeTable` weitergeben. |
| `on_choice_filter_changed(text: String)` | `void` | Suchfeld des Auswahlfensters geändert: Optionen filtern. |
| `on_choice_cancel()` | `void` | Knopf „Abbrechen“ im Auswahlfenster: Antwort null. |
| `on_next_pressed()` | `void` | Phasen-Knopf bzw. Leertaste: ruft `ForgeTable.pass_priority()` auf (passen bzw. im Kampf Angreifer/Blocker bestätigen). |
| `on_secondary_pressed()` | `void` | Zweiter Knopf über dem Phasen-Knopf: ruft `ForgeTable.press_secondary()` auf („Kein Angriff“, „Angriff abbrechen“, „Blocks abbrechen“). |
| `on_end_turn_pressed()` | `void` | Knopf „Sofort beenden“: ruft `ForgeTable.end_turn()` auf. |

## ManaCost

*Erbt von `Resource` · Quelle: `rust/src/game/mana.rs`*

Kosten einer Karte, als Resource im Inspector editierbar.

### Eigenschaften

| Name | Typ | Beschreibung |
|---|---|---|
| `generic` | `int` | Beliebiges Mana. |
| `white` | `int` | Weißes Mana, das mit Weiß bezahlt werden muss. |
| `blue` | `int` | Blaues Mana, das mit Blau bezahlt werden muss. |
| `black` | `int` | Schwarzes Mana, das mit Schwarz bezahlt werden muss. |
| `red` | `int` | Rotes Mana, das mit Rot bezahlt werden muss. |
| `green` | `int` | Grünes Mana, das mit Grün bezahlt werden muss. |
| `colorless` | `int` | Muss mit farblosem Mana bezahlt werden. |

### Funktionen

| Funktion | Rückgabe | Beschreibung |
|---|---|---|
| `static parse(text: String)` | `ManaCost` | Erzeugt Kosten aus der Textschreibweise, z. B. `"2RR"`. Ungültig -> null. |
| `static from_dictionary(dict: Dictionary)` | `ManaCost` | Erzeugt Kosten aus einem Dictionary `{ ManaType: Menge, "generic": Menge }`. |
| `can_pay(pool: ManaPool)` | `bool` | Prüft, ob der Pool die Kosten bezahlen kann. |
| `get_total()` | `int` | Gesamter Manawert. |
| `to_text()` | `String` | Kurzschreibweise, z. B. "2RR". |

## ManaPool

*Erbt von `Node` · Quelle: `rust/src/game/mana.rs`*

Manavorrat eines Spielers.

### Funktionen

| Funktion | Rückgabe | Beschreibung |
|---|---|---|
| `add_mana(mana_type: int (Manafarbe), amount: int)` | `void` | Fügt Mana hinzu. |
| `remove_mana(mana_type: int (Manafarbe), amount: int)` | `bool` | Nimmt `amount` Mana der Art `mana_type` wieder aus dem Pool (z. B. Tapp zurückgenommen). Alles oder nichts: false, wenn nicht genug davon im Pool liegt. |
| `pay_mana(cost: ManaCost)` | `bool` | Bezahlt die Kosten (alles oder nichts). true = bezahlt. |
| `pay_mana_dict(cost: Dictionary)` | `bool` | Wie `pay_mana`, mit Kosten als Dictionary `{ ManaType: Menge, "generic": Menge }`. |
| `can_pay(cost: ManaCost)` | `bool` | Kann dieser Pool die Kosten `cost` bezahlen? |
| `get_amount(mana_type: int (Manafarbe))` | `int` | Vorrat einer Farbe (0 = Weiß, 1 = Blau, 2 = Schwarz, 3 = Rot, 4 = Grün, 5 = Farblos). |
| `get_total()` | `int` | Gesamtes Mana im Pool (alle Farben zusammen). |
| `empty_pool()` | `void` | Leert den Pool (z. B. am Ende jeder Phase). |
| `to_dictionary()` | `Dictionary` | Aktueller Vorrat als Dictionary `{ ManaType: Menge }`. |
| `static get_mana_color(mana_type: int (Manafarbe))` | `Color` | Anzeigefarbe eines Manatyps (für HUD-Symbole). |
| `static get_mana_name(mana_type: int (Manafarbe))` | `String` | Anzeigename eines Manatyps, z. B. tr!("Rot", "Red"). |
| `to_text()` | `String` | Lesbare Anzeige, z. B. "Rot 2 · Grün 1". |

### Signale

| Signal | Beschreibung |
|---|---|
| `pool_changed()` | Wird nach jeder Änderung ausgelöst. |

## ManaPoolView

*Erbt von `Control` · Quelle: `rust/src/game/mana_view.rs`*

Manapool als Reihe runder Symbole (HUD). Wird vom `TurnHud` jedes Frame über `set_amounts()` gefüttert; die Breite passt sich der Anzahl der Kreise an. Beim Bezahlen sind einsetzbare Kreise golden umrandet und anklickbar ([signal mana_clicked]).

### Funktionen

| Funktion | Rückgabe | Beschreibung |
|---|---|---|
| `set_amounts(amounts: PackedInt64Array)` | `void` | Setzt die Mana-Mengen je Art (Reihenfolge Weiß, Blau, Schwarz, Rot, Grün, Farblos). Leeres Array = nichts anzeigen. Zuwachs lässt den Kreis aufspringen. |

### Signale

| Signal | Beschreibung |
|---|---|
| `mana_clicked(mana: int)` | Ein anklickbares Mana-Symbol wurde angeklickt (`mana` = Manaart: 0 = Weiß, 1 = Blau, 2 = Schwarz, 3 = Rot, 4 = Grün, 5 = Farblos). |

## PhaseTimeline

*Erbt von `Control` · Quelle: `rust/src/game/phase_timeline.rs`*

Zeitstrahl der Phasen (HUD, unten rechts über dem Hauptbutton): ein mit Enttappen, Auffrischung, Ziehen und Hauptphase 1 wachsender Kreis, ein Punkt „Kampf“, der seine drei Schritte (Phasen-Index 4–6) erst während des Kampfs aufklappt, und ein Kreis für Hauptphase 2 und Ende, der im Endschritt kleiner wird. Die aktuelle Station leuchtet; erledigte sind golden, kommende hohl. Darüber steht der Name der aktuellen Phase. Wird vom `TurnHud` jedes Frame über `set_state()` gefüttert.

### Funktionen

| Funktion | Rückgabe | Beschreibung |
|---|---|---|
| `set_state(titles: PackedStringArray, current: int)` | `void` | Setzt Phasennamen (in Spielreihenfolge) und Index der aktuellen Phase (-1 = vor Spielbeginn). |
| `draw_pulse()` | `void` | Zeichnet die aktuelle Station mit pulsierendem Schein in die eigene Ebene (jedes Bild; gerufen vom Signal `draw` der Ebene). |

## PlayerSeat

*Erbt von `Node3D` · Quelle: `rust/src/game/seat.rs`*

Ein Sitzplatz am Tisch (Mensch oder KI) mit seinen Zonen, Lebenspunkten und dem Manapool. Die Tischszene enthält zwei: `Player` (unten) und `Opponent` (oben).

### Eigenschaften

| Name | Typ | Beschreibung |
|---|---|---|
| `display_name` | `String` | Anzeigename in HUD und Protokoll. |
| `is_ai` | `bool` | Wird dieser Platz von der KI gespielt? |
| `starting_life` | `int` | Lebenspunkte zu Spielbeginn. |
| `hand` | `Zone` | Zone der Handkarten. |
| `deck` | `Zone` | Zone des Nachziehstapels (Deck). |
| `battlefield` | `Zone` | Zone des Spielfelds (Kreaturen, Helden, Artefakte, Verzauberungen, Ausrüstung, Auren). |
| `discard` | `Zone` | Zone des Friedhofs (Ablagestapel). |
| `exile` | `Zone` | Zone des Exils (CR 406): offener Stapel neben dem Friedhof. |
| `resource_zone` | `Zone` | Reihe für ausgespielte Ressourcen-Karten. |
| `mana_pool` | `ManaPool` | Manapool dieses Spielers. |
| `life` | `int` | Aktuelle Lebenspunkte (≤ 0 = Partie verloren). |

### Funktionen

| Funktion | Rückgabe | Beschreibung |
|---|---|---|
| `owns_zone(zone: Zone)` | `bool` | Gehört die Zone zu diesem Sitzplatz? |

## SeatBadge

*Erbt von `Control` · Quelle: `rust/src/game/seat_badge.rs`*

Rundes Helden-Medaillon (HUD): Figur in der Seitenfarbe `accent` (oder `portrait`) im Metallrahmen, darunter eine Plakette mit den Lebenspunkten `life`. Ist `active` gesetzt (Spieler am Zug), dreht sich ein goldener Strahlenkranz dahinter; Lebensänderungen blitzen kurz rot (Schaden) bzw. grün (Heilung). Wird vom `TurnHud` jedes Frame über `set_state()` aktualisiert und mit der Unterkante (Spieler) bzw. Oberkante (Gegner) an den Anker `HeroAnchor` des Sitzplatzes gesetzt.

### Eigenschaften

| Name | Typ | Beschreibung |
|---|---|---|
| `seat_name` | `String` | Name der Seite (für Lebensänderungen: erst ab dem ersten Zustand blitzt die Plakette). |
| `life` | `int` | Angezeigte Lebenspunkte. |
| `active` | `bool` | Ist diese Seite am Zug? (goldener Strahlenkranz) |
| `accent` | `Color` | Farbe der Figur (Seitenfarbe, z. B. blau für den Spieler, rot für den Gegner). |
| `portrait` | `Texture2D` | Eigenes Porträt (rund zugeschnitten) statt der gezeichneten Figur; leer = Figur. |
| `targetable` | `bool` | Ist dieser Spieler gerade ein gültiges Ziel? Dann pulsiert ein goldener Ring und ein Klick löst [signal clicked] aus (sonst lässt das Medaillon Mausklicks durch). |
| `target_selected` | `bool` | Ist dieser Spieler als Ziel gewählt? (heller, ruhiger Ring) |

### Funktionen

| Funktion | Rückgabe | Beschreibung |
|---|---|---|
| `is_targetable()` | `bool` | Getter für `targetable`. |
| `set_targetable(value: bool)` | `void` | Setter für `targetable`: nur ein wählbares Medaillon fängt Mausklicks ab. |
| `set_state(seat_name: String, life: int, active: bool)` | `void` | Setzt Name, Lebenspunkte und „am Zug“ auf einmal. Weicht `life` vom bisherigen Wert ab, blitzt die Plakette kurz auf (rot bei Schaden, grün bei Heilung). |

### Signale

| Signal | Beschreibung |
|---|---|
| `clicked()` | Das Medaillon wurde angeklickt, während `targetable` gesetzt war. |

## Hello

*Erbt von `Node2D` · Quelle: `rust/src/hello.rs`*

Beispiel-Node: zeigt, dass die Rust-Extension geladen wird.

### Eigenschaften

| Name | Typ | Beschreibung |
|---|---|---|
| `greeting` | `String` | Text, der beim Start ausgegeben wird. |

### Funktionen

| Funktion | Rückgabe | Beschreibung |
|---|---|---|
| `add(a: int, b: int)` | `int` | Beispielfunktion: gibt `a + b` zurück. |

## Arena

*Erbt von `Node3D` · Quelle: `rust/src/table/arena.rs`*

Arena-Kulisse: Sandstein-Spielbrett mit gemeißeltem, leuchtendem Runenkreis in der Mitte, großer Raute und Mittellinie (ohne eingelassene Felder für Deck, Ablage und Ressourcen), flacher Steinrand mit Goldeinlage, Leuchtkristalle in den Ecken, gestufter Sockel, Schieferboden, Säulenruinen, Feuerschalen und Arenamauer mit Bannern. Alles wird beim Start prozedural erzeugt. Maßstab: 1 Einheit = 10 cm; Brettoberfläche bei y = 0, Mittellinie bei z = 0 (Spieler z > 0, Gegner z < 0). Tool-Klasse: Die Kulisse wird auch im Godot-Editor gebaut, damit die Szene dort sichtbar ist. Die erzeugten Kinder haben keinen Owner und werden daher nicht in die `.tscn` gespeichert. Nach Änderungen an den Eigenschaften im Inspektor baut sich die Kulisse neu auf.

### Eigenschaften

| Name | Typ | Beschreibung |
|---|---|---|
| `board_size` | `Vector2` | Größe des Spielbretts (x = Breite, y = Tiefe) in Einheiten. |
| `texture_resolution` | `float` | Auflösung der Brett-Textur in Pixeln pro Einheit (höher = schärfere Ornamente, längerer Start). |
| `rune_glow` | `float` | Leuchtstärke des Runenkreises (0 = aus). |
| `player_color` | `Color` | Bannerfarbe der Spielerseite. |
| `opponent_color` | `Color` | Bannerfarbe der Gegnerseite. |

### Funktionen

| Funktion | Rückgabe | Beschreibung |
|---|---|---|
| `rebuild()` | `void` | Baut die gesamte Kulisse neu auf (entfernt vorher alle erzeugten Kinder). Wird in `ready()` aufgerufen und im Editor automatisch nach Änderungen im Inspektor. |
| `set_board_size(value: Vector2)` | `void` | Setter für `board_size`; baut die Kulisse (verzögert) neu auf, wenn sie schon steht. |
| `set_texture_resolution(value: float)` | `void` | Setter für `texture_resolution`; baut die Kulisse (verzögert) neu auf, wenn sie schon steht. |
| `set_rune_glow(value: float)` | `void` | Setter für `rune_glow`; baut die Kulisse (verzögert) neu auf, wenn sie schon steht. |
| `set_player_color(value: Color)` | `void` | Setter für `player_color`; baut die Kulisse (verzögert) neu auf, wenn sie schon steht. |
| `set_opponent_color(value: Color)` | `void` | Setter für `opponent_color`; baut die Kulisse (verzögert) neu auf, wenn sie schon steht. |

## Button3D

*Erbt von `Area3D` · Quelle: `rust/src/table/button3d.rs`*

Anklickbarer Knopf in der 3D-Szene: bewegte Oberfläche und Leuchten im Farbstil `style` (Nachbau der Blender-Materialien) mit Beschriftung (`text`). Wird vom `TableController` per Raycast angeklickt (Kollisions-Layer 3) und meldet das mit [signal pressed]. Richtet sich auf Wunsch zur Kamera aus (`face_camera`).

### Eigenschaften

| Name | Typ | Beschreibung |
|---|---|---|
| `style` | `ButtonStyle` | Farbstil: 0 = Orange (Feuer), 1 = Blau (Wolken), 2 = Grau. Lässt sich jederzeit wechseln (z. B. „Kein Angriff“ blau → „Angriff abbrechen“ grau). |
| `text` | `String` | Beschriftung auf dem Knopf. |
| `width` | `float` | Breite der Bildfläche in Welteinheiten (Höhe folgt aus dem Seitenverhältnis). |
| `face_camera` | `bool` | Knopf immer parallel zur Bildebene der Kamera ausrichten. |
| `enabled` | `bool` | Anklickbar? (Ausgegraut und ohne Klickwirkung, wenn nicht.) |
| `body_half` | `float` | Halbe Länge des geraden Knopfkörpers in Blender-Einheiten: 1,795 = breite Pille wie in Blender, 0,12 = kurzer Knopf für ein, zwei Symbole. Höhe, Rundung und Leuchten bleiben. |

### Funktionen

| Funktion | Rückgabe | Beschreibung |
|---|---|---|
| `set_hovered(hovered: bool)` | `void` | Maus über dem Knopf (vom `TableController` gesetzt): Knopf wächst, Leuchten wird heller. |
| `is_hovered()` | `bool` | Ist die Maus über dem Knopf? |
| `activate()` | `void` | Klick auslösen (vom `TableController` bzw. von Skripten): kurzes Eindrücken und [signal pressed]. Ohne Wirkung, wenn der Knopf nicht `enabled` ist oder gerade verschwindet. |
| `dismiss()` | `void` | Knopf ausblenden (0,15 s) und danach freigeben; nicht mehr anklickbar. |
| `pin_to_screen(anchor: Vector2, offset: Vector2, width_px: float)` | `void` | Knopf an eine Bildschirmstelle heften (wie ein HUD-Element, aber in der 3D-Szene): Mitte = `anchor` × Fenstergröße + `offset` (Pixel, z. B. `anchor` (1, 1) = unten rechts), `width_px` = Breite der Bildfläche in Pixeln. Er steht dann jedes Frame kurz vor der Kamera, parallel zur Bildebene, vor allen Karten. |
| `set_style(style: ButtonStyle)` | `void` | Setter für `style`: Farben des Knopfes sofort umstellen (Oberfläche und Leuchten laufen ohne Neustart weiter). |
| `set_label_text(text: String)` | `void` | Beschriftung ändern (ohne Wirkung, wenn sie gleich bleibt). |

### Signale

| Signal | Beschreibung |
|---|---|
| `pressed()` | Der Knopf wurde angeklickt. |

## Card3D

*Erbt von `Area3D` · Quelle: `rust/src/table/card3d.rs`*

Eine Spielkarte auf dem 3D-Tisch: Mesh mit Vorder-/Rückseite und Beschriftung, Hover-Neigung und Hover-Zoom (offene Karten werden groß und lesbar), Raycast-Ziel (Kollisions-Layer 1), Gleit-Animationen per Tween sowie alle Spielwerte (Name, Typzeile, Kosten, Regeltext, Werte aus den Kartendaten der Engine; Tapp-Zustand).

### Eigenschaften

| Name | Typ | Beschreibung |
|---|---|---|
| `face_texture` | `Texture2D` | Textur der Vorderseite. |
| `back_texture` | `Texture2D` | Textur der Rückseite. |
| `max_tilt_degrees` | `float` | Maximale Neigung bei Mouseover (Grad). |
| `tilt_speed` | `float` | Glättung von Tilt/Zoom (größer = schneller). |
| `move_duration` | `float` | Dauer der Gleit-Animation zu einem Slot (Sekunden). |
| `zoom_enabled` | `bool` | Offene Karten beim Hovern selbst groß und lesbar vor die Kamera holen (setzt die Zone, `Zone.zoom_cards`: an für die eigene Hand; sonst zeigt der `CardInspector` eine Kopie mit Fähigkeiten und Buffs). |
| `zoom_screen_height` | `float` | Anteil der Bildschirmhöhe, den die gezoomte Karte einnimmt. |
| `zoom_distance` | `float` | Abstand der gezoomten Karte zur Kamera (liegt damit vor allem anderen). |
| `zoom_margin_px` | `float` | Rand zum Bildschirmrand in Pixeln. |
| `hover_lift` | `float` | Anheben (entlang der Kartennormalen) für verdeckte Karten / ohne Zoom. |
| `hover_scale` | `float` | Skalierung für verdeckte Karten / ohne Zoom. |
| `card_name` | `String` | Kartenname (wie von der Engine). |
| `card_kind` | `int (Kartentyp)` | Kartenart: 0 = Kreatur, 1 = Land, 2 = Spontanzauber, 3 = Hexerei, 4 = Planeswalker, 5 = Ausrüstung, 6 = Artefakt, 7 = Verzauberung, 8 = Aura, 9 = Schlacht. |
| `owner_seat` | `int` | Besitzer (0 = Spieler, 1 = Gegner): auf dessen Friedhof geht die Karte. |

### Funktionen

| Funktion | Rückgabe | Beschreibung |
|---|---|---|
| `set_face_texture(tex: Texture2D)` | `void` | Setzt die Textur der Vorderseite. |
| `set_back_texture(tex: Texture2D)` | `void` | Setzt die Textur der Rückseite. |
| `set_hovered(hovered: bool)` | `void` | Aktiviert/deaktiviert den Hover-Zustand. |
| `is_hovered()` | `bool` | Liegt der Mauszeiger gerade auf der Karte? |
| `is_zoomed()` | `bool` | Wird die Karte gerade vergrößert angezeigt? |
| `screen_quad()` | `PackedVector2Array` | Die vier Ecken der Karte, wie sie gerade auf dem Bildschirm erscheint (inklusive Hover-Zoom, Neigung und Drag-Schwung), in Pixeln. Leer, wenn die Karte hinter der Kamera liegt. |
| `update_hover_point(world_point: Vector3)` | `void` | Übergibt den Raycast-Trefferpunkt (Weltkoordinaten) für den Tilt-Effekt. |
| `set_dragging(dragging: bool)` | `void` | Markiert die Karte als gezogen (Hover aus, laufende Animation stoppt). Setzt der `TableController`. |
| `is_dragging()` | `bool` | Wird die Karte gerade gezogen? |
| `set_drag_sway(sway: Vector2)` | `void` | Neigung während des Ziehens (x = Rollen um Z, y = Nicken um X), Radiant. |
| `move_to(target: Transform3D, delay: float)` | `void` | Lässt die Karte per Tween sanft zu `target` (globale Transform) gleiten. |
| `snap_to(target: Transform3D)` | `void` | Setzt die Karte sofort ohne Animation. |
| `stop_motion()` | `void` | Bricht eine laufende Gleit-Animation ab. |
| `set_ready_glow(on: bool)` | `void` | Dezenter goldener Rand „diese Karte kann eine Fähigkeit aktivieren“ an/aus (für Skripte; Rot/Blau setzt das Spiel selbst). Eine Markierung hat Vorrang. |
| `flash()` | `void` | Rahmen leuchtet kurz hell und orange auf (etwas wurde aktiviert: gespielt, Fähigkeit, Mana-Einschlag, Angriff …), danach zurück zum normalen Leuchten. |
| `set_attacking(attacking: bool)` | `void` | Angreifer im Kampf: Karte rückt etwas zum Gegner vor und trägt die rote Angriffsspitze an der Oberkante (`false` = zurück an ihren Platz). |
| `set_block_trail(on: bool)` | `void` | Blocker im Kampf (Video „Blocken“): über der Kante zum Angreifer steigt ein flackernder Schweif als Bogen auf – orange, solange der Block geplant ist, gelb, sobald er erklärt ist (`set_block_declared()`). Blendet in ≈ 0,25 s ein und aus. |
| `set_block_declared(on: bool)` | `void` | In einem erklärten Block (Angreifer und Blocker, CR 509.1): Rahmen, Angriffsspitze und Blockschweif werden gelb ([`BLOCKED_GLOW`]), bis der Kampf endet (`false`). |
| `set_turned(turned: bool)` | `void` | Kartenbild um 180° gedreht zeigen (setzt die Zone, `Zone.turn_cards`): Karten des Gegners liegen in seiner um 180° gedrehten Zone und wären sonst von unserer Seite aus kopfüber. Nur das Bild dreht sich; Vorrücken und Schlag im Kampf bleiben zum Gegner gerichtet. |
| `set_combat_alone(alone: bool)` | `bool` | Im Kampf geblockt bzw. blockend (`true`): die Karte bildet auf dem Spielfeld einen eigenen Stapel, statt mit gleichen Karten zusammenzuliegen. Gibt zurück, ob sich etwas geändert hat. |
| `set_combat_x(x: float, on: bool)` | `bool` | Geblockter Angreifer: liegt seinem Blocker auf dem Spielfeld genau gegenüber (`x` = Welt-x des Blockers bzw. die Mitte mehrerer Blocker; `on = false` = wieder normal einreihen). Gibt zurück, ob sich die Lage spürbar geändert hat (dann muss die Zone neu anordnen). |
| `set_tile(tile: bool)` | `void` | Kachel-Form (Spielfeld): nur Name und Bild, Stärke/Widerstand unten rechts; beim Zoomen und Ziehen wird die ganze Karte gezeigt. Setzt die Zone beim Anordnen. |
| `set_bright(bright: bool)` | `void` | Immer hell in Originalfarben zeigen (Karten auf einer 3D-Bühne vor der Kamera). |
| `set_zoom(zoom: bool)` | `void` | Beim Hovern selbst groß vor die Kamera holen (Hand) – setzt die Zone beim Anordnen. |
| `is_tile()` | `bool` | Wird die Karte als Kachel gezeigt? |
| `is_attacking()` | `bool` | Ist die Karte als Angreifer markiert (vorgerückt, mit Angriffsspitze)? |
| `lunge(target: Vector3, reach: float, delay: float)` | `void` | Schlag im Kampf: die Karte holt zum Weltpunkt `target` aus (Anteil `reach` des Wegs, angehoben), trifft nach [`LUNGE_OUT`] Sekunden und kehrt zurück; Start nach `delay`. |
| `hold(seconds: float)` | `void` | Die Karte bleibt `seconds` lang an ihrem Platz: Bewegungen (`move_to()`) beginnen erst danach (getroffene Kreatur im Kampf, damit der Schlag sichtbar bleibt). |
| `set_marker_kind(kind: int)` | `void` | Aura um die Karte setzen: 0 = keine, 1 = Angreifer (rot), 2 = Blocker (blau), 3 = ausgewählt/wählt ein Ziel (gold). |
| `set_tapped_state(tapped: bool)` | `void` | Setzt nur den Tapp-Zustand der Karte (schräg, ausgegraut, Tapp-Symbol), ohne Spielregeln (kein Mana, kein Neu-Anordnen) – für Skripte und Tests. |
| `is_tapped()` | `bool` | Ist die Karte getappt (leicht schräg, ausgegraut, Tapp-Symbol)? |
| `get_zone()` | `Zone` | Die Zone, in der die Karte aktuell liegt (None während des Ziehens). |

## CardInspector

*Erbt von `Node3D` · Quelle: `rust/src/table/card_inspector.rs`*

Kartenansicht beim Hovern wie `Fähigkeiten.png`: helle Kopie der Karte am Bildschirmrand, daneben eine dunkle Tafel mit Fähigkeiten (Symbol, Name fett, Erklärung kursiv) und Veränderungen. `hide_card()` blendet sie aus.

### Funktionen

| Funktion | Rückgabe | Beschreibung |
|---|---|---|
| `hide_card()` | `void` | Ansicht ausblenden. |
| `is_showing()` | `bool` | Wird gerade eine Karte gezeigt? |

## CostSymbolCanvas

*Erbt von `Control` · Quelle: `rust/src/table/cost_symbols.rs`*

Zeichenfläche eines einzelnen Kostensymbols (intern: liegt in einer unsichtbaren `SubViewport`, deren Textur die 3D-Kostensymbole nutzen).

## DungeonCanvas

*Erbt von `Control` · Quelle: `rust/src/table/dungeon_view.rs`*

Zeichenfläche einer Dungeon-Karte (intern: liegt in einer unsichtbaren `SubViewport`).

## DungeonView

*Erbt von `Node3D` · Quelle: `rust/src/table/dungeon_view.rs`*

Dungeon-Ansicht wie in Arena: Dungeon(s) groß vor dem abgedunkelten Tisch; je nach Modus Dungeon oder Raum per Klick wählen (Signal [signal picked]) bzw. ansehen (Klick → [signal dismissed]).

### Funktionen

| Funktion | Rückgabe | Beschreibung |
|---|---|---|
| `update_pointer(world: Vector3)` | `void` | Trefferpunkt des Mausstrahls (Welt): Dungeon bzw. wählbarer Raum darunter wird hervorgehoben. |
| `set_hovered(hovered: bool)` | `void` | Maus verlässt die Ansicht. |
| `activate()` | `void` | Klick: wählt je nach Modus; in der reinen Ansicht schließt jeder Klick. |
| `close()` | `void` | Schließen (0,15 s ausblenden), danach freigeben. |

### Signale

| Signal | Beschreibung |
|---|---|
| `picked(index: int)` | Gewählt: Dungeon-Index (Dungeon wählen) bzw. Nummer der Option des Raums (Raum wählen). |
| `dismissed()` | Klick in der reinen Ansicht: schließen. |

## FramePacer

*Erbt von `Node` · Quelle: `rust/src/table/frame_pacer.rs`*

Begrenzt die Bildrate (`Engine.max_fps`) je nach Lage: volle Rate, solange sich etwas tut (Eingaben, Nachrichten der Engine), gedrosselt in Ruhe und stark gedrosselt im minimierten Fenster. Spart Strom und Abwärme, ohne dass Bewegungen ruckeln: Jede Eingabe schaltet sofort auf die volle Rate zurück.

### Eigenschaften

| Name | Typ | Beschreibung |
|---|---|---|
| `active_fps` | `int` | Bildrate, solange sich etwas tut (0 = unbegrenzt). Liegt die Bildwiederholrate des Monitors (mit V-Sync) nicht höher, begrenzt V-Sync allein. |
| `idle_fps` | `int` | Bildrate in Ruhe: keine Eingabe und keine Nachricht der Engine seit `hold_seconds`. |
| `minimized_fps` | `int` | Bildrate, solange das Fenster minimiert ist. |
| `hold_seconds` | `float` | Sekunden nach der letzten Aktivität, bis auf `idle_fps` gedrosselt wird (lang genug für die Animationen, die eine Aktivität auslöst). |

## ManaFx

*Erbt von `Node3D` · Quelle: `rust/src/table/fx.rs`*

Mana-Effekt (Land → Karte) bzw. nur Einschlag. Gibt sich nach Frame 80 selbst frei.

### Funktionen

| Funktion | Rückgabe | Beschreibung |
|---|---|---|
| `set_target(target: Node3D)` | `void` | Ziel nachträglich setzen (z. B. sobald die Karte auf dem Stack liegt). |
| `has_target()` | `bool` | Hat der Effekt schon ein Ziel? |

## LoyaltyView

*Erbt von `Node3D` · Quelle: `rust/src/table/loyalty_view.rs`*

Planeswalker-Ansicht wie in Arena: die Karte groß, ihre Loyalitätsfähigkeiten als Felder auf der Karte (nicht aktivierbare ausgegraut). Signale [signal picked] (Feld angeklickt) und [signal dismissed] (Klick neben die Karte).

### Funktionen

| Funktion | Rückgabe | Beschreibung |
|---|---|---|
| `update_pointer(world: Vector3)` | `void` | Trefferpunkt des Mausstrahls (Welt): das Feld darunter wird hervorgehoben; merkt sich, ob die Maus auf der Karte liegt. |
| `set_hovered(hovered: bool)` | `void` | Maus verlässt die Ansicht bzw. betritt sie (ohne Trefferpunkt). |
| `activate()` | `void` | Klick: auf ein aktivierbares Feld → [signal picked]; neben die Karte → [signal dismissed]. |
| `close()` | `void` | Ansicht schließen (0,15 s ausblenden), danach freigeben; nicht mehr bedienbar. |
| `belongs_to(card: Card3D)` | `bool` | Gehört die Ansicht zur Karte `card`? |

### Signale

| Signal | Beschreibung |
|---|---|
| `picked(index: int)` | Ein aktivierbares Feld wurde angeklickt: `index` = Nummer der Fähigkeit (Reihenfolge wie übergeben). |
| `dismissed()` | Klick neben die Karte: die Ansicht soll schließen. |

## ManaWheel

*Erbt von `Node3D` · Quelle: `rust/src/table/mana_wheel.rs`*

Farbrad vor der Kamera an einer Bildschirmstelle: Segmente (Farbe + Manasymbol), Kartenbild in der Mitte, Hinweis darüber. Signal [signal picked] beim Klick auf ein Segment.

### Funktionen

| Funktion | Rückgabe | Beschreibung |
|---|---|---|
| `update_pointer(world: Vector3)` | `void` | Trefferpunkt des Mausstrahls (Welt) auf dem Rad: das Segment darunter wird hervorgehoben (außerhalb des Rings bzw. über der Mittelscheibe keins). |
| `set_hovered(hovered: bool)` | `void` | Maus verlässt das Rad bzw. betritt es (ohne Trefferpunkt). |
| `activate()` | `void` | Klick: liegt die Maus auf einem Segment, wird [signal picked] mit seiner Nummer gesendet. |
| `close()` | `void` | Rad schließen (0,15 s ausblenden und schrumpfen), danach freigeben; nicht mehr bedienbar. |

### Signale

| Signal | Beschreibung |
|---|---|
| `picked(index: int)` | Ein Segment wurde angeklickt: `index` = Nummer des Segments (Reihenfolge wie übergeben). |

## SearchList

*Erbt von `Node3D` · Quelle: `rust/src/table/search_list.rs`*

Auswahlliste mit Filterfeld: zeigt `set_entries()`-Einträge in sieben Zeilen, scrollt mit dem Mausrad bzw. Scrollbalken. Signale [signal selected] (Zeile angeklickt) und [signal confirmed] (zweiter Klick auf die gewählte Zeile).

### Funktionen

| Funktion | Rückgabe | Beschreibung |
|---|---|---|
| `selected_index()` | `int` | Gewählter Eintrag (Nummer in der Liste von `set_entries()`), -1 = keiner. |
| `hovered_index()` | `int` | Eintrag unter der Maus, -1 = keiner. |
| `move_selection(step: int)` | `void` | Auswahl um `step` Zeilen verschieben (Pfeiltasten); ohne Auswahl die erste Zeile. |
| `update_pointer(world: Vector3)` | `void` | Mauspunkt auf der Trefferfläche (Weltkoordinaten): Zeile bzw. Scrollbalken darunter. |
| `set_hovered(hovered: bool)` | `void` | Maus verlässt die Liste. |
| `activate()` | `void` | Klick: Zeile wählen (zweimal = bestätigen) bzw. im Scrollbalken dorthin springen. |

### Signale

| Signal | Beschreibung |
|---|---|
| `selected(index: int)` | Eintrag `index` (Nummer in der Liste von `set_entries()`) wurde angeklickt. |
| `confirmed(index: int)` | Die gewählte Zeile wurde noch einmal angeklickt (Doppelklick): Auswahl bestätigen. |

## Showcase

*Erbt von `Node3D` · Quelle: `rust/src/table/showcase.rs`*

Bühne vor der Kamera: Abdunkelung, Kartenfächer (`Zone`), Titel und `Button3D`-Knöpfe. `open()` stellt sie vor die aktuelle Kamera, `show_cards()` holt Karten herein, `close()` gibt sie an ihre Zonen zurück und blendet aus.

### Funktionen

| Funktion | Rückgabe | Beschreibung |
|---|---|---|
| `open_at(point: Vector3, scale: float, lift: float)` | `void` | Die Bühne an einer Stelle auf dem Tisch statt vor der Kamera (z. B. am Stack), parallel zur Bildebene, um `scale` vergrößert, ohne Abdunkelung: `point` = Weltpunkt der Bühnenmitte, die Bühne rückt `lift` zur Kamera hin (vor die Karten dort). |
| `open()` | `void` | Stellt die Bühne vor die aktuelle Kamera (Abstand fest, parallel zur Bildebene) und zeigt oben rechts „Schlachtfeld ansehen“ (`enable_peek()`). |
| `set_texts(title: String, subtitle: String, note: String)` | `void` | Titel, Untertitel und Hinweis setzen (leer = ausblenden). Wie in Arena bleibt der Titel kurz: Ist er breiter als der Bildschirm ([`TITLE_MAX_WIDTH`]; in der Fokus-Anordnung [`FOCUS_TITLE_MAX_WIDTH`], dort schrumpft er zuerst bis [`TITLE_MIN_FONT`]), wird er geteilt ([`split_title`]) – der Rest kommt vor den Untertitel. Der Untertitel bricht in Zeilen um und wird bei langem Text kleiner. |
| `set_focus_layout()` | `void` | Fokus-Anordnung für Fragen zu einer Karte (Kicker, Ja/Nein …): die Karte groß und hell in der Mitte, die Knöpfe untereinander darunter, der Titel direkt über der Karte, kein Untertitel (er läge auf der Karte). Vor `add_button()` aufrufen. |
| `set_focus_layout_sized(scale: float, center_y: float)` | `void` | Fokus-Anordnung mit eigener Kartengröße `scale` (Anteil der Kartengröße) und Höhe der Kartenmitte `center_y` (Bühnen-Einheiten), z. B. größer für die X-Bühne. |
| `set_title_card(card: Card3D)` | `void` | Karte am Stack, zu der die Frage gehört (der gewirkte Zauber): Der Untertitel der Fokus-Anordnung steht dann immer mittig als Überschrift über dieser Karte statt rechts neben der großen Karte (dort läge er auf dem Stack) und folgt ihr jedes Bild. Nach `set_focus_layout()` aufrufen. |
| `static focus_button_width()` | `float` | Breite eines Knopfes in der Fokus-Anordnung (Welteinheiten). |
| `enable_scroll()` | `void` | Fächer nebeneinander statt überlappend legen; passt er nicht ganz ins Bild, erscheint unter ihm ein `Slider3D`, mit dem man ihn nach links und rechts schiebt (Start: Mitte). |
| `enable_line()` | `void` | Wie `enable_scroll()`, aber die Karten liegen auf einer geraden Linie (ohne Bogen und ohne Drehung), z. B. beim Abwerfen. |
| `enable_scroll_with(step: float, radius: float)` | `void` | Wie `enable_scroll()`, mit eigenem Kartenabstand `step` (kleiner als eine Kartenbreite = überlappend) und Bogenradius `radius` (klein = stärker gebogen). |
| `on_scroll(value: float)` | `void` | Schieberegler bewegt (0 = linkes Ende sichtbar, 1 = rechtes Ende). |
| `enable_coverflow(step: float, scale: float)` | `void` | Fächer als Cover-Flow zum Durchsuchen vieler Karten (Bibliothek, Friedhof …), nach `show_options()` bzw. `add_owned_cards()` aufrufen: Karten in Größe `scale` im Abstand `step` überlappend nebeneinander, die mittlere vorn und gerade, die übrigen zur Seite gedreht und nach hinten versetzt, am Rand dicht gestapelt (`Zone.coverflow`). Die Karte unter der Maus kommt groß und gerade nach vorn (Hover-Zoom). Bei vielen Karten erscheint darunter ein `Slider3D`; Schieben lässt die Karten durch die Mitte laufen (Start: Mitte). |
| `enable_peek()` | `void` | Knopf „Schlachtfeld ansehen“ oben rechts: blendet die Bühne samt Abdunkelung und den Karten darauf aus (auch ausgeliehene wie die Starthand; der Tisch ist dann sichtbar und hoverbar), ein erneuter Druck („Zurück zur Auswahl“) holt sie zurück. Die Entscheidung bleibt dabei offen. |
| `on_peek()` | `void` | „Schlachtfeld ansehen“ gedrückt: Bühne aus- bzw. wieder einblenden. |
| `set_subtitle(subtitle: String)` | `void` | Nur den Untertitel ersetzen (Titel und Hinweis bleiben), z. B. den Kartennamen als Überschrift über der Karte am Stack. |
| `shift_fan(delta: Vector3)` | `void` | Hauptfächer samt Beschriftungen um `delta` (Bühnen-Einheiten) verschieben, z. B. tiefer oder nach links neben eine Karte am Stack; Schieben mit dem Regler behält das bei. |
| `static button_width()` | `float` | Breite eines Knopfes in dieser Bühne (Welteinheiten). |
| `is_covering()` | `bool` | Deckt die Bühne gerade den Tisch ab (abgedunkelt, nicht über „Schlachtfeld ansehen“ ausgeblendet, nicht am Schließen)? Dann blenden HUD und Zielpfeile sich aus. |
| `holds(card: Card3D)` | `bool` | Liegt `card` gerade in der Bühne? |
| `close()` | `void` | Karten zurückgeben, Knöpfe ausblenden, abdunkeln aufheben und danach freigeben. |

## Slider3D

*Erbt von `Area3D` · Quelle: `rust/src/table/slider3d.rs`*

Waagerechter Schieberegler in der 3D-Szene: Schiene der Breite `width` mit orangem Griff. `value` 0 … 1 (links … rechts); jede Änderung meldet [signal value_changed]. Bedient wird er per Maus über den `TableController` (Ziehen am Griff oder Klick auf die Schiene).

### Eigenschaften

| Name | Typ | Beschreibung |
|---|---|---|
| `width` | `float` | Länge der Schiene (Welteinheiten). |
| `value` | `float` | Stellung 0 (links) … 1 (rechts). |

### Funktionen

| Funktion | Rückgabe | Beschreibung |
|---|---|---|
| `set_value(value: float)` | `void` | Stellung setzen (0 … 1, wird begrenzt) und melden. |
| `set_hovered(hovered: bool)` | `void` | Maus über dem Regler (vom `TableController`): Saum und Griff heller. |
| `set_dragging(dragging: bool)` | `void` | Wird gerade gezogen (vom `TableController`): Griff leuchtet stärker. |
| `drag_to(point: Vector3)` | `void` | Griff zu dem Weltpunkt `point` schieben (Lage entlang der Schiene). |

### Signale

| Signal | Beschreibung |
|---|---|
| `value_changed(value: float)` | Die Stellung hat sich geändert (0 … 1). |

## TableCamera

*Erbt von `Camera3D` · Quelle: `rust/src/table/table_camera.rs`*

Tischkamera: Übersicht über den Tisch, Nahaufnahme einer Karte (Rechtsklick) und leichtes Mitführen beim Ziehen einer Karte. Alle Übergänge sind weich.

### Eigenschaften

| Name | Typ | Beschreibung |
|---|---|---|
| `overview_position` | `Vector3` | Kameraposition in der Übersicht. |
| `overview_target` | `Vector3` | Blickpunkt in der Übersicht. |
| `focus_distance` | `float` | Abstand zur Karte in der Nahaufnahme. |
| `follow_strength` | `float` | Wie stark die Kamera einer gezogenen Karte folgt (0 = gar nicht, 1 = voll). |
| `follow_distance` | `float` | Abstand zur gezogenen Karte, wenn follow_strength = 1. |
| `smoothing` | `float` | Glättung der Kamerabewegung (größer = schneller). |

### Funktionen

| Funktion | Rückgabe | Beschreibung |
|---|---|---|
| `show_overview()` | `void` | Zurück zur Tischübersicht. |
| `focus_on(node: Node3D)` | `void` | Nahaufnahme einer Karte (oder eines beliebigen Node3D). |
| `follow(node: Node3D)` | `void` | Leichtes Mitführen (z. B. während eine Karte gezogen wird). |
| `is_focused_on(node: Node3D)` | `bool` | Ist die Kamera gerade auf genau diesen Node fokussiert? |
| `is_overview()` | `bool` | Ist die Kamera gerade in der Übersicht? |

## TableController

*Erbt von `Node3D` · Quelle: `rust/src/table/table_controller.rs`*

Wurzel der Tischszene: Maus-Eingabe (Hover, Drag-and-Drop per Raycast) und Kamera-Fokus (Rechtsklick). Kennt keine Regeln: Jeder Drop wird per [signal card_dropped] gemeldet; der `ForgeTable` spielt die Karte dann über Forge oder legt sie an ihren Platz zurück.

### Eigenschaften

| Name | Typ | Beschreibung |
|---|---|---|
| `camera` | `TableCamera` | Die Tischkamera. |
| `drag_height` | `float` | Höhe der Drag-Ebene über dem Tisch (Tischoberfläche = y dieses Nodes). |
| `drag_follow_speed` | `float` | Wie schnell die gezogene Karte der Maus folgt. |
| `ray_length` | `float` | Länge der Maus-Raycasts in Einheiten. |

### Funktionen

| Funktion | Rückgabe | Beschreibung |
|---|---|---|
| `zone_under_mouse()` | `Zone` | Zone unter dem Mauszeiger (Drop-Fläche auf dem Zonen-Layer; `null` = keine), z. B. um zu prüfen, ob ein Pfeil über der eigenen Hand losgelassen wurde. |
| `static move_top_card(from: Zone, to: Zone, delay: float)` | `Card3D` | Bewegt die oberste Karte von `from` nach `to` (z. B. Gegner zieht / spielt aus). |
| `set_language(code: String)` | `void` | Stellt die Sprache des Spiels um: `"de"` = Deutsch, `"en"` = Englisch. HUD, Hinweise und neue Protokolleinträge wechseln sofort; alle Karten werden neu beschriftet. Bereits geschriebene Protokolleinträge bleiben unverändert. |
| `get_language()` | `String` | Aktuelle Sprache: `"de"` oder `"en"`. |
| `get_hovered_card()` | `Card3D` | Die Karte, die gerade unter dem Mauszeiger liegt. |
| `get_aiming_card()` | `Card3D` | Karte, von der gerade ein Pfeil gezogen wird (null = keine). |
| `get_aim_point()` | `Vector3` | Mauspunkt auf der Tischebene (Welt), jedes Physik-Frame aktualisiert – Endpunkt gezogener Pfeile. |
| `get_dragged_card()` | `Card3D` | Die Karte, die gerade gezogen wird. |

### Signale

| Signal | Beschreibung |
|---|---|
| `card_dropped(card: Card3D, from_zone: Zone, to_zone: Zone)` | Wird ausgelöst, wenn eine Karte per Drag-and-Drop in eine Zone gelegt wurde. |
| `zone_clicked(zone: Zone)` | Eine anklickbare Zonen-Fläche wurde angeklickt (z. B. der Exil-Strudel): `zone` = die Zone. |
| `cancel_pressed()` | Escape gedrückt: die Kamera geht in die Übersicht; das Spiel bricht ein laufendes Wirken bzw. Aktivieren ab, wo die Engine das erlaubt. |
| `card_clicked(card: Card3D)` | Eine Karte wurde angeklickt (gedrückt und ohne nennenswerte Bewegung losgelassen) – z. B. als Ziel wählen oder Fähigkeit aktivieren. Funktioniert in allen Zonen, auch bei Karten, die man nicht ziehen kann. |
| `aim_started(card: Card3D)` | Von einer nicht verschiebbaren Karte (z. B. Kreatur auf dem Spielfeld) wird gezogen: der Spieler zieht einen Pfeil. Wohin er zeigt, liefert `get_aim_point()`; worauf die Maus liegt, `get_hovered_card()`. |
| `aim_released(card: Card3D, target: Card3D)` | Pfeil losgelassen: `target` = Karte unter der Maus (null = keine). Was das bedeutet (Angriff, Block, Ziel), entscheidet der `ForgeTable`. |

## TargetArrows

*Erbt von `Node3D` · Quelle: `rust/src/table/target_arrows.rs`*

Zeichnet die leuchtenden Pfeile aus `Assets/Pfeil/MTG_Pfeil.blend` auf dem Tisch (Angriff, Block, Zielwahl, Ziele auf dem Stack). Wird vom `ForgeTable` erzeugt und jedes Frame mit den aktuellen Pfeilen gefüttert; Höhe des Tisches = y dieses Nodes. Neue Pfeile wachsen in 0,33 s vom Start zum Ziel.

### Funktionen

| Funktion | Rückgabe | Beschreibung |
|---|---|---|
| `set_arrow_pairs(points: PackedVector3Array, colors: PackedInt32Array)` | `void` | Setzt Pfeile aus Skripten: `points` enthält je zwei Punkte (Start, Ziel; Welt) pro Pfeil, `colors` einen Farbindex pro Pfeil (0 Gold, 1 Blau, 2 Schwarz, 3 Rot, 4 Grün, 5 Grau, 6 Orange, 7 Gelb; fehlt er: Gold). Der `ForgeTable` überschreibt die Pfeile jedes Frame, solange er läuft. |

## FlickerLight

*Erbt von `OmniLight3D` · Quelle: `rust/src/table/tavern.rs`*

Punktlicht, das wie eine Kerzen-/Kaminflamme flackert. Tool-Klasse: flackert auch im Editor (wird von `Arena` und `Tavern` dort mit erzeugt).

### Eigenschaften

| Name | Typ | Beschreibung |
|---|---|---|
| `base_energy` | `float` | Grundhelligkeit des Lichts. |
| `flicker_strength` | `float` | Relative Stärke des Flackerns (0 = ruhig). |
| `flicker_speed` | `float` | Geschwindigkeit des Flackerns (1 = normal). |

## Tavern

*Erbt von `Node3D` · Quelle: `rust/src/table/tavern.rs`*

Tavernen-Kulisse: Holztisch mit Messing-Trennlinie, Kerzen, Krüge, Münzen sowie Gastraum mit Kamin, Fachwerk, Regalen, Fässern, Bank und Hockern – alles prozedural erzeugt. Maßstab: 1 Einheit = 10 cm; Tischoberfläche bei y = 0, Trennlinie bei z = 0. Tool-Klasse: Die Kulisse wird auch im Godot-Editor gebaut (Kinder ohne Owner, werden nicht in die `.tscn` gespeichert).

### Eigenschaften

| Name | Typ | Beschreibung |
|---|---|---|
| `table_size` | `Vector2` | Größe der Tischplatte (x = Breite, y = Tiefe). |
| `table_height` | `float` | Höhe der Tischplatte über dem Boden. |

## Zone

*Erbt von `Area3D` · Quelle: `rust/src/table/zone.rs`*

Ein Bereich auf dem Tisch, der Karten hält und anordnet: Hand, Deck, Friedhof, Spielfeld, Ressourcen-Reihe oder Stack. Karten gleiten per Tween an ihre Plätze; angelegte Karten (Ausrüstung, Auren) liegen versetzt unter ihrem Träger. Die Zone ist eine Area3D auf Kollisions-Layer 2 (Drop-Ziel für Raycasts).

### Eigenschaften

| Name | Typ | Beschreibung |
|---|---|---|
| `kind` | `int (Zonenart)` | Art der Anordnung: 0 = Hand, 1 = Stapel, 2 = Spielfeld, 3 = Ressourcen-Reihe, 4 = Stack. |
| `size` | `Vector3` | Ausdehnung der Drop-Fläche (x = Breite, y = Höhe der Kollisionsbox, z = Tiefe). |
| `accepts_drops` | `bool` | Dürfen Karten hier abgelegt werden? |
| `cards_draggable` | `bool` | Dürfen Karten aus dieser Zone gegriffen werden? |
| `click_to_draw` | `bool` | Klick auf eine Karte zieht die oberste Karte auf die Hand (Deck). |
| `face_up` | `bool` | Karten offen (true) oder verdeckt (false) anzeigen. |
| `show_area` | `bool` | Zonenfläche sichtbar markieren und beim Drüberziehen hervorheben. |
| `show_costs` | `bool` | Karten zeigen ihre Manakosten als Symbolreihe über der Oberkante (eigene Hand, wie im Video „Spree und Stack“; `table::cost_symbols`). |
| `hand_radius` | `float` | Hand: Radius des Fächerbogens. |
| `hand_card_angle_degrees` | `float` | Hand: Winkel zwischen zwei Karten (Grad). |
| `hand_max_spread_degrees` | `float` | Hand: maximaler Gesamtwinkel des Fächers (Grad). |
| `tail_offset` | `Vector3` | Hand: Verschiebung der Endgruppe (`add_tail_card()`, lokal) – z. B. damit der Commander in der fast verdeckten Hand des Gegners ganz zu sehen ist. |
| `coverflow` | `bool` | Hand als Cover-Flow (Kartenwahl-Bühne): Karten im Abstand `coverflow_step` nebeneinander, die mittlere vorn und gerade, die übrigen zur Seite gedreht, nach hinten versetzt und überlappend; am Rand dicht gestapelt. Ersetzt den Fächerbogen. |
| `coverflow_step` | `float` | Cover-Flow: Abstand der Karten (Zonen-Einheiten, kleiner als die Kartenbreite = überlappend). |
| `coverflow_scroll` | `float` | Cover-Flow: Verschiebung aller Karten nach rechts (Zonen-Einheiten); was dadurch in die Mitte rückt, dreht sich nach vorn (Schieberegler der Bühne). |
| `pile_spacing` | `float` | Stapel: Höhe je Karte. |
| `grid_columns` | `int` | Spielfeld im Rastermodus: Anzahl Spalten. |
| `grid_spacing` | `Vector2` | Spielfeld im Rastermodus: Abstand der Karten (x, z). |
| `free_placement` | `bool` | Karten bleiben dort liegen, wo sie abgelegt werden (statt Raster). |
| `auto_arrange` | `bool` | Automatische Anordnung nach Kartenart (hat Vorrang vor Raster und freier Ablage), aus Sicht der Seite (lokal −z = zur Tischmitte = „oben“): Kreaturen vorn mittig an der Mittellinie, alle anderen bleibenden Karten hinten rechts neben dem Medaillon. Länder liegen in einer eigenen Ressourcen-Zone (hinten links). |
| `mirror_x` | `bool` | Automatische Anordnung seitenverkehrt (lokal x gespiegelt): für die um 180° gedrehte Zone des Gegners, damit seine Artefakte von uns aus wie unsere rechts und Planeswalker links liegen. |
| `card_scale` | `float` | Größe der Karten in dieser Zone (1 = normal). Spielfeld und Ressourcen verkleinern sie automatisch, wenn sonst nicht alle in eine Reihe passen. |
| `tile_cards` | `bool` | Karten als Kachel zeigen (nur Name und Bild, Stärke/Widerstand unten rechts; siehe `Card3D.set_tile()`) – Form der Karten auf dem Spielfeld. |
| `turn_cards` | `bool` | Kartenbilder um 180° gedreht zeigen (`Card3D.set_turned()`): für die um 180° gedrehten Zonen des Gegners, damit seine Karten von unserer Seite aus lesbar sind. |
| `zoom_cards` | `bool` | Karten dieser Zone beim Hovern selbst groß vor die Kamera holen (eigene Hand); sonst zeigt die Kartenansicht eine Kopie. |
| `swallow_cards` | `bool` | Karten verschwinden in der Zone (Exil-Strudel): sie gleiten zur Mitte, schrumpfen dabei auf fast nichts und sinken unter den Tisch – unsichtbar und nicht anklickbar. Verlassen sie die Zone, wachsen sie auf dem Weg zum neuen Platz wieder. |

### Funktionen

| Funktion | Rückgabe | Beschreibung |
|---|---|---|
| `set_attached(card: Card3D, host: Card3D)` | `void` | Legt `card` an `host` an (Aura, Ausrüstung, Befestigung; CR 301.5, 303.4): Sie liegt dann versetzt unter dem Träger und folgt ihm beim Anordnen. `host` = null löst die Verbindung. Beide Karten müssen in dieser Zone liegen; ordnet nur bei Änderung neu an. |
| `add_card(card: Card3D, index: int, delay: float)` | `void` | Fügt eine Karte ein (index < 0 = ans Ende) und ordnet alle Karten neu an. `delay` verzögert nur die Animation der neuen Karte (z. B. zum Austeilen). |
| `add_tail_card(card: Card3D)` | `void` | Handfächer: Karte ans rechte Ende in die eigene Gruppe mit Lücke davor legen (z. B. Karten, die aus einer anderen Zone spielbar sind); neue Handkarten kommen immer davor. |
| `add_card_at(card: Card3D, world_point: Vector3, delay: float)` | `void` | Legt eine Karte frei am Weltpunkt ab (nur mit `free_placement`, sonst wie `add_card` ans Ende). Die Position wird in der Zone begrenzt. |
| `is_free_placement()` | `bool` | Bleiben Karten dort liegen, wo sie abgelegt werden (statt Raster)? |
| `is_hand()` | `bool` | Ist das ein Handfächer (eigene Hand, Gegnerhand, Fächer einer 3D-Bühne)? |
| `remove_card(card: Card3D)` | `int` | Entfernt eine Karte; gibt ihren bisherigen Index zurück (-1 falls nicht enthalten). |
| `layout()` | `void` | Ordnet alle Karten neu an (Tween zu ihren Slots). |
| `get_card_count()` | `int` | Anzahl der Karten in der Zone. |
| `get_top_card()` | `Card3D` | Oberste/letzte Karte der Zone. |
| `get_cards()` | `Array[Card3D]` | Alle Karten der Zone, unterste bzw. erste zuerst. |
| `accepts_drops()` | `bool` | Dürfen hier Karten abgelegt werden? |
| `cards_draggable()` | `bool` | Dürfen Karten aus dieser Zone gegriffen werden? |
| `click_to_draw()` | `bool` | Zieht ein Klick auf eine Karte dieser Zone eine Karte? (nur ohne Regeln) |
| `set_highlighted(on: bool)` | `void` | Hebt die Zonenfläche hervor (nur sichtbar, wenn `show_area` aktiv ist). |
| `insertion_index(world_point: Vector3)` | `int` | Einfüge-Index für eine Karte, die am Weltpunkt `world_point` abgelegt wird. |

