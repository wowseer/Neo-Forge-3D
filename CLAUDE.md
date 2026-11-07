# CLAUDE.md – Neo Forge 3D

## Ziel

**Eine 3D-Oberfläche, die das komplette Backend von Forge bedient – und am Ende in NeoForge lebt.**

- **NeoForge** (`~/OProgrammieren/Java/NeoForge/`, github.com/AdrianLopez98/NeoForge) ist eine neue
  Oberfläche für Forge in **JavaFX**: ein Modul `forge-gui-neo`, das in einen Forge-Checkout
  eingebaut wird (Menüs, Deckbau, Modi wie Commander, Quest, Ascent, Adventure, Online). Den
  Spieltisch zeichnet dort `forge.neo.match.NeoMatchUI` (Forges `IGuiGame`); gestartet wird eine
  Partie in `forge.neo.match.NeoGame` über Forges `HostedMatch`.
- **Integration:** Startet NeoForge eine Partie 1 gegen 1 mit eingeschaltetem 3D-Tisch
  (Einstellungen → Tisch → „3D-Tisch (Neo Forge 3D)“, Entscheidung in `ClashTable.useFor`),
  übernimmt unser `RemotePlayerController` den menschlichen Sitzplatz und NeoForge startet diese
  Godot-UI als Spieltisch (lokaler Socket, Protokoll v3). Deckauswahl, Formate und Modi bleiben
  bei NeoForge – hier wird dafür **keine** UI gebaut.
- Bis dahin läuft die UI eigenständig gegen Forge über `forge-bridge/` (Java-Prozess, stdin/stdout).
- Jede Entscheidung, die die Engine einem Spieler abverlangt, trifft der Spieler in der 3D-Ansicht
  selbst; jeder Teil des Spielzustands ist auf dem Tisch oder im HUD sichtbar. Die KI steuert nur
  Gegner, nie den menschlichen Spieler.
- Ein eigenes Regelwerk wird **nicht** wieder eingeführt: Die UI prüft keine Regeln, sie zeigt den
  Zustand der Engine an und schickt ihr die Antworten des Spielers.
- **Nicht verwechseln:** `~/Entwicklung/Java/forge/rust/` (github.com/wowseer/forge-rules-rs) ist ein
  eigenständiges Projekt, das Forges Java-Regeln in Rust neu schreibt. Es ist **nicht** NeoForge;
  von dort stammt nur das Vorbild unseres Entscheidungsmodells.

## Entscheidungsmodell und Protokoll (verbindlich)

- **Protokoll-Modell** (angelehnt an `forge-engine/src/decision.rs` aus forge-rules-rs):
  `Decision` (was die Engine fragt), `Choice` (Antwort), `Action` (Prioritäts-Aktion), `Target`,
  `PayAction`, `CardChoiceReason` – in Rust im godot-freien Crate `rust/crates/clash-protocol/`
  (`decision.rs`); `rust/src/rules/` ist nur die Godot-Hülle (`Request` mit Dictionaries, `json.rs`).
- Das Protokoll zur Java-Brücke bildet diese Typen **1:1** ab (Variantenname als `decision` /
  `choice`, Felder mit denselben Namen). Die Java-Brücke übersetzt Forges `PlayerController`-Aufrufe
  in `Decision`s und die `Choice`s zurück.
- Die Rust-UI spricht nur mit einer **Engine-Schnittstelle** (`RulesEngine`, Anfragen/Zustand),
  nie direkt mit Java-Details. UI-Code darf keine Annahmen über Java/Forge enthalten.
- Fragt Forge etwas, das das Basismodell nicht kennt (z. B. Reihenfolge von Ersatzeffekten,
  Startspieler, Abstimmung, Marken-Typ), gibt es eine allgemeine Erweiterung (Liste von Optionen →
  `Choice::Number(index)`). Jede Erweiterung in `docs/Forge-Protocol.md` unter
  „Erweiterungen gegenüber dem Basismodell“ auflisten.
- IDs: `CardId` = Forge-Karten-ID, `PlayerId` = Sitzplatz-Index.
- **Genau zwei Spieler:** Der 3D-Tisch ist nur für Partien mit zwei Spielern (du und ein Gegner,
  auch Commander 1 gegen 1). Partien mit mehr als zwei Spielern startet NeoForge automatisch mit
  seiner 2D-Oberfläche – dafür wird hier **keine** UI gebaut. Code arbeitet trotzdem mit
  `PlayerId`s (Sitzplatz-Index) statt „dem Gegner“ als Sonderfall, wo das nichts kostet.
- **Umfang dieses Projekts:** Formate Standard, Pioneer, Modern, Legacy, Vintage, Pauper, Commander,
  Booster Draft, Cube. **Keine UI bauen für** Archenemy, Planechase (Planar-Würfel), Vanguard,
  Conspiracy, Un-Sets (Contraptions, Attractions, Sticker), Alchemy/Digital-only, Ante. Im Scope
  sind u. a. Dungeons/Initiative, Monarch, Tag/Nacht, „The Ring“, Embleme, Energie/Gift/Erfahrung,
  Sagas/Klassen/Räume, Schlachten, Commander-Zone.

## Weitere Anweisungen (vor der Arbeit lesen)

| Bevor du … | lies |
|---|---|
| eine Forge-Entscheidung oder Zustandsanzeige einbaust | `docs/Kartenabdeckung.md` (alle 113 Forge-Entscheidungen → 13 Bedienmuster, nötiger Zustand, Reihenfolge) |
| eine Animation baust oder änderst | `docs/claude/ANIMATION.md` (Grundsätze, Timing-Tabelle, Vorlage) |
| ein Spielfeld / eine Kulisse baust oder überarbeitest | `docs/claude/SPIELFELD.md` (Detailgrad, Textur-Pipeline, Vorlage) |

## Architektur

```
Godot 4.7 (godot/)                Rust-GDExtension (rust/)                 Java (forge-bridge/)
table.tscn  ─────────────▶  TableController  Hover, Drag-and-Drop, Kamera
                            ForgeTable       Forge-Zustand → Card3D/Zonen,
                                             Spielerzüge → Antworten
                            TurnHud          liest HudView vom ForgeTable
                            ForgeBridge  ── JSON Lines stdin/stdout ──▶  clash.forgebridge.BridgeMain/ForgeBridge
                                         (eingebettet: TCP-Socket zu NeoForge) └─ Forge 2.0.15 (forge-bridge/forge/)
```

Eingebettet in NeoForge: `forge.neo.clash.ClashTable` (NeoForge-Repo) öffnet einen Port, startet
Godot mit `-- --clash-port=N` und hängt den menschlichen Sitz (`RemoteLobbyPlayer`) an die Brücke.

| Ort | Inhalt |
|---|---|
| `rust/src/forge/bridge.rs` | `ForgeBridge`: Godot-Knoten, Signale je Nachricht, `answer()`; Prozess/Socket über `clash-forge-client` |
| `rust/src/forge/table.rs` | `ForgeTable`: gleicht den Tisch mit `state` ab (Karten-ID → `Card3D`), Deck-Stapel, Leuchten spielbarer Karten, Drops → Optionen, `hud_view()` |
| `rust/src/table/` | Darstellung: `Card3D`, `Zone`, `TableCamera`, `TableController` (regelfrei), Kulisse |
| `rust/src/game/` | Anzeige-Bausteine: HUD, Medaillons, Phasen-Zeitstrahl, Mana-Typen, Kartendaten für das Kartenbild |
| `rust/src/rules/` | Engine-Grenze der UI: Godot-Hülle um `clash-protocol` (`Request` mit Dictionaries, `target_from_var`, `json.rs` Variant ↔ JSON) |
| `rust/crates/clash-protocol/` | Protokoll-Modell (`Decision`, `Choice` …), JSON-Form, Befehle (`command`), `RulesEngine` – ohne Godot, wiederverwendbar |
| `rust/crates/clash-forge-client/` | `ForgeClient`: Java-Brücke starten oder mit Gastgeber verbinden (`--clash-port`), Lesethread – ohne Godot; Beispiel `examples/autoplay.rs` |
| `rust/src/game/decision_view.rs` | `Decision` → Auswahlfenster → `Choice` (engine-neutral) |
| `forge-bridge/src/clash/forgebridge/` | öffentlich: `ClashBridge` (Fassade), `TableLauncher` (Tisch starten + Socket); `engine/` intern: `RemotePlayerController` (Forge-Aufrufe), `Decisions` (→ `Decision`, Antwort-`Choice` lesen), `StateJson` (Zustand → JSON), `LogForwarder` (Spielprotokoll), `BridgeCore` (Befehle, Partie, Anfragen); `app/BridgeMain` (eigenständiger Prozess) |
| `integrations/neoforge/` | NeoForge-Anbindung, getrennt mitgebbar: `ClashTable.java` (maßgeblich hier) und `neoforge-host.patch` (NeoForges Klassen) |
| `docs/Embedding.md` | Einbau der Bausteine in andere Projekte (Rust-UIs, andere Sprachen, Java-Gastgeber, NeoForge) |
| `forge-bridge/forge/` | lokale Kopie von Forge (Jar + `res/`), **nicht im Git** (GPL, Kartendaten von WotC) |
| `docs/Forge-Protocol.md` | Protokoll (Befehle, Nachrichten, `state`-Aufbau). **Bei jeder Protokolländerung pflegen** |

Forge-Sitzplatz 0 = Spieler unten (ferngesteuert), Sitzplatz 1 = Gegner oben (Forge-KI).

## Stand

**Protokoll v3 steht:** `Decision`/`Choice` in `rust/crates/clash-protocol/` (Engine-Grenze,
`RulesEngine`), Java-Übersetzer `forge-bridge/.../Decisions.java`, UI-Abbildung
`rust/src/game/decision_view.rs`. Jedes weitere Bedienmuster direkt gegen die passende
`Decision`-Variante bauen. Fertig: `ChooseTargets` per Klick/Pfeil, Aktivieren per Klick,
`DeclareAttackers` / `DeclareBlockers` per Klick mit Kampfpfeilen, `PayMana` (Quellen anklicken,
Automatik). Gespielt wird über Forges menschlichen Ablauf `PlaySpellAbility.playSpellAbility`
(im Jar), der die überschriebenen Controller-Methoden fragt. 3D-Bühnen (`table/showcase.rs`) für
`ChooseCards`, `MulliganBottom`, `Scry`; `OrderTriggers` im Auswahlfenster. Kosten-Auswahlen
(`RemoteCostDecision`), `AssignCombatDamage`, `DivideAmount`, `PayUnlessCost`,
`PayLifeForPhyrexian`, Trigger über den menschlichen Ablauf. Nächster Schritt laut Fahrplan:
Sichtprüfung aller neuen Anzeigen durch den User, danach Deckauswahl/Formate; alle
Entscheidungen im Umfang sind ferngesteuert, der Zustand ist vollständig.

Ferngesteuert, also vom Spieler in der UI entschieden:
- [x] Priorität: Karte oder Fähigkeit spielen bzw. passen (`chooseSpellAbilityToPlay`): Karte aus der Hand auf den Tisch ziehen, Button/Leertaste = passen
- [x] Mulligan (`mulliganKeepHand`): Auswahlfenster im HUD
- [x] Wirken über Forges menschlichen Ablauf (`PlaySpellAbility.playSpellAbility`): optionale Kosten, Modi, Typ, X, Ziele per Klick (`ChooseTargets`), Mana bezahlen (`PayMana`)
- [x] Allgemeine Auswahl (`Decisions.java`, `decision_view.rs`, `choice_panel.rs`): Modi, Optionen, Ja/Nein, Zahlen, Farben, Kartennamen, optionale Kosten, Startspieler (Muster M9–M12)
- [x] Kampf: Angreifer und Blocker per Klick; Kartenfächer für Kartenwahl, Mulligan, Hellsicht; Reihenfolge von Triggern

Angezeigt:
- [x] Hand, Spielfeld, Länder-Reihe, Friedhof, verdeckte Gegnerhand, getappt
- [x] Bibliothek als Deck-Stapel; Ziehen gleitet animiert vom Deck auf die Hand
- [x] Phase, Leben, Runde, Spieler am Zug, Spielende
- [x] Stack: Zauber und Fähigkeiten in der Tischmitte, Ziele als violette Pfeile, nur als 3D-Stapel (keine 2D-Liste)
- [x] Spielprotokoll oben rechts
- [x] Kommandozone ohne eigene Fläche (Commander nur als Kopie am Ende der Hand, auch beim Gegner, Kosten mit Commander-Steuer; Embleme/Dungeons/Effekte am Ende der Artefakt-Reihe), Monarch/Initiative/Stadtsegen/Ring, Tag/Nacht, doppelseitige Karten (Umdreh-Animation), verdeckte Karten
- [x] Kartenbild aus Engine-Daten: Regeltext mit Symbolen, Typzeile, Farben, Stärke/Widerstand, Loyalität, Verteidigung
- [x] Exil-Stapel, Marken- und Spielstein-Plaketten, angelegte Karten unter dem Träger, Spieler-Marken

## Fahrplan

Jeder Punkt ist ein eigener Schritt: Brücke (Java), Protokoll, Rust/UI, Doku, Test, Commit.
Die vollständige Liste mit Zuordnung zu Bedienmustern steht in `docs/Kartenabdeckung.md`.

**Entscheidungen des Spielers** (`Decision` → Forge-Methode in `RemotePlayerController`):
1. ~~`ChooseTargets`~~ ✔ (Wirken, Trigger, Spielerwahl für Effekte; `ChooseCards` auf dem Tisch per Klick; offen: `chooseNewTargetsFor`)
2. ~~`DeclareAttackers`~~ ✔ (offen: Erschöpfen/Rekrutieren)
3. ~~`DeclareBlockers`~~ ✔ (Reihenfolge der Blocker nur mit alter Regel-Option, ferngesteuert)
4. ~~`PayMana`~~ ✔ (offen: Pool-Mana gezielt wählen, `chooseManaFromPool`, andere Kosten wie Opfern/Abwerfen)
5. ~~`Priority` mit `Action::Activate` per Klick, Menü bei mehreren~~ ✔
6. ~~`ChooseCards`, `Scry`, `OrderTriggers`, `MulliganBottom`, Delve, Convoke, Improvise~~ ✔
7. ~~`AssignCombatDamage`, `PayUnlessCost`, `PayLifeForPhyrexian`, Kosten, Trigger-Ziele, Erschöpfen/Rekrutieren, Kopien, neue Ziele~~ ✔

**Zustand, der noch fehlt:**
- ~~Stack mit Zaubern und Fähigkeiten, Ziele als Pfeile~~ ✔ (Zone `Stack`, 3D-Stapel)
- ~~Exil, Marken (Counter), angelegte Karten (Auren, Ausrüstung), Spielsteine~~ ✔
- ~~Regeltext auf dem Kartenbild; Kartenmodell verschlankt~~ ✔ (`CardDef::from_state`, `card_text.rs`)
- ~~Spielprotokoll~~ ✔ (Nachricht `log` aus Forges `GameLog`, oben rechts im HUD)
- ~~Kommandozone (Embleme, Commander, Dungeons, Ring), doppelseitige/verdeckte Karten~~ ✔

**Als Nächstes:** reine 3D-Oberfläche – alle 2D-Fenster und HUD-Anzeigen in die 3D-Szene holen.
Liste zum Abhaken: `docs/UI-Aktionen.xlsx` – **„Restliste“ (alles, was noch offen ist: 2D-Fenster,
2D-HUD, Mechaniken ohne eigene Darstellung, Fehlendes; mit vorgeschlagener Reihenfolge – hier
abhaken)**, dazu „Bausteine“ (neue 3D-Bausteine in Bau-Reihenfolge), „Aktionen“ (87
Interface-Aktionen) und „Elemente“ (jedes sichtbare Element und was daraus wird).
NeoForge-Integration steht (siehe Ziel). Mehr als 2 Spieler gibt es im 3D-Tisch nicht (NeoForge
nimmt dann seine 2D-Oberfläche).
Deckauswahl und Formate übernimmt NeoForge.

## Quellcode von Forge und NeoForge

- Laufzeit ist das Jar von **Forge 2.0.15** (`forge-bridge/forge/`). Signaturen immer gegen dieses
  Jar prüfen (`javap -cp <jar> <Klasse>`); der Compiler in `build.sh` meldet Abweichungen.
- Java-Quellcode zum Nachlesen: `forge-bridge/forge-game/`, `forge-bridge/forge-ai/`,
  `forge-bridge/forge-core/` (aus dem Fork `~/Entwicklung/Java/forge`, Branch `rust-godot`).
  Der Ablauf für menschliche Spieler steht in `forge-game/.../player/PlaySpellAbility.java`.
  Nur lesen, nie ändern. Der Fork kann vom Jar abweichen: Logik aus dem Quellcode übernehmen,
  aber nur Methoden verwenden, die es in 2.0.15 gibt.
- NeoForge (JavaFX-Oberfläche) im Quellcode: `~/OProgrammieren/Java/NeoForge/forge-gui-neo/`
  (Partie-Start `forge/neo/match/NeoGame.java`, Tisch `NeoMatchUI.java`). Gebaut wird es als Modul
  im Forge-Checkout `~/OProgrammieren/Java/forge` (Forge auf NeoForges `FORGE_REF`, Modul im
  `pom.xml`, NeoForges `BUILDING.md`) mit Maven aus `~/.local/opt/apache-maven-*` und dem System-JDK
  (27 geht): `tools/neoforge_build.sh`, starten mit `tools/neoforge_play.sh`. Eine installierte
  itch.io-Version (`~/Games/NeoForge/lib/app/`) ist nur für `tools/neoforge_patch.sh` /
  `neoforge_clash.sh` nötig und gerade nicht vorhanden. Der Integrationscode liegt im NeoForge-Repo
  auf dem lokalen Branch `neo-forge-3d` (`main` bleibt wie upstream); dort nur lokal committen,
  **nie pushen** (Remote gehört AdrianLopez98).
- forge-rules-rs (Rust-Neuschreibung der Regeln, nicht NeoForge): `~/Entwicklung/Java/forge/rust/`
  – Vorbild des Entscheidungsmodells (`forge-engine/src/decision.rs`). Nur lesen.

## Bauen, starten, testen

| Was | Befehl |
|---|---|
| Rust bauen | `cd rust && cargo build` (auch der dev-Build ist optimiert, `rust/Cargo.toml`: Godot lädt `target/debug/`) |
| Unit-Tests | `cd rust && cargo test` |
| Brücke bauen | `forge-bridge/build.sh` (nutzt `forge-bridge/forge/`, sonst `~/Games/Forge`) → `build/forge-bridge.jar` (Programm) + `build/forge-bridge-core.jar` (Bibliothek) |
| Brücke von Hand | `forge-bridge/run.sh`, dann JSON-Zeilen tippen, z. B. `{"cmd":"ping"}` |
| Brücken-Code nach NeoForge kopieren | `tools/sync_neoforge.sh` und danach `tools/neoforge_patch.sh` (nach jeder Änderung an `forge-bridge/src/` oder `integrations/neoforge/src/` – sonst spielt NeoForge mit der alten Brücke) |
| NeoForge-Klassen geändert (im NeoForge-Repo, lokal committet) | `tools/neoforge_export_patch.sh` → `integrations/neoforge/neoforge-host.patch` neu (Änderungen an NeoForge immer dort festhalten) |
| Crates ohne Godot testen | `cd rust && cargo test -p clash-protocol -p clash-forge-client`; Beispiel `cargo run -p clash-forge-client --example autoplay -- <forge> <bridge.jar> <deck1> <deck2>` |
| NeoForge mit 3D-Tisch bauen (Maven) | `tools/neoforge_build.sh` (Patch, Sync, Modul in den Forge-Checkout, `mvn package`) |
| NeoForge mit 3D-Tisch spielen | `tools/neoforge_play.sh` (Daten in `~/.local/share/NeoForge`, Log `neo/neo.log` dort; Testbefehl: `NEOFORGE_CMD=clash`, Godot ohne Fenster über `CLASH_GODOT=<Hülle mit --headless>`) |
| NeoForge ins Anwendungsmenü | `tools/neoforge_menu.sh` (nie NeoForges `linux/add-to-menu.sh` aus dem Quellcode: zeigt auf ein nicht vorhandenes `bin/NeoForge`) |
| Spieltisch exportieren | `tools/export_table.sh` → `export/linux/` (`neo-forge-3d.x86_64` + `.so`, Release ohne Klassen-Doku; braucht Godots Exportvorlagen). `tools/neoforge_build.sh` legt ihn nach `forge-gui-neo/target/clash3d/`, NeoForge startet dann ihn statt Godot + Projektordner |
| NeoForge-Patch bauen (ohne Maven) | `tools/neoforge_patch.sh` → `forge-bridge/build/neoforge/neoforge-clash-patch.jar` |
| NeoForge-Partie mit 3D-Tisch | `tools/neoforge_clash.sh [--deck=NAME] [--timeout=SEK]` (Testkopie der Installation, eigene Spielstände bleiben unberührt) |
| Godot | `godot` (`/usr/bin/godot`, 4.7.2 aus dem System, im PATH) |
| Spiel starten | Godot öffnen, F5 (Hauptszene `godot/table.tscn`) |
| API-Referenz | `python3 tools/gen_api_doc.py` → muss „ohne Beschreibung: 0“ melden |

**Testdecks:** `forge-bridge/testdecks/` (`spieler.dck` / `gegner.dck` = gemischte Standard-Decks; `auswahl.dck` löst viele Auswahl-Dialoge aus; `kartenwahl.dck` Hellsicht, Abwerfen, Bibliothek durchsuchen; `kosten.dck` + `gegner_counter.dck` Kosten, Verteilen, phyrexianisches Mana, Trigger-Ziele, Mana Leak; `rot.dck` / `gruen.dck` nur Länder + eine Kreatur).

**Partie ohne Fenster testen:** ein temporäres SceneTree-Skript (z. B. `godot/_probe.gd`, danach
wieder löschen), das `res://table.tscn` lädt, sich an `ForgeBridge.decision_requested` hängt und im
nächsten `_process` über `ForgeTable` bzw. das HUD antwortet (`submit_choice`, `pass_priority`, `TurnHud.on_choice_option`). Nicht direkt im
Signal-Handler antworten. Start:
`godot --headless --path godot -s res://_probe.gd`. Forge braucht ca. 5 s zum Laden; das
Brücken-Log steht in `~/.local/share/godot/app_userdata/Neo Forge 3D/forge-bridge.log`.

Lange Spiel- und Belastungstests immer mit `--headless`: Ein verdecktes Testfenster kann die
Bildausgabe (Vulkan, vsync) blockieren, dann steht der Hauptthread scheinbar „eingefroren“ – das
ist kein Fehler im Spiel. Ausgaben von Testskripten mit `printerr` (ungepuffert); in Shell-Befehlen
nie `pkill -f <muster>` mit einem Muster, das im eigenen Befehl steht (beendet die eigene Shell) –
z. B. `pkill -f "[B]ridgeMain"`.

Fenster-Renders (`--write-movie`) öffnen sich auf dem Desktop des Users und bekommen dessen echte
Mausklicks: nur für Optik verwenden, Spiellogik ohne Fenster prüfen, nie `warp_mouse`.

## Regeln für die Arbeit

- **Videos des Users sind Vorlagen für allgemeine UI-Regeln, keine Testfälle.** Der User schickt
  immer wieder Videos (z. B. in `~/Dokumente/MTG Clash/` oder im Projektordner), die ein
  Verhalten zeigen – meist an einem Beispiel mit zwei Karten. Nicht dieses Beispiel nachbauen
  oder Karten hartcoden, sondern die dahinterliegende Regel erkennen („jede Karte mit mehreren
  Spielweisen bekommt eine Auswahl“, „ein Zauber, der auf etwas auf dem Stack reagiert, bekommt
  einen Pfeil dorthin“) und sie allgemein in der 3D-UI umsetzen, für alle Karten und Fälle.
  Getestet wird mit den vorhandenen Testdecks, nicht mit Decks aus den Karten im Video.
- **Sprache:** Code-Kommentare und `///`-Doku auf **Deutsch**. Die Entwickler-Doku für GitHub ist
  **Englisch**: `README.md`, `forge-bridge/README.md`, `integrations/neoforge/README.md`,
  `docs/DOCUMENTATION.md`, `docs/Forge-Protocol.md`, `docs/Embedding.md`, `docs/Mechanics.md` (jede Mechanik mit Bild aus `docs/bilder/`, nur mit `CLASH_FREE_ASSETS=1` aufgenommen). Interne Pläne bleiben
  Deutsch (`CLAUDE.md`, `docs/claude/`, `docs/Kartenabdeckung.md`, `docs/UI-Aktionen.xlsx`).
  Commit-Messages auf Englisch. Projektname überall „Neo Forge 3D“ (getrennt geschrieben).
- **Doku immer mitpflegen**, im selben Schritt wie die Änderung:
  1. `///`-Doku an jeder neuen oder geänderten Godot-Klasse, `#[func]`, `#[export]`, `#[var]`, `#[signal]` (erscheint in der Godot-Hilfe, F1)
  2. `docs/DOCUMENTATION.md` (Bedienung, Szenenaufbau, Stand)
  3. `docs/Forge-Protocol.md` bei jeder Protokolländerung
  4. `python3 tools/gen_api_doc.py`
- **Nach jeder Aufgabe committen** auf `main` (`git add . && git commit`). Vorher `git status`
  ansehen: nur eigene Änderungen committen, fremde (z. B. vom User gelöschte Dateien) erwähnen.
- Shell ist zsh: Platzhalter ohne Treffer (`rm godot/_probe*.uid`) brechen den ganzen Befehl ab.
  Doku-Skripte in einem eigenen Aufruf ausführen und ihre Ausgabe prüfen, nie an `rm` & Co. hängen.
- Rust-Build ohne Warnungen halten; toten Code löschen statt mit `allow` zu verstecken. Einzige
  Das Protokoll-Modell (`clash-protocol`) ist eine Bibliothek: ihre öffentlichen Teile gelten
  nicht als tot, auch wenn die UI sie (noch) nicht nutzt.
- **Modular halten:** `clash-protocol` und `clash-forge-client` bleiben ohne Godot (nur
  `serde_json`); Godot-Typen nur in `rust/src/`. In Java ist nur `clash.forgebridge.ClashBridge`
  und `TableLauncher` öffentliche Schnittstelle, Forge-Details bleiben in `engine/`. Eingriffe in
  Gastgeber (NeoForge) nie in der Brücke, sondern in `integrations/<gastgeber>/` bzw. als Patch.
- Texte im Spiel zweisprachig: `tr!("Deutsch", "English")` / `trf!` aus `rust/src/i18n.rs`.
- **Kosten immer als Mana-Symbole:** Jedes UI-Element, das Kosten zeigt, nimmt den Baustein
  `rust/src/table/cost_symbols.rs` (`cost_row`, `rich_label`; in `Button3D`-Texten und Bühnen-Beschriftungen
  reicht „{1}{U}“ im Text). Symbole aus dem Symbolbogen `Assets/Mana/Mana.svg`
  (`game/mana_symbols.rs`; farblos {C} aus `Assets/Mana/Colorless.svg`), nie „{…}“ als Text und keine eigenen Motive.
- Godot-Logik in Rust (gdext), kein GDScript im Spiel (nur temporäre Testskripte).
- **Sparsam je Bild** (Messungen und Begründung: `docs/DOCUMENTATION.md`, Abschnitt Performance):
  In `process()`/`draw()` nur Geändertes setzen (Transformationen, Shader-Parameter,
  Theme-Overrides, Sprites); Animationen am Ziel einrasten lassen. 2D-Anzeigen, die sich jedes Bild
  bewegen, in ruhenden Teil (nur bei Änderung `queue_redraw`) und eigene animierte Ebene trennen;
  viele Flächen über `game/tri_batch.rs` statt einzelner `draw_polygon`/`draw_circle`. Shader und
  erzeugte Bilder über `table/shared.rs` teilen, nie `Shader::new_gd()` je Instanz. Was ohne Eingabe
  oder Engine-Nachricht eine Animation startet, ruft `table::frame_pacer::wake()`.
- Lizenz: das ganze Projekt ist GPL-3.0 wie Forge (`LICENSE`). Keinen Forge-Code in `rust/`
  übernehmen (die Grenze bleibt das Protokoll). `forge-bridge/forge/` (Jar + Kartendaten) nie
  committen. Bilder für README/Doku nur mit `CLASH_FREE_ASSETS=1` (keine Kartenbilder, keine
  Symbole aus `Assets/Mana/*.svg`).
- Begriffe in Code und Doku wie im Protokoll-Modell und den Comprehensive Rules (CR-Nummern in Kommentaren,
  wo eine Regel umgesetzt bzw. angezeigt wird).
