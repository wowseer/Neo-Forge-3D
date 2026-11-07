# Neo Forge 3D – Documentation

A card game in a 3D arena. The presentation is written in **Rust** (godot-rust / gdext 0.5.5) and
loaded into **Godot 4.7** as a GDExtension. **All game rules come from Forge** (Java rules engine,
see section 5); there is no rules implementation of our own. The game is bilingual
(**German / English**, see section 6).

This file gives the overview. Every single class, property, function and signal is described

- **directly in Godot:** editor → *Help → Search Help* (F1) → enter the class name,
  e.g. `ForgeTable`, `Card3D`, `Zone`;
- **as a file:** [API-Referenz.md](API-Referenz.md) (German).

Both are generated from the same Rust doc comments (`///`) (gdext feature `register-docs`).

---

## 1. Building and running

| Step | Command |
|---|---|
| Build Rust | `cd rust && cargo build` (release: `cargo build --release`) |
| Unit tests | `cd rust && cargo test` |
| Build the Forge bridge | `forge-bridge/build.sh` (see section 5) |
| Start the game | open the Godot project `godot/`, F5 (main scene `table.tscn`; needs Java 17+ and Forge) |
| Regenerate the API reference | `python3 tools/gen_api_doc.py` |

Restart the game after every Rust build so the new library is loaded.
`godot/rust.gdextension` points to `rust/target/debug/` in debug and to `rust/target/release/` in
exports.

### Performance

The game is meant to stay light on CPU, GPU and battery:

- **Optimised debug build.** Godot (also when NeoForge starts the table) loads the library from
  `rust/target/debug/`, so `rust/Cargo.toml` builds the `dev` profile optimised (`opt-level = 1`
  for the game, `3` for dependencies such as `godot`; debug info stays). Unoptimised, the Rust
  code alone needed about twice the CPU time per frame.
- **Frame rate (`FramePacer`, `table/frame_pacer.rs`, node in `table.tscn`).** While something
  happens – any input, every message from the engine and `hold_seconds` (2 s) after it, long
  enough for the animations it triggers – the table draws at `active_fps` (60; with V-Sync on a
  monitor of up to 60 Hz, V-Sync alone limits). At rest it draws at `idle_fps` (30), minimised
  at `minimized_fps` (10). Any input switches back to the full rate at once. Other code reports
  activity with `frame_pacer::wake()`. Change the values in the inspector of the `FramePacer`
  node (0 = unlimited).
- **Per-frame code writes only what changed.** Card images settle at their target and are then
  no longer moved (`Card3D`: every transform change updates all parts of a card in the rendering
  server and its shadows), counters/shields/cost rows/3D buttons/camera likewise. 2D displays
  that animate every frame keep their resting parts and redraw only the moving ones: medallions
  (`SeatBadge`: ring, figure, plate only on change; target ring, ray crown and eyes in own
  canvas items) and the phase timeline (only the current, pulsing station). Many filled shapes
  are drawn as one triangle array (`game/tri_batch.rs`) – Godot creates GPU buffers for every
  single `draw_polygon`/`draw_circle`.
- **Shared resources.** Shaders with the same code and generated images (tap symbol) exist only
  once (`table/shared.rs`), not once per card, button or effect – no shader compiling when new
  cards or effects appear.
- **Java bridge memory.** `clash-forge-client` starts the JVM with G1 settings that return unused
  heap (`JVM_MEMORY_ARGS`): Forge holds only about 200 MB of live data during a match, but the
  JVM used to keep about 1.2 GB; now about 0.7 GB (same `-Xmx`). Each new state is converted to
  Godot data only once (it used to be converted twice).

Measured with an AI game at 1152×648 (RTX 3070 Ti, 1 core = 100 %, before → after):

| | before | after |
|---|---|---|
| CPU Godot, match running (60 fps) | 41 % | 21 % |
| CPU Godot, resting table | 68 % (60 fps) | 17 % at 60 fps, 10 % with `FramePacer` (30 fps) |
| Memory Godot (resident) | about 1.95 GB | about 1.0 GB |
| Memory Java bridge (resident) | about 1.2 GB | about 0.7 GB |

**Own graphics only** (e.g. for images that must not show protected material): set the
environment variable `CLASH_FREE_ASSETS=1` or start Godot with `-- --free-assets`
(`rust/src/assets.rs`). The table then shows no card images (Scryfall) and uses no symbols from
`Assets/Mana/*.svg`, only self-drawn card frames with type symbols (creature: crossed blades,
instant: lightning bolt, sorcery: magic circle, artifact: gear, equipment: sword, planeswalker:
crown …) and its own mana symbols (sun, drop, skull, flame, tree, diamond). The images in the
README were made this way.

---

## 2. Structure of the scene `table.tscn`

```
Table (TableController)          root: mouse input, drag and drop, camera focus
├── WorldEnvironment, Lantern    light and atmosphere
├── Arena (Arena)                procedural arena with game board (scenery)
├── Camera (TableCamera)         overview / close-up
├── Player (PlayerSeat)          player at the bottom (Forge seat 0)
│   ├── Battlefield (Zone)       battlefield (automatic layout, `auto_arrange`)
│   ├── Deck, Discard (Zone)     deck, graveyard
│   ├── Exile (Zone)             exile (face-up pile between graveyard and table centre)
│   ├── Resources (Zone)         land row
│   ├── ManaPool (ManaPool)      mana pool (display)
│   ├── HeroAnchor (Marker3D)    position of the medallion (directly above the hand)
│   ├── ManaAnchor (Marker3D)    right end of the mana symbols (below the land row; more mana types grow to the left)
│   └── Hand (Zone)              hand cards (fan, close to the camera at the bottom edge)
├── Opponent (PlayerSeat)        opponent at the top (Forge AI, seat 1), same structure, rotated
│                                by 180°; hand at the top edge, medallion directly below it
├── Stack (Zone)                 stack upright at the right edge (kind 4, floats slightly above the board)
├── FramePacer (FramePacer)      frame rate: full while something happens, lower at rest
├── TurnHud (TurnHud)            screen display, reads from the ForgeTable
├── ForgeBridge (ForgeBridge)    Java process with Forge
└── ForgeTable (ForgeTable)      syncs the table with Forge, carries out moves
```

The `TableController` knows no rules: it reports every drop via the `card_dropped` signal; the
`ForgeTable` plays the card through Forge or puts it back in its place.

**Table layout** as in Arena (reference `mtg_arena_spielfeld.png`), per side from the outside in:
at the bottom **graveyard, deck, exile** on the left and the **hand** to the right of them; above
them **lands** on the left and **artifacts/enchantments**
on the right (the player's **medallion** sits centred directly above the hand, as in the video "Blocken"), right-aligned up to the table edge; above that the **creatures** across the full width. The opponent is only mirrored
in depth (their piles top left, artifacts on the right from our point of view; `Zone.mirror_x`).
The **stack** stands upright at the right edge. There is **no command zone** on the table: the
commander lies in its owner's hand fan, emblems and dungeons at the end of the back row. If creatures
and the back row do not fit into the depth of the battlefield, or the creatures do not fit into
its width, all tiles shrink together (the row behind only counts if the creatures reach over it);
with few creatures the tiles are large (`card_scale` 2.7). The creature area is deep enough for
a full-size creature and a pile of two (1.65 units); only taller piles or a crowded row make the
creatures smaller. The back row (lands, artifacts) keeps room towards the creatures for tapped,
tilted cards (`BACK_ROW_TAP_ROOM`).

**Battlefield layout** (`Zone.auto_arrange`, both battlefields): cards are placed automatically by
type, each from the point of view of its side ("up" = towards the table centre):

| Area | Cards |
|---|---|
| front, centred at the middle line | creatures in one row; if they do not fit, they get smaller, then two rows |
| back row left (left of the medallion) | lands (own land row): left to right, lands of the same name stacked overlapping, groups by mana colour; tapped lands form their own pile directly to the right of the untapped ones of the same name. The land row lies far enough behind the creature row that tilted (tapped) lands do not touch the creatures. Land row, deck, graveyard and exile lie high enough that the hand does not cover them, the mana pool below them between lands and hand |
| creature row, left | **planeswalkers** on the far left of the creature row; their size depends only on their number (full size up to two, then gently smaller); from the third on they lie in two rows. Their area on the left spans the full depth of the battlefield, the lands only start to the right of it. The creatures lie centred to the right and shrink when there are many; a second creature row appears once one row would have to shrink below 60 % and two rows allow larger cards. Tall piles move closer together. Creatures and planeswalkers never reach into the lower row (lands left, artifacts right); with many groups, lands also get two rows within their zone without touching the exile below |
| back row right (right of the medallion) | everything else (battles, artifacts, equipment, enchantments …) in the row behind the creatures, **sorted by category** (artifacts and equipment, then enchantments and auras, then the rest – emblems, dungeons, "The Ring" and other effects from the command zone) and **right-aligned up to the table edge** (beyond the battlefield zone); with many of them they move further left (at most to just right of the medallion above the hand, `BACK_ROW_START`; they never lie over it); they **never overlap** and, as soon as that allows larger cards, form **two rows** (the upper one reaches up beside the creature row, right of the rightmost creature) – only when even two rows do not fit do the cards get smaller – preferably split at a category boundary (artifacts in the upper row, enchantments in the lower one), each row ending at the table edge; **identical cards are stacked** like lands of the same name (same name, tap state, power/toughness, damage and counters; creatures one below the other – each further one 0.2 card lengths closer to the player and on top, the title bars below stay visible –, other cards offset side by side by 0.3 card widths; `Card3D::stack_key`); tapped, **attacking** (already when clicked during attack selection) and **summoning-sick** creatures (CR 302.6) each form their own pile (order: ready, summoning sick, attacking, tapped), at most 6 cards per pile (the 7th starts a new one beside it so creature piles do not grow over mana pool and hand), hosts of attached cards lie alone; when it gets full, **all cards of the battlefield shrink together** so creatures (centre) and the other cards (to the right) each stay in one row – down to 45 % of the size; only then two creature rows and the other cards at the very back, finally overlapping |
| bottom left | graveyard, deck (library), exile side by side; hand to the right of them (with the commander at its right end) |

**Tile design** (like `Cratzuren icons.png`): every card on the battlefield and in the land row is
a tile of its own – frame in the card's colour, name bar at the top (dark font), the image crop
(Scryfall `art_crop`, `crop` in the state) in the middle, at the bottom a bar with a light, rounded
value box (power/toughness dark, boosted blue, weakened red; loyalty or defense) and at the bottom
left a round black button with a white symbol (↷ for a tap ability, otherwise ◎); counters as
coins at the top right (`placeholder::tile_face`). Without an image crop (not loaded yet, face
down) the upper part of the front. On a **full card with its real image** (card view, hand, stages) changed power/toughness get an **own value box** drawn over the printed one (light parchment with a dark rim, covering the printed box of old and new frames completely; boosted blue, weakened red; `STATS_PLATE_*` in `card3d.rs`), so the printed numbers no longer show through. **Shape and size**: cards on the battlefield and in the land
row are **tiles** (`Zone.tile_cards`, `Card3D.set_tile`): only name and image, power/toughness at
the bottom right, slightly wider than tall; while dragging the full card appears, while hovering
the card view (see below). Tapped cards (lands too) are grey and stand slightly tilted as soon as
they are used. Size `Zone.card_scale` (battlefield 2.7, lands 2.4); if a row is too full, the
cards automatically shrink (down to 45 %), then they move closer together.

**Stage headings** name only the mechanic ("Vote", "Choose a keyword", "Revealed cards", "Name a card.", "Choose a creature type.", "Use the triggered ability?" …), never card names or rules texts; they come from the decision's `purpose` (`decision_view::purpose_title`). Card names appear only as the heading above the card at the stack.

### Scenery and screen display

The look is inspired by digital trading card games (game board from above, medallions with life
totals, a large action button), but is a design of its own without third-party graphics –
everything is generated procedurally in Rust.

**`Arena` (`rust/src/table/arena.rs`)** – default scenery of `table.tscn`:

| Element | Description |
|---|---|
| Game board | sandstone with chiselled grooves (normal map from a height map): double frame with corner rings, large diamond, middle line (z = 0) |
| Rune circle | in the centre of the board; rings, star and glyphs glow turquoise (emission texture, strength `rune_glow`) |
| Rim | flat stone rim (only 0.06 above the board) with gold inlay; glowing crystals in gold settings set into the corners – the board looks like a flat playmat |
| Surroundings | stepped plinth, slate floor, two fire bowls, column ruins, round arena wall with banners in the player (`player_color`) and opponent colour (`opponent_color`) |

The board is wide (`board_size` = 19 × 9.6 in the scene) so it fills almost the whole screen
width; the battlefields are 17 wide and reach from the middle line to the rim (z 0.15 … 4.85);
library/graveyard/exile lie on the far left (x = −8.9, for the opponent top right), the land rows
at the back left (x = −4.8, mirrored for the opponent), the stack on the right (x = 6.0). The
light cone (`Lantern.spot_angle` 46°) covers the whole board. Adjustable in the inspector:
`board_size`, `texture_resolution` (pixels per unit of the board texture), `rune_glow`,
`player_color`, `opponent_color`. The old scenery `Tavern` still exists: replace the `Arena` node
in `table.tscn` with a `Tavern` node (then set `Lantern.light_energy` back to about 10; the arena
only needs about 3 because of the bright stone).

**Visible in the editor:** `Arena`, `Tavern` and `FlickerLight` are tool classes – the scenery is
also built in the Godot editor when `table.tscn` is open. The generated nodes have no owner, do
not appear in the scene tree and are not saved into the `.tscn`. Changing a property of the
`Arena` in the inspector rebuilds it immediately (`Arena.rebuild()`). Cards, medallions and HUD
are only created when the game starts (F5). After a new Rust build, reload the scene if needed
(*Scene → Reload Saved Scene*) or restart the editor.

**Camera and layout** (as in digital card games): the overview camera looks down steeply (about
75°) from a large distance with a narrow field of view (`fov` 30°, `Camera.overview_position`
(0, 23.2, 6.4)) – so there is almost no perspective, the battlefield looks flat. Close-up
distance `focus_distance` 2.4. The player's own hand lies enlarged (scale 2.8) just above the
board in front of its front edge at the bottom of the screen (the lower half of the cards is cut
off, hovering zooms the whole card), the opponent's hand at the top edge. Both battlefields are
3.7 units deep (z = ±0.05 to ±3.75; land rows at z = ±3.0, graveyard/deck/exile at z = ±4.45).
Whoever moves the camera or the hand must move the `HeroAnchor` markers along.

**HUD (`TurnHud`)** – reads the display structure `HudView` from the `ForgeTable` every frame
(property `forge_table`):

| Area | Contents |
|---|---|
| top / bottom centre | (During a 3D stage – starting player, mulligan, discard, "Choose one" – greyed out and almost transparent.) `SeatBadge` medallion of the opponent (red, directly below their hand) / player (blue, directly above the own hand). The position comes from the `HeroAnchor` markers (`TurnHud.player_hero_anchor` / `opponent_hero_anchor`); without markers the medallions sit top / bottom left. **Medallions, names and the mana pool always lie in the background:** the screen is drawn in layers (`table/layers.rs`): the main camera (`TableCamera`) draws only the scenery (table, walls; visual layer 20, `BACKGROUND_VISUAL_LAYER`), above it the canvas layer with medallions, names and mana pool (`SEAT_CANVAS_LAYER`), above that a second camera with the same position draws everything else – cards, hand, stack, stages, 3D buttons, arrows, effects – into a transparent `SubViewport` (`FOREGROUND_CANVAS_LAYER`, composited with premultiplied alpha), and the rest of the HUD (phase timeline, choice window, hints) lies on top (`HUD_CANVAS_LAYER`). So every card, stage or effect that passes over a medallion covers it. Additive glows (card frame glow, block trail, mana effects) use premultiplied blending with alpha 0 so they stay additive on the transparent layer. Mouse rays still come from the main camera; the foreground layer lets clicks through. While a card is hovered or dragged, both medallion rows also fade softly (30 % opacity; `CardCutout`, `card_cutout.rs`). Round hero medallion as in the video "Blocken" (all self-drawn, sizes follow the window height, reference 1080 px): golden metal ring lit from the top left, inside a hooded figure in the side's colour (its hood tip reaches over the ring; `SeatBadge.portrait` replaces it with an own image), below it a dark rounded **life plate** overlapping the ring with the life total in a bold serif font, a silver and a golden blade to either side; slowly turning golden **ray crown** = active player; the plate flashes red on damage, green on healing and the number briefly grows. The player's `HeroAnchor` lies above the hand centre (x = 1.0, z = 4.5) so the plate ends just above the hand's cost symbols; the opponent's hand reaches over the far table edge into the play area (z = −5.0) and their medallion hangs directly below it, the hood slightly overlapping the card backs (`HeroAnchor` x = 1.0, z = −4.8) |
| bottom right, above the button | `PhaseTimeline`: timeline of the phases with three stations – (1) a circle for untap, upkeep, draw and main phase 1, which grows with each of these steps; (2) a "combat" dot whose three steps (attackers, blockers, damage) only unfold during combat; (3) a circle for main phase 2 and end. Symbols: untap = circular arrow, upkeep = sun, draw = card with plus, main phases = fanned cards, end = crescent moon, combat = crossed swords (attackers = sword, blockers = shield, damage = flame). The current station glows. Forge's `PhaseType` is mapped onto it (`forge/table.rs`, `phase_index`) |
| centre of the screen | only at the end of the game: "Game over – X wins!" |
| below the own land row | mana pool as round symbols (`ManaPoolView`, `mana_view.rs`), from Forge's `mana_pool` (floating mana, e.g. from rituals) |
| left screen edge, top / bottom | opponent (top left) or player (bottom left): turn number with hourglass symbol (`HourglassIcon`, `hourglass.rs`), below it the player name; the name of the active side is golden |
| centre of the screen | choice window (`ChoicePanel`, `choice_panel.rs`) for all general Forge decisions: one option, several options, yes/no, number; which engine decision produces which window is defined in `decision_view.rs` (usage see section 3) |
| bottom right | stacked: phase timeline, **phase button** "Pass »" (also space bar; in combat "All Attack" / "2 Attackers »" or "No Blocks »" / "1 Blocker »", otherwise greyed out) and below it the short button **"»»"** (end turn: only the symbols, the button is only as wide as they are – `Button3D.body_half`; greyed out during the opponent's turn). Both are orange animated 3D buttons (`Button3D` with `pin_to_screen`: just in front of the camera, fixed pixel size, in front of all cards); hidden during a 3D stage (starting player, mulligan). **In combat a second button of the same size sits above the phase button** (`TurnHud.secondary_button`, offered by `ForgeTable::combat_secondary`): blue "No Attacks" or grey "Cancel Attacks" / "Cancel Blocks" (`Button3D.style` switches in place); it grows in like every 3D button and the timeline slides up smoothly to make room for it (`STACK_LIFT_SPEED`). The engine's combat hints (e.g. invalid attack) are shown in gold above the timeline |

### Files

| Folder / file | Contents |
|---|---|
| `rust/src/table/` | presentation: `Card3D`, `Zone`, `TableCamera`, `TableController`, target and combat arrows `TargetArrows` (`target_arrows.rs`), sceneries `Arena` (`arena.rs`) and `Tavern` (`tavern.rs`), procedural textures (`textures.rs`), card images (`placeholder.rs`) |
| `rust/src/game/` | display building blocks: `PlayerSeat`, card model from the engine's card data (`cards.rs`: `CardDef::from_state`, card kind, value box), rules text symbols and font sizes (`card_text.rs`), counter names (`counters.rs`), mana types and pool (`mana.rs`), phase list (`phases.rs`), HUD (`hud.rs`), player medallion (`seat_badge.rs`), hourglass (`hourglass.rs`), mana pool display (`mana_view.rs`), phase timeline (`phase_timeline.rs`), card preview (`card_preview.rs`), choice window (`choice_panel.rs`), 3D stages for card choices in `table/showcase.rs`, engine decision → choice window → answer (`decision_view.rs`), card cut-out of the medallions (`card_cutout.rs`) |
| `rust/src/rules/` | the UI's engine boundary: the protocol model (`Decision`, `Choice`, `Action`, `Target` …, `RulesEngine`) comes from the Godot-free crate `rust/crates/clash-protocol/`; here only `Request` with card data as `Dictionary` and the conversion Godot ↔ JSON (`json.rs`). The UI only knows these types |
| `rust/crates/clash-protocol/` | protocol model + JSON form (protocol v3) and commands (`command`), without Godot – reusable ([Embedding.md](Embedding.md)) |
| `rust/crates/clash-forge-client/` | connection to the bridge: start the Java process or connect to a host, reader thread, `ForgeClient`; example `examples/autoplay.rs` (table without graphics) |
| `rust/src/forge/` | `ForgeBridge` (Java process with Forge, implements `RulesEngine`) and `ForgeTable` (match on the 3D table) |
| `forge-bridge/` | Java bridge to Forge (GPL-3.0): source code, `build.sh`, `run.sh`, test decks; `forge/` = local Forge copy (not in Git) |
| `rust/src/i18n.rs` | bilingualism: language, `tr!`/`trf!` macros, name table |
| `docs/` | this documentation, the Forge protocol, the card coverage (`Kartenabdeckung.md`, German: what is still missing for all cards) and the API reference |
| `docs/claude/` | working instructions for Claude (German): animation design, battlefields |
| `CLAUDE.md` | project goal, roadmap and working rules for Claude (German) |
| `tools/gen_api_doc.py` | generates `docs/API-Referenz.md` |

---

## 3. Controls

| Action | Effect |
|---|---|
| **Who goes first?** | If you win the coin toss, two glowing 3D buttons float one above the other over the table centre: "I go first" (orange) and "Forge AI goes first" (blue), the question above them. Mouse over = the button grows, the glow gets brighter; click = choice. Building blocks: `Button3D` (`table/button3d.rs`, collision layer 3 `CONTROL_LAYER`, takes precedence over cards in the `TableController`), look: shader `table/button_shader.rs`, a recreation of the Blender materials from `Assets/Buttons/*.blend` including drivers – fire (orange) or clouds (blue) flow in a 4 s loop, the glow pulses; colour style `ButtonStyle` |
| **Mulligan** | The opening hand appears large in front of the camera (stage `Showcase`, `table/showcase.rs`: table dimmed, cards slide in from the hand), above it who goes first and the question, below it "Mulligan" (blue, left) and "Keep" (orange, right); below that a hint how many cards another mulligan puts on the bottom of the library. Hovering a card shows it large. After the choice the cards slide back into the hand. Following the London rule, Forge asks right after every mulligan which cards go to the bottom of the library: click cards on the same stage (marked golden, clicking again removes the mark), the orange button "Done" only becomes active with the right number; then "Mulligan" or "Keep" again |
| Mouse over a card | **Hand cards** are brought large and readable in front of the camera as before (`Zone.zoom_cards` of the own hand). **All other cards** only tilt slightly; for them the **card view** (`CardInspector`, `table/card_inspector.rs`, like `Fähigkeiten.png`) shows a bright **copy** of the card large at the screen edge opposite the mouse, a bit above the screen centre so it ends above the phase timeline (`CardInspector`, `LIFT`) – the card itself stays where it is. Beside it a dark panel with the **abilities** (keywords with the engine's explanation, e.g. Flying, Lifelink, Kicker) and the **changes** (power/toughness compared to the printed values, damage, counters, commander/Ring-bearer, attached cards). If the shown card changes (loyalty after an ability, new counters, power/toughness), the view rebuilds immediately. Not for face-down cards, not while dragging and targeting, not through buttons |
| Graveyard · deck · exile (side by side at the bottom left, mirrored at the top right for the opponent) | On the left the **graveyard** (face-up cards, as large as the deck), in the middle the **deck**, on the right the **exile**: when cards are there, a dark swirling **vortex** opens (`table/exile_portal.rs`, shader: noise in polar coordinates, turning inwards), about the size of a card; cards that go into exile slide into it, shrink and disappear (`Zone.swallow_cards`). When the exile is empty, the vortex closes. **Clicking** the graveyard or the vortex (own or the opponent's) opens the cards in it as a cover flow like when searching the library ("Graveyard (n)" / "Exile (n)", newest card first, "View battlefield", "Done" or space/Escape closes). If the clicked card has an action of its own (e.g. flashback), that action is performed instead; in the view, playable cards glow, clicking one closes the view and performs the action |
| Deck | face-down pile as large as your library in Forge; the cards are larger than normal (`card_scale` 1.7 of the deck zones) so the pile is easy to see next to the battlefield cards. Drawing happens automatically in the draw step (Forge); the card visibly slides from the deck into the hand |
| Playable hand cards | glow as soon as Forge waits for your priority and you can play and pay for them right now |
| Cards from other zones in the hand | As in Arena: every card you can currently play from another zone – graveyard (flashback, escape …), exile (foretell, adventure, "you may play"), sideboard (companion), library – additionally appears as a copy fixed at the far right of the hand fan, with a gap to the real hand cards (`Zone::add_tail_card`; newly drawn cards go before it): it flies there from its zone, its frame glows white, above it is the cost to play it from that zone (e.g. flashback {2}{R} instead of {R}; for the commander including the commander tax of {2} per previous cast, CR 903.8), plus a plaque ("from graveyard", "companion" …). Drag it onto the table to play it from its zone; when it is no longer playable, the copy disappears (`ForgeTable::sync_hand_proxies`). **Commanders** (CR 903) are shown only there: as long as a commander is in the command zone, its copy always lies at the right end of its owner's hand – the opponent's too, face up and moved a little towards the table (`Zone.tail_offset`) – with its current cost including the commander tax; it glows white and can be dragged onto the table only when it can be paid for. It is not part of the opening hand on the mulligan stage. For tests outside NeoForge, `ForgeBridge.game_type = "Commander"` starts a Commander match (test decks `commander_spieler.dck` / `commander_gegner.dck`) |
| Cost of hand cards | Above each of the player's hand cards (except lands) its mana cost is shown at the top right as a row of round mana symbols that sticks out halfway over the top edge (zone with `show_costs`, building block `table/cost_symbols.rs`) |
| **Cost symbols** (everywhere the 3D UI shows costs or mana) | The symbols from the symbol sheet `Assets/Mana/Mana.svg` (`game/mana_symbols.rs`, built into the library and rendered at runtime): colours, numbers 0–20, X/Y/Z, snow, hybrid, 2/colour, Phyrexian, tap/untap – in the costs above the hand cards, on buttons and labels, in the mana pool, in the colour wheel and in the cost pips of drawn card images. Colourless mana {C} comes from `Assets/Mana/Colorless.svg` (inserted into the sheet on load). **Running text** also shows the symbols instead of "{2}{R}": the rules text on cards (as an image with embedded symbols, `cost_symbols::rich_text_texture`), the hint above the phase button, the card view on hover and the titles/subtitles of the 3D stages (`mana_symbols::fill_rich`, `cost_symbols::rich_label`). Buttons ("Pay {1}{U}", "Kicker – {R}", "Offspring {2}") and labels ("{3}" above Spree modes) show these symbols instead of "{…}". The costs above the hand cards lie one layer below buttons and stage labels (render priority 0) |
| Drag a glowing card from the hand and release it **anywhere** on the table (not over the hand) | play: Forge gets the matching option, pays the mana automatically and puts the card down (lands into the land row, permanents onto the battlefield, spells into the graveyard). While you drag, the card is also shown **large at the very top of the screen centre**. Invalid moves snap back into place |
| Phase button "Pass" or space bar | pass priority; in the declare attackers/blockers step confirm the planned attack or blocks. If you have nothing to play, the game passes automatically – **except in your own second main phase and your own end step**: there it always stops (the timeline shows the change to it) until you pass yourself |
| **Choose a colour (colour wheel)** | When Forge asks for a mana colour (`ChooseColor`, e.g. "one mana of any colour", "choose a colour", per mana for colour combinations) or a basic land type (`ChooseType` with only Plains/Island/Swamp/Mountain/Forest), a **colour wheel** appears (`ManaWheel`, `table/mana_wheel.rs`): segments white at the top, clockwise blue, black, red, green (colourless grey) with sun, drop, skull, flame, tree, the card image in the centre, above it "(card) Choose a colour.". It grows out of the card in 0.20 s; the segment under the mouse brightens, a click chooses. For engine questions (colour, basic land type, protection from a colour) the wheel stands in the **centre of the battlefield** with the source card's image in the middle of the wheel, and the question is shown at the top of the screen like a stage title so it does not lie below the medallions (`ManaWheel::set_prompt_on_top`). If a land has several mana abilities with one colour each, clicking the land opens the same wheel at the card (Escape or clicking beside it closes it without a choice), with the question directly above the wheel |
| **Colour wheel: further questions** | Which floating mana from the pool pays is, as in Arena, always decided by Forge itself (no question per mana symbol). The same wheel (motifs as in the mana pool, colourless = grey diamond) is also used for: **colour including colourless**, **several colours** (a click chooses or deselects – chosen segments bright with a golden ring, the others dimmed –, below it orange "Done", only active with a valid number) and **mana abilities of a card** like Cavern of Souls: one segment per fixed mana type, an ability "one mana of any colour" gets one segment per colour – the chosen colour also answers Forge's follow-up question. If two possibilities have the same colour (e.g. pool mana from two sources), there are buttons instead |
| **Cancel casting / activating** | For every question Forge makes cancelable while casting one of your cards or activating one of your abilities (targets, modes, X, options …): the **phase button** reads "Cancel" (also space bar), a small grey **✕ button** sits at the top right of the card, **Escape** cancels, and you can **drag the card back into your hand** with the arrow. Forge returns the card and asks for priority again. Covering stages ("Choose one", kicker …) have their own cancel button; the phase button is only hidden during covering stages |
| Button "»»" (end turn) | only in your own turn: pass automatically until the end of the turn, continue without attacking. Blockers, choices and targets are still asked; if another player puts something on the stack, automatic passing ends so you can respond |
| Choice window (centre of the screen) | appears for every general Forge decision that has no 3D stage. The card concerned is marked in gold. **One option:** click the button. **Several:** select/deselect buttons (golden = chosen), then "Done" – only active with a valid number. **Number:** number field, "OK". "Cancel" only where Forge allows it. Colour options are tinted in the mana colour |
| **Click vs. drag** | A short click (the mouse moves less than 6 pixels) = click; only when it moves is the card dragged |
| Clicking a glowing card on the battlefield | activate an ability (e.g. equip). See "Ability of a card on the table" |
| **Choose one** (several ways to play) | If a card has several ways to enter play or be used (cast, cycling, adventure, other face, alternative costs, abilities …), a 3D stage appears when you drag it in or click it: table dimmed, title "Choose one", one copy of the card per possibility, glowing blue, with a label above it (e.g. "Cast", "Cycling {2}"); hover enlarges. A click chooses, the orange "Cancel" button closes the stage (the card stays in the hand). "Cast" / "Play land" shows the whole card, an ability a copy with only its text (on the real card image on a light panel above the text box); with up to three possibilities they are spread further apart |
| **Selection buttons** | Buttons that confirm a selection ("Done", "Confirm targets") carry **no counter** like "(2/3)": how many are chosen is shown by the marked cards or buttons; the button only becomes active once the number is valid. Exception: the combat buttons name what they declare ("2 Attackers »", "1 Blocker »"; see "Attacking") |
| **Choose a mode** ("Choose one —", Confluence …) | 3D stage: one copy of the card per mode, showing **only that mode** (label "Mode 1", "Mode 2" …). One mode: a click chooses immediately. Several: a click chooses (orange) or deselects, "Done"; repeatable modes count up per click (plaque "×2"), "Reset" clears the choice |
| **Ability of a card on the table** | A click no longer activates an ability right away (otherwise costs like discarding or tapping would already be paid on click): even with only one ability, "Choose one" opens with the copy of the ability and "Cancel". Mana abilities (lands) stay a single click. One of your own untapped **creatures in main phase 1 or at the beginning of combat** additionally gets the whole card with "**Attack**" there: the UI then passes until attackers are declared and enters the creature as an attacker (confirm with "1 Attacker »") |
| **Spree / Tiered** (modes with their own additional costs, CR 702.172) | Like "Choose a mode", but: title = keyword ("Spree"), below it "Choose one or more additional costs." (Tiered: "Choose one of the additional costs."), above each mode copy its cost **"{3}"** (as mana symbols, without a plus sign; like on every stage with captions above its cards – "Mode 1", costs – the row of copies sits a bit lower so the captions keep clear of the subtitle, `Showcase::show_options`). A click chooses (orange) or deselects, for Tiered it swaps the choice. At the bottom orange **"Pay {total cost}"** – base cost plus chosen mode costs, recalculated on every click, only active with a valid choice – and "Cancel" where Forge allows it. Then payment proceeds as usual |
| **Spell being cast at the stack during mode selection** | For every mode choice for a card that is not on the battlefield (it is being cast; CR 601.2a: first onto the stack, then modes), the **whole card stands bright at the stack position** on the right of the stage; the row of choices moves to the left. Meanwhile the original in the hand is hidden. After the choice the spell lies on top of the stack pile, older entries peek out behind it |
| **Choose a replacement effect** (several replacement effects on the same event, CR 616.1; generally every choice where each possibility belongs to a card) | Like the Spree stage: title "Choose replacement effect", below it "Which replacement effect applies first?", one copy of its source card per possibility with only the text of that effect (with up to three possibilities further apart, almost flat). A click chooses (orange) or deselects, orange **"Apply"** only active with a choice. If several effects belong to the same card, "Effect n" is shown above. No 2D window |
| **Card options** (static ability first, card face, spell/ability for an effect, way to play when an effect lets you play the card, several spells/abilities) | The same stage as for replacement effects: one copy of its card per possibility with only its text (for card faces the whole face with name, type, cost, text and values). Title per question: "Choose static ability", "Choose face", "Choose one" ("How should the card be played?"), "Choose n"; below it Forge's question text. One choice: "Apply"; several: a click chooses or deselects, "Done" only active with a valid number; "Cancel" where Forge allows it |
| **From the opening hand** (Leylines etc., CR 113.6b) | card options stage with the whole cards: title "From the opening hand", a click chooses or deselects, "Done" – also possible without a choice |
| **Choose a pile** (Fact or Fiction …) | stage "Choose pile": one card fan per pile side by side with "Pile n (k)" above it, face-up cards with image, face-down ones as card backs. Clicking a card chooses its whole pile (all cards orange), "Apply" confirms (`ForgeTable::show_piles`) |
| **Choose cards on the table** (sacrifice, tap, convoke, improvise, crew …) | No window: clicking the glowing cards chooses or deselects them (if exactly one is required, immediately); the phase button counts along ("Done »") and confirms, Forge's question is shown as a hint above it; with too few cards "Cancel" where allowed. If a player must be chosen ("Choose a player"), the medallions pulse as for targets |
| **Dungeons (as in Arena)** | Venture (CR 701.49): if a dungeon must be chosen, all dungeons appear large side by side in front of the dimmed table (blue outline, golden on hover), a click chooses. For the next room the dungeon is shown large; as with planeswalkers, the reachable rooms are **fields on the card** (pulsing blue outline and slightly filled, golden under the mouse), clicking a room chooses it. The card is the **real card image** (Scryfall, the dungeon's token printing); the current room is outlined in gold with a game piece. Where the rooms lie on the image is detected by `Dungeon::room_rects` (`game/dungeon.rs`) from the dark walls of the image (horizontal walls = levels, vertical = rooms; rooms spanning several levels such as Oubliette are detected) and matched against the engine's room structure; if that does not match or the image is missing (or "own graphics only"), the card is drawn by the game (stone map, rooms in levels, paths). The dungeon lies at the end of the battlefield's back row; clicking it shows it large (a click closes). During the view the 2D displays are hidden |
| **Planeswalkers (as in Arena)** | Planeswalkers lie on the far left of the creature row. On the battlefield, instead of the value box they carry a **loyalty shield** at the bottom centre of the tile (black, white border, white number; `Card3D`, `cost_symbols::loyalty_shield`); when the loyalty changes, it briefly pops up and flashes. Clicking your own planeswalker opens the **planeswalker view** (`LoyaltyView`, `table/loyalty_view.rs`): the card large on the left of the screen, the table dimmed, its text box split into one field per ability that you click **on the card** – on the left edge of the card the cost symbol (+N light shield pointing up, −N dark pointing down, 0 grey), next to it the text with mana symbols (the font adapts to the text length). Abilities that cannot be activated right now (too little loyalty, already activated one, wrong timing, no valid target) are grey. Hover highlights a field in gold, a click activates it (targets, costs etc. afterwards as usual), clicking beside the card closes the view. Other abilities of the planeswalker are shown as fields without a symbol below |
| **Name a card, choose a type (as in Arena)** | `NameCard` (CR 201.3) and `ChooseType` (e.g. creature type for Cavern of Souls, CR 205.3): stage with the heading "(Source) Choose a creature type." / "(Source) Name a card.", below it a **list with filter field** (`table/search_list.rs`, `SearchList`): dark rounded field with "Filter …" and a magnifier, a list of seven rows with scroll bar (mouse wheel, click into the bar). When naming a card the list always shows **all card names** alphabetically (no preselection from your own deck – you usually name an opponent's card, e.g. with Pithing Needle). For types the list first shows the engine's **suggestions** (types on your own cards); **"Show all"** switches to all options ("Suggestions only" back). Just start typing: the list then searches all options (prefix first, then substring; for types suggestions first); backspace deletes, arrow keys move the selection. A click selects a row (golden), **"Done"** (or a second click on the row, or Enter) confirms; Escape / "Cancel" cancels where allowed. When naming a card, the card of the row under the mouse (otherwise of the selected row) is shown large next to the list with its real image (data via `card_info`); clicking it names it (`ForgeTable::show_search`) |
| **Divide (as in Arena)** | Dividing damage or counters while casting (`DivideAmount`, CR 601.2d; also prevention shields, CR 615.7): at each target on the table – the card, or to the right of a player's medallion – "−", the assigned amount and "+" appear (clicking the number = +1; if targets lie close together, the rows move above each other). The start is the minimum amount per target; the phase button shows "Divide N more …" or "Done »". **Combat damage** (`AssignCombatDamage`, CR 510.1c): stage "Assign combat damage" with the blockers large side by side ("lethal: N" above), the buttons below them, a trample target to the right with its name; lethal damage is prefilled in order, "Done" only once everything is assigned (`ForgeTable::show_divide`) |
| **Long questions (stage titles)** | As in Arena, a stage title stays short: if it does not fit the width (`Showcase::set_texts`, `split_title`), it is split at the first ": " (head = title), otherwise after the first sentence, otherwise after the words that fit with "…"; the rest is shown smaller in the subtitle, which wraps into lines and shrinks for long text. If a card is in focus large (yes/no, numbers, X), the details are shown to the right of it (with mana symbols as a wrapped text image) On stages with one large card the title sits above the card next to "View battlefield": it may be at most 2.3 units wide, shrinks first (down to font size 52), only then is split, and a plain question that still does not fit wraps onto two lines. "… unless you pay" asks briefly "Prevent the effect?", the cost is on the orange button ("Pay {2}") |
| **Button column** (three or more possibilities as text: counter type, keyword, protection type, number from a list, vote) | Title per question ("Choose counter", "Choose ability", "Choose protection", "Choose number", "Vote"), below it Forge's question text. If there is a source card, it stands large on the left, the buttons to its right one above the other (first one orange, the others blue; tighter when there are many, two columns from eight), otherwise centred. Counter types carry their coin (+1/+1 green, −1/−1 red, otherwise golden with initial). Protection from a colour: the **colour wheel** with the colours to choose instead. "Cancel" in grey below, where allowed (`ForgeTable::show_option_buttons`) |
| **Yes/No** (optional ability, "… unless you pay", Phyrexian mana, either-or) | The card concerned large and bright in the centre, the question above it (no ability text beside the card – it is on the card already; if the card lies on the stack, its **card name** stands as the heading above it; Phyrexian mana asks briefly "{W/P} pay with 2 life?"), below it **two buttons one above the other**: orange at the top (first answer, e.g. "Yes", "Pay"), blue below. If the card is on the stack (a spell being cast, e.g. for kicker or casualty), the details (card name) stand as a heading **above the card on the stack** and follow it while it slides into place (`Showcase.set_title_card`); otherwise to the right of the large card |
| **Optional costs** (kicker, buyback, bargain, gift …, also keyword additional costs such as casualty, conspire, offspring) | The card large and bright in the foreground, above it "Kicker" (or "Casualty" …), below it orange **"Kicker – {R}"** (or "Casualty" without a number, "Offspring {2}"; costs other than mana are explained by the card text) and grey **"Normal"**; sacrificing/tapping afterwards via card selection; with several optional costs one toggle button (✓) per cost and "Done". Blue **"Cancel"** (or Escape) takes the spell back to the hand – also for casualty/offspring, where the card is already on the stack |
| **Choose a target** | No window: as soon as a card or ability needs a target, a golden **arrow** hangs from it (the card already lies on the stack) and follows the mouse. Valid targets glow (cards) or get a pulsing golden ring (player medallions); moving over one snaps the arrow there. **Clicking** (or dragging the arrow from the card and releasing it) chooses the target – depending on the card a creature, a player or something else. If there is exactly one possible target, it is taken without a click. This applies to every card that targets (CR 115), even if only players qualify (e.g. Lightning Bolt with no creatures around): no prompt, only the arrow. The arrow has the colour of the card. With only one target it continues immediately; with several, clicking again = deselect, then the phase button "Confirm targets »". As long as too few targets are chosen, the button reads "Cancel" and takes the spell back. What the card requires as a target is shown in gold above the phase timeline |
| **Choose X** (X costs) | At the stack on the right (without dimming, `Showcase.open_at`): the card **large, bright and readable** (at the stack twice the usual card size, in front of the camera 1.2 times – the same size on screen; `Showcase.set_focus_layout_sized`; no "View battlefield" button, the table stays visible; glowing orange; the original card on the stack behind it is hidden meanwhile and only reappears once the X stage has disappeared), above it "Choose a value for X.", below it the button row **−5**, **−**, orange **"Pay X = n"**, **+**, **+5** (minus only active from 1) and grey "Cancel" (takes the spell back). "Pay" sends the value to Forge, then payment proceeds as usual |
| **Choose a number** (`ChooseNumber`: "choose a number", "how many counters", "reduce by how much" …) | The same number stage as for X: the card large and readable, the question text above it (the engine's generic "Choose a number" becomes "Choose a number from 0 to 13."), below it **−5**, **−**, orange **"Confirm: n"**, **+**, **+5**; the start is the smallest allowed value, "−"/"+" only active where the value can change; grey "Cancel" only where Forge allows it. If the card lies on the stack, the stage stands there (moved left far enough that title and buttons stay fully on screen; without dimming, the phase button shows "Make a choice …"), otherwise in front of the camera. No 2D window |
| **Choose a player** (choice for an effect **without a target** where only players can be chosen and exactly one is chosen, e.g. "choose a player", CR 115.1) | Like the starting player choice, without a card: over the table one **orange** button with their name per opponent, below it the **blue** button "Myself"; if you may choose none, finally grey "No player". Above it "(card) Choose a player.". Clicking the medallion and the arrow still work; cancel via the phase button or the ✕ at the card. Real targets (CR 115: "target player", "any target" as with Lightning Bolt) **never** get buttons, even if only players qualify – there you only drag the arrow (see "Choose a target") |
| **Paying mana** | **Always automatic:** when you drag a card onto the table (or activate an ability), Forge pays the costs itself right away – no window, also not with mana tapped in advance (the floating mana in the pool is used as well). **Tapping in advance:** you can click lands and other mana sources at any time while you have priority (CR 605.3a; they do not glow for this), the mana then floats in the mana pool. **Untapping again:** clicking a land tapped in advance whose mana has not been spent yet undoes the mana ability (Forge's undo for mana abilities, action `UndoMana`) – the land stands up again, the mana disappears from the pool |
| **Choose cards from the hand** (discard, also as a cost or because of hand size; to the bottom of the library after a mulligan; splice; other choices only from the hand) | 3D stage like the mulligan (question text "Title: rest" becomes title and subtitle, e.g. "Splice"): the selectable hand cards large side by side **on a straight line** (no fan arc, not turned; `Showcase::enable_line`) in front of the camera (glowing blue); if they do not all fit on screen, move them left and right with the **slider** below (`Slider3D`: drag the orange handle or click the rail). Title e.g. "Discard 1 card(s)". A click chooses (orange), clicking again deselects; if only one card is allowed, a click swaps the choice. Orange "Done" only active with a valid number, blue "Cancel" only where Forge allows it. No 2D window |
| **Scry / surveil** | 3D stage: title "Scry" (or "Surveil") with instructions, a dark box, below it on the left **BOTTOM** (or **GRAVEYARD**) and on the right **TOP**. Your library lies in the box as a **face-down pile**; only the revealed cards are visible. They first lie on the right **on top of** the pile (the rightmost goes on top). Dragging one to the left (or clicking it) slides it **under** the pile so it peeks out on the left; for surveil it lies to the left of the pile (graveyard). Another click brings it back to the top. "View battlefield" at the top right. Orange "Done": the cards fly face down to the library pile on the table (for surveil only into the graveyard if none stays on top) |
| **Choose cards from hidden zones** (search the library, revealed cards, graveyard …) | 3D stage: when searching the library, the own library pile flashes first. Table dimmed (table cards behind it do not react to the mouse; while a dimmed stage is open, medallions, names, phase timeline and target arrows are hidden), title ("Choose up to one card" …) with source and text, the cards large as a **cover flow** (`Showcase.enable_coverflow`, `Zone.coverflow`): the middle ones straight and in front, the others turned to the side, offset backwards and overlapping, densely stacked at the edge. The card under the mouse comes forward large and straight. With many cards the fan runs through the centre with the **slider** below. **"View battlefield"** at the top right hides the stage together with all cards on it – also cards lent from the table such as the opening hand at the mulligan (table visible and hoverable), "Back to selection" brings it back – every stage in front of the camera that dims the table has this button (mulligan, choices, scry, name a card …; `Showcase::open`). One card: a click chooses immediately; several: a click chooses (orange), "Done"; if you may take none, blue **"Decline"** is shown at the bottom right. View only: "Done". The chosen card then slides from the library pile to its place |
| **Choose an order** | Simultaneous own triggers (CR 603.3b; or cards that change zones at the same time, costs): 3D stage "Set order" like "Choose one" – one card per entry side by side (for triggers a copy of the triggering card that shows only the text of this ability), with a slider for many cards. A click assigns the next number (**1**, **2**, … large and bold in the middle of the card, outlined in orange; short number plaques like "×2" use the same style, `style_badge` in `table/card3d.rs`); clicking again takes the number back, the others move up. "Reset" clears all numbers, orange "Done" is active as soon as all are numbered (number 1 goes first) |
| **Choose costs** | If a card must be sacrificed, discarded, tapped, exiled or returned to the hand as a cost (e.g. "Sacrifice a creature:"), the card selection appears with all matching cards. If there are exactly as many matching cards as needed, there is no question |
| **Phyrexian mana** | Before paying, per symbol the question "pay 2 life?" ("Pay 2 life" / "Pay with mana") |
| **"…, unless you pay"** | If you can pay the cost, the game asks "Pay" / "Don't pay"; payment works as when casting. If you cannot pay, there is no question |
| **Targets of triggered abilities** | Own triggers with a target (e.g. "When … enters, it deals 4 damage to target creature") ask for the target as when casting (click on card/medallion) |
| **Revealed cards** | If a player reveals cards (e.g. a searched card from the library), they appear in the card fan with the heading "Revealed – …"; "Done" closes the display. Messages like "chooses red" are written to the game log |
| **Copies, redirecting, exert, enlist** | If an effect copies a spell and allows new targets (e.g. Reverberate), the game asks "Choose new targets for …?"; on "Yes" click a target as when casting. Counterspells and copy effects target the spell on the stack (click the card on the stack). When attacking, the game asks for attackers to exert or enlist: click them on the table, "Done" |
| **Convoke, improvise, delve** | When casting such spells the game asks before paying: untapped creatures (convoke) or artifacts (improvise) glow on the table, each chosen one pays one mana (matching colour first); for delve the graveyard cards appear in the card fan, each exiled card saves {1}. "Done" with 0 cards = pay normally |
| **Attacking** (declare attackers step) | **Combat view:** creatures that can attack glow blue. Click (or drag) = attack: the glow turns orange, the card moves towards the opponent and carries a red **attack spike** at its top edge; clicking again = don't attack. **Arrow only with several attack targets:** if the opponent has a planeswalker (or a battle), an orange arrow shows whom the attacker attacks – drag the arrow and release it on the medallion or planeswalker, or click the target afterwards. Without planeswalkers there is no arrow. **Buttons at the bottom right** (two stacked, the phase timeline moves up above them): as long as no attacker is chosen, the blue **"No Attacks"** sits above the orange **"All Attack"**. "No Attacks" continues without an attack; "All Attack" (also space bar) only *chooses* every creature that can attack – nothing is declared yet. As soon as at least one attacker is chosen (by "All Attack" or by clicking), the top button becomes the grey **"Cancel Attacks"** (deselects all, back to "No Attacks" / "All Attack") and the bottom one **"2 Attackers »"** (with the number of chosen attackers), which declares the attack. Invalid attacks (e.g. a creature must attack) are asked again with a hint |
| **Blocking** (declare blockers step) | Your creatures that can block glow. **Drag an arrow** from your creature to an attacker = block (into empty space = remove the block). Alternatively click one of them (golden mark): an arrow then hangs on it and follows the mouse, snapping onto an attacker under the mouse (the card preview stays hidden meanwhile, as while dragging, so it does not cover the arrow); click an attacker = block (orange arrow). On assignment the blocker's frame fades from blue to orange and flashes, and a **flickering trail** of light rises as an arc above its edge facing the attacker (`Card3D::set_block_trail`). **Once the blocks are declared** (confirmed by you or chosen by the AI), the attacker and its blocker **turn yellow** – frame, attack spike, trail (calmer) and arrow – until combat ends (`Card3D::set_block_declared`); **each blocked attacker and each blocker gets its own pile** (identical creatures are no longer stacked with them, `Card3D::set_combat_alone`), also for the AI's blocks. **A blocked attacker lies directly opposite its blocker** (with several blockers: opposite their middle; `Card3D::set_combat_x`), the other attackers right beside it; it does not move forward, so the cards keep the normal gap between the rows and do not overlap. The two creature rows lie about half a card apart (`FRONT_GAP` in `table/zone.rs`); other attackers move forward by at most a third of that gap. Combat arrows are always straight short arrows between the cards (never the stack arc). Clicking a blocker = remove the block. Several blockers on the same attacker are possible. Confirm with the phase button: without blockers it reads **"No Blocks »"**, otherwise **"1 Blocker »"** / **"2 Blockers »"**; as soon as a blocker is chosen, the grey **"Cancel Blocks"** above it removes all planned blocks. Invalid blocks are asked again with Forge's reason |
| **Counters, tokens, attached cards, exile** | Counters are shown as **coins** at the top right of the card edge (sticking out halfway, one per counter type, one below the other): silver with the count, golden rim (+1/+1 green, −1/−1 red); when a counter is added, the coin briefly pops up and glows blue (`Card3D::set_counters`, `cost_symbols::counter_coin`). The card view on hover shows the counter's name. Loyalty and defense are shown in the value box. Also as a coin: **class level** ("L2", CR 716), for **rooms** the unlocked doors ("1/2", CR 709.5; the names of the unlocked doors as a plaque), **prepared** creatures (Prepare) as a blue coin "V" – their spell then lies as a copy to the right of the hand with the plaque "prepared" and the spell's cost. Tokens lie on the table as the same **tiles** as other cards (image field cut out of their Scryfall image, since tokens have no separate image crop; `placeholder::tile_face`), without a plaque on the image; "Token" is shown in the card view on hover. A **modal spell on the stack** shows only the chosen modes (on the real image on the light panel). **Removing a counter as a cost** (Power Conduit etc.): click the permanent on the table, with several counter types a "Choose counter" button column with coins. **Multikicker/replicate/squad**: the number stage offers at most as many as the available mana allows (the engine's estimate, at most 20). Auras and equipment lie slightly offset under their host (towards the table centre and to the right, their name bar visible) and move with it; the creature row moves back by their overhang so that **no card crosses the middle line** to the other side, also when attacking (`ATTACH_*` in `zone.rs`). Exiled cards lie face up on the exile pile next to the graveyard. Player counters (poison, energy, experience …) lie as plaques next to the medallion (see "Player designations") |
| **Command zone, double-faced and face-down cards** | The command zone has no area of its own: commanders lie in the hand fan (see above); emblems, dungeons (plaque "Room: …"), The Ring, The Monarch, The Initiative and other effects lie at the right end of the side's back row on the battlefield. Double-faced cards visibly flip over when transforming (0.35 s, 180° around the long axis, `Card3D::flip_to`); the other face is shown as a plaque "↺ Name". Face-down cards (morph, manifest) show the card-back pattern with their values (usually 2/2). Day ☀ / night ☾ is shown next to the turn number |
| **Player designations (as in Arena)** | Flat on the table **in one row at the height of the life plate** (below the artifact row): **poison** to the left of the medallion, everything else to its right – first the other **player counters** as dark **plaques** with a symbol on the left and the count on the right – poison (green Φ), energy (orange bolt), experience (purple star), others with their initial –, then the **designations** as free-standing, golden glowing **emblems**: monarch (crown with a red stone, CR 724), initiative (red shield with a sword, CR 725), city's blessing (golden city in a halo, CR 702.131), the Ring (golden band with a glowing inscription, number = how often it has tempted, CR 701.54). While a stage covers the table they are hidden like the medallions. New or changed ones briefly pop up (`ForgeTable::sync_seat_tokens`, row `SEAT_ROW_*`, drawing `cost_symbols::seat_token`, classification `cost_symbols::is_seat_designation`). All motifs are self-drawn |
| **Stack** | At the right edge of the screen as a **pile of cards**, large and tilted towards the camera (zone `Stack`, layout `ZoneKind::Stack`): spells slide there when cast (already while targets and costs are chosen, entry "being cast"); activated and triggered abilities appear as the card of their source. The newest entry lies in front and resolves first; each older one lies a bit further up and to the left, slightly turned, behind it. **Arrows explain what responds to what:** each entry with targets gets arrows in the colour of its card (blue, red, …; multicoloured gold, colourless grey) to its targets; if it targets an entry below it (e.g. a counterspell), the arrow becomes an arc that rises from the front card and lands on the card behind it. New arrows grow out in 0.33 s |
| The AI's combat | The AI's attackers also move forward and carry the attack spike (on the edge facing us); the opponent's cards are shown turned by 180° so they are readable from our side (`Zone.turn_cards`, `Card3D::set_turned` – only the card image turns); arrows only for attacks on planeswalkers/battles (orange), declared blocks yellow (arrow, frames, trail) |
| **Combat damage** | Every creature that deals combat damage **strikes**: in 0.3 s it lunges at its target (the player's medallion, a planeswalker or creature), a red impact from the mana glow assets (flash, star, shock wave, sparks), then back. An attacker **runs into its blocker**: it lifts off, tilts slightly and lands on top of it; the blocker only braces briefly against it. Both stay in place until the hit has landed (`Card3D::hold`), only then do dead creatures move to the graveyard |
| **Card glow** | frame glow from `Assets/Mana Glow Effekte` on every card: **blue** = playable or selectable (default), **orange** = activated – the card is being dragged, cast or its ability activated, chosen as attacker/blocker/target, land tapped for mana; **yellow** = attacker or blocker in a declared block. On activation the frame briefly flashes (`Card3D.flash`) |
| **AI pace** | The Forge AI decides instantly; so you can watch its turn, the bridge waits briefly after each of its actions (land 0.7 s, spell/ability 1 s, attackers 0.9 s, blockers 0.7 s, each phase change in its turn 0.25 s; an AI turn thus takes about 3–4 s). Adjustable via `ForgeBridge.ai_pace` (factor, 0 = no pauses) |
| **Paying mana (effect)** | When a land (or a mana source) is tapped for mana – by you or the AI – the effect from `Assets/Mana Glow Effekte` plays (`table/fx.rs`, `ManaFx`): the land flashes, a rune circle and a mana orb appear, a comet with a tail flies in an arc to the paid card on the stack, impact with star, shock wave and sparks, the card's frame lights up. Colour = the mana produced (white, blue, black, red, green, colourless golden). Without a spell (mana tapped in advance) it flies to the mana pool display (`ManaAnchor`; for the AI without an anchor to the medallion) |
| **Arrows** | Recreation of the arrow from `Assets/Pfeil/MTG_Pfeil.blend` (`target_arrows.rs`): glowing band with a bright core, a head and light waves running towards the head; colours from the palette of the Blender material (red/orange = attack, orange = planned block, yellow = declared block, gold = targeting, violet = targets on the stack). New arrows grow from start to target in 0.33 s. Always straight: when one end moves (e.g. while dragging or aiming with the mouse), the arrow follows without bending or wobbling |
| Right-click on a card | camera close-up; again or Esc = overview |

**Card images (JPG):** like Forge and NeoForge, the cards show real card images from Scryfall.
The bridge looks for them in Forge's image folder (`~/.cache/forge/pics/cards/`) and in
NeoForge's (`~/Games/NeoForge/datos/cache/pics/cards/`, a different location via the environment
variable `CLASH_EXTRA_PICS`) and downloads missing ones from Scryfall into Forge's image folder in
the background (in the first game with a deck the images appear one by one). With an image the
card hides name, cost, type line and text (they are in the image); power/toughness only if they
differ from the printed values. A printing with a **normal frame** is always shown: if the
printing from the deck list is a special printing (Secret Lair, showcase, borderless, comic
editions …), the bridge takes the newest printing from a core set or expansion for the image
(otherwise reprint, Commander or draft sets; never "The List") – only the image, the card in the
game stays unchanged. That way name, cost and text box are in the same place on almost all cards.
Stage copies with their own text (one mode, one replacement effect) show the real image with a
light panel over the printed text box that contains only that text (font up to 56 points). If a
card has no normal-frame printing at all, the bridge additionally sends the image crop (`art`,
Scryfall `art_crop`); the stage copy then gets the drawn frame with this crop in the image field
and its own text in the text box. Without an image, for face-down cards and tokens the drawn
placeholder stays (`placeholder::card_face`). If an image only finishes while waiting, the bridge
sends a new state afterwards so it appears immediately. The images belong to Wizards of the Coast
and never go into Git.

**Placeholder card image:** name, cost symbols, type line and rules text come from the engine.
Symbols in the rules text are replaced (`{T}` → ↷, `{R}` → Ⓡ, `{2}` → ②, hybrid as "(R/G)"), the
font size of name, type line and rules text adapts to the text length (`card_text::fit_line` /
`fit_font`). Value box at the bottom right: creatures power/toughness (green = above the printed
value, red = below or damaged), planeswalkers loyalty, battles defense. Frame colour = card colour
(gold = multicoloured, grey = colourless, lands by their mana colour); type symbol in the image
field per card type (planeswalker = crown, battle = shield).

**Tapped** cards are tilted slightly (15°, `zone::TAP_ANGLE_DEGREES`), greyed out and show a grey
tap symbol in the centre (`textures::tap_icon()`).

**Spells and abilities with targets:** after a card is played, the game asks one after the other
for everything the spell needs: optional costs (e.g. kicker), mode, X value and the **target**
(click on a card or medallion, see above). Activated abilities of permanents (e.g. equip) glow
when they are possible and are activated by click.

The player makes every decision within scope (see `docs/Kartenabdeckung.md`) themselves; the
Forge AI only steps in when an answer is invalid. **Mana from the pool:** floating mana (e.g. from
rituals or lands tapped in advance) is used by the automatic payment (see "Paying mana").
**Modes with repetition** ("you may choose the same mode more than once"): every click counts the
mode again ("2× …"); with a full selection a click removes it again. Crewing vehicles: click
creatures on the table until the required power is reached, then "Done".

### Debug window (debug builds only)

When the game is started from the editor (or a debug export; `OS.is_debug_build()`), the
`ForgeTable` creates a **debug window** (`DebugPanel`, `game/debug_panel.rs`). It does not exist
in release exports.

| Control | Effect |
|---|---|
| **F10** | show/hide the window (on first opening it loads all card names from Forge) |
| Search field + list | filter card names (prefix first, then substring; at most 200 hits) |
| "To my hand" / double-click, "To the AI's hand" | the card is created anew and put into the hand (`debug_add_card`); an open priority is asked again, the card is playable right away |
| "Infinite mana: me" / "… AI" | Forge refills the mana pool to 50 per mana type on every phase change and spell (`debug_infinite_mana`) |

---

## 4. Rules

All rules (turn structure, timing, mana, stack, combat, state-based actions) come from **Forge**.
The game itself checks nothing: it shows Forge's game state and sends your decisions (priority,
mulligan, choices) to Forge.

---

## 5. Forge rules engine (Java)

**The goal is NeoForge** (JavaFX front end for Forge): there, the 3D table is to play the
matches NeoForge starts. The UI speaks the protocol model (`clash-protocol`, modelled on
forge-rules-rs); the Java bridge translates Forge into it (protocol v3). Forge runs as a separate
Java process (`forge-bridge/`, GPL-3.0). The Godot node `ForgeBridge` (`rust/src/forge/bridge.rs`,
transport from `clash-forge-client`) starts it and exchanges JSON lines over stdin/stdout. How
other projects use these building blocks: [Embedding.md](Embedding.md). The player's decisions
(priority, mulligan, general choices) arrive as the signal `decision_requested` and are answered
with `answer()`; all Forge game events arrive as `forge_event`.

Setup, protocol, GDScript example and limits: [Forge-Protocol.md](Forge-Protocol.md).
Forge files: if a copy (jar + `res/`) lies in `forge-bridge/forge/` (not in Git), it is used
before `~/Games/Forge`.

**`ForgeTable` (`rust/src/forge/table.rs`)**

- Starts the bridge and the match automatically (`autostart`): `player_deck` against
  `opponent_deck` (default: mixed test decks `forge-bridge/testdecks/spieler.dck` – red with
  instants, sorceries, X spells, modal spells, kicker, equipment – against `gegner.dck` – green
  with auras, combat, ramp). Forge seat 0 = player at the bottom (you), seat 1 = opponent at the
  top (Forge AI).
- Rebuilds the table on every new Forge state: one `Card3D` per Forge card (image from name,
  type, cost, power/toughness); hand → hand, lands → land row, other permanents → battlefield,
  graveyard → discard pile, exile → exile pile, opponent's hand face down; tapped state is taken
  over. Attached cards (`attached_to`: auras, equipment) lie offset under their host in its zone
  (`Zone.set_attached`), on the opponent's side too. Counters as coins at the top right
  (`Card3D::set_counters`), "token" as a plaque above the image field (`Card3D::set_badges`,
  texts from `game/counters.rs`); stack entries → zone `stack_zone` (spells as their card,
  abilities as the card of their source, which appears on the source). Cards slide into place
  animated. **Deck:** the library lies as a face-down pile in the deck zone (count = Forge's
  `library`). When Forge draws a card, the top deck card slides into the hand and is turned face
  up there (it stays face down for the opponent). When the opponent plays a card, it slides from
  their face-down hand onto the table. While Forge sends events, the state is queried again in
  batches every `refresh_interval` seconds.
- Provides the HUD with phase, life, names, turn, winner, button and choice window (`hud_view()`).

---

## 5a. Playing in NeoForge (3D table)

NeoForge (JavaFX front end for Forge) can play its 1 vs 1 matches on this 3D table: menus, decks
and modes stay in NeoForge, the game table is Godot. Technically: NeoForge opens a local port,
starts Godot with `-- --clash-port=N`, and the human seat is driven through the bridge (protocol
v3) (details: `docs/Forge-Protocol.md`, section "Embedded in NeoForge").

**Only 1 vs 1:** the 3D table takes matches with exactly two players (you and one opponent,
Commander 1 vs 1 too, and Adventure duels against a single enemy – they start in
`adventure/NeoDuelBridge` instead of `NeoGame`, `ClashTable.openForDuel`). Matches with more players, two people on one computer (hot seat),
Planechase, Archenemy and Vanguard stay on NeoForge's own table (`ClashTable.useFor`).

**Building and starting:** `tools/neoforge_build.sh` builds NeoForge from source with the 3D table
(NeoForge repository + Forge checkout as in NeoForge's `BUILDING.md`, Maven, Java 17+) and bundles
the exported game table (`tools/export_table.sh` → `export/linux/`, copied to
`forge-gui-neo/target/clash3d/`; NeoForge starts it instead of Godot plus the project folder),
`tools/neoforge_play.sh` starts it with the paths to Godot and this project already set,
`tools/neoforge_menu.sh` puts it into the application menu. Details:
`integrations/neoforge/README.md`.

**Enabling:** in NeoForge under *Settings → Table → 3D table (Neo Forge 3D)*: "Play 1 vs 1 games
on the 3D table" = yes; "Godot program" (path to the Godot 4.7 binary) and "Neo Forge 3D project
folder" (the `godot/` folder of this project) if the start script does not set them. Order: system
property (`-Dneo.clash=true -Dneo.clash.godot=… -Dneo.clash.project=…`, this start only) →
NeoForge setting → environment variable (`NEOFORGE_CLASH`, `CLASH_GODOT`, `CLASH_PROJECT`). If
Godot does not start within 2 minutes, NeoForge plays with its own table as usual. While the match
runs, NeoForge's window says "The game is running in the 3D table window."

**Flow:** start a match in NeoForge → the Godot window opens with the match → the end of the game
is shown for 4 s, then Godot closes and NeoForge shows the result. If the Godot window is closed
in the middle of the match, the player's seat concedes.

**Developing and testing without Maven:** NeoForge's sources only build in a Forge checkout with
Maven (`tools/neoforge_build.sh`). For the integration an installed NeoForge (`~/Games/NeoForge`,
the itch.io build) is enough:
- The NeoForge integration lives separately in `integrations/neoforge/` (`ClashTable.java` and
  `neoforge-host.patch` for NeoForge's classes, instructions in the README there); once
  `git -C <neoforge> apply integrations/neoforge/neoforge-host.patch`,
- `tools/sync_neoforge.sh` copies the bridge (without `app/`) and `ClashTable` into NeoForge,
- `tools/neoforge_patch.sh` compiles them together with the modified NeoForge classes
  (`ClashTable`, `NeoGame`, `NeoDuelBridge`, `SafeAi`, `NeoMain`, `SettingsPanel`, language files) into a patch jar,
- `tools/neoforge_export_patch.sh` regenerates `neoforge-host.patch` from the NeoForge repository,
- `tools/neoforge_clash.sh [--deck=NAME] [--timeout=SEC]` plays a Commander match with the 3D
  table in a test copy (your own NeoForge data stays untouched; command `NeoMain clash`).
  With `CLASH_CMD=ui tools/neoforge_clash.sh` NeoForge's normal menu starts instead (test copy,
  patch loaded, Godot paths already set); there you only need to enable the 3D table in the
  settings.

## 6. Language (German / English)

- **Switching** in code: `$Table.set_language("en")` / `$Table.set_language("de")`; query with
  `$Table.get_language()`. The default is German.
- **Switched immediately:** HUD, phases and card labels.
- **Card names** come from Forge (English).
- **Documentation:** this file and the developer docs in `docs/` are English; the Godot help
  (`///` doc comments) and code comments are German.

### Adding new texts (`rust/src/i18n.rs`)

| Case | Notation |
|---|---|
| fixed text | `tr!("Abbrechen", "Cancel")` |
| text with values | `trf!("{n} Karten", "{n} cards")` or `trf!("{} zieht", "{} draws", name)` |

---

## 7. Custom scripts (GDScript) – example

```gdscript
@onready var forge: ForgeBridge = $Table/ForgeBridge
@onready var ft: ForgeTable = $Table/ForgeTable

func _ready() -> void:
    forge.forge_event.connect(func(e): print(e.event, ": ", e.text))
    forge.game_over.connect(func(r): print(r.winner, " wins"))

func status() -> void:
    print(ft.get_pending_kind())          # e.g. "Priority", "Mulligan", "ChooseTargets" or ""
    print(ft.get_state().get("phase"))    # e.g. "MAIN1"
```
