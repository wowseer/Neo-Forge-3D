# NeoForge integration

Lets NeoForge (JavaFX front end for Forge, github.com/AdrianLopez98/NeoForge) play its 1 vs 1
matches on the 3D table of Neo Forge 3D. Menus, decks and modes stay with NeoForge; for the human
seat NeoForge starts Godot as the game table and drives it through the Forge bridge (protocol v3,
local socket). A switch in NeoForge's settings turns the 3D table on and off.

The 3D table is used for **every two-player match** in every mode (you against one opponent:
Constructed, Commander 1 vs 1, Draft, Sealed, Tournament, Quest, Ascent, Rogue and Adventure duels). Matches with more players, two people on one computer (hot seat), Planechase, Archenemy and
Vanguard always use NeoForge's 2D table. Tutorial lessons and puzzles also stay on the 2D table
(the lesson guide lives in NeoForge's table; puzzles need a human-marked controller, which the
remote seat is not).

Everything here is kept separate from the bridge itself, so it can be handed to any NeoForge
repository.

## Contents

| File | What |
|---|---|
| `src/forge/neo/clash/ClashTable.java` | Wrapper for NeoForge: reads the settings (system property → NeoForge setting → environment variable), decides whether a match runs on the 3D table (`useFor`: switched on, 1 vs 1, no hot seat, no unsupported variant), starts the table via `clash.forgebridge.TableLauncher`, provides the remote-controlled player (`ClashBridge.remotePlayer`) |
| `neoforge-host.patch` | Changes to NeoForge's own classes against NeoForge upstream: `SettingsPanel` hides the Godot paths when a table is bundled, `NeoGame` (3D table for the human seat of a 1 vs 1 match, `NeoMatchUI` runs invisibly as a spectator, NeoForge's window says the match runs in the 3D table window), `adventure/NeoDuelBridge` (the same for Adventure duels: they are set up by Forge's Adventure and start there instead of in `NeoGame`; the human, last in the Adventure's seat list, moves to seat 0 – `ClashTable.openForDuel`), `SafeAi` (do not replace the remote seat with the AI fallback), `NeoMain` (test command `clash`), `SettingsPanel` (section "3D table (Neo Forge 3D)" at the top of the *Table* tab), `adventure/NeoWindow` (window handoff for Adventure duels), `quest/NeoQuestMatch` + `quest/QuestLowLife` (Quest duels: Forge's `QuestUtil` builds the seats, a `HostedMatch` subclass moves the player to seat 0 at the 3D table, keeps the low-life option, and gives the seat back to Forge's quest player before the rewards are computed – Forge finds the winner through that `LobbyPlayer`), language files. Made against NeoForge 1.0.66 (`v1.0.66`) |

The bridge itself (`clash.forgebridge` from `forge-bridge/src/`, without the program entry point
`app/`) is copied in by `tools/sync_neoforge.sh`.

## Building NeoForge with the 3D table

NeoForge builds as a module inside a Forge checkout (NeoForge's `BUILDING.md`: Forge at the
commit `FORGE_REF` from `.github/workflows/macos.yml`, `<module>forge-gui-neo</module>` added to
Forge's `pom.xml`). With that in place:

```sh
NEOFORGE=~/OProgrammieren/Java/NeoForge FORGE_SRC=~/OProgrammieren/Java/forge \
  tools/neoforge_build.sh     # patch (once), copy bridge + ClashTable, copy the module, Maven
tools/neoforge_play.sh        # start NeoForge (data in ~/.local/share/NeoForge)
tools/neoforge_menu.sh        # optional: "Neo Forge" in the application menu, .frp replays open with it
```

Do not use NeoForge's own `linux/add-to-menu.sh` for a source build: it belongs to the packaged
itch.io version and points the menu entry to `bin/NeoForge`, which does not exist in the source
tree (the entry then starts nothing). `neoforge_menu.sh` writes the same entry, started through
`neoforge_play.sh`.

`neoforge_build.sh` applies `neoforge-host.patch` if the NeoForge repository does not have it yet,
runs `tools/sync_neoforge.sh`, copies `forge-gui-neo` into the Forge checkout and builds with
`mvn package -pl forge-gui-neo -am` (Maven from the `PATH`, otherwise
`~/.local/opt/apache-maven-*/bin/mvn`, or `MVN=…`). Java 17 or newer. Finally it exports the game
table (`tools/export_table.sh`: Rust in release mode, Godot export "Linux" with the game data
embedded) and puts it into `forge-gui-neo/target/clash3d/` (`neo-forge-3d.x86_64` +
`libneo_forge_3d.so`). The export needs Godot's export templates of the editor's version
(`~/.local/share/godot/export_templates/<version>/`, *Editor → Manage Export Templates*).

**Bundled table:** NeoForge looks for `clash3d/` next to its classes (source build:
`target/classes` → `target/clash3d/`; package: next to the jar) and starts the exported table from
there – no Godot installation, no project folder (`ClashTable.bundledTable`). Only without it does
it fall back to the Godot program plus the project folder, which `neoforge_play.sh` passes as
defaults (`CLASH_GODOT`, `CLASH_PROJECT`); then the Godot extension has to be built
(`cd rust && cargo build`).

Installing by hand instead:

```sh
NEOFORGE=~/OProgrammieren/Java/NeoForge                              # NeoForge checkout
git -C "$NEOFORGE" apply integrations/neoforge/neoforge-host.patch   # once
NEOFORGE="$NEOFORGE" tools/sync_neoforge.sh    # copy bridge + ClashTable (after every change)
```

Without Maven, against an installed NeoForge: `tools/neoforge_patch.sh` → put
`forge-bridge/build/neoforge/neoforge-clash-patch.jar` in front of the NeoForge jars on the class
path (see `tools/neoforge_clash.sh`).

## Enabling

Settings have their own tab *Settings → 3D table*: "Game table" = **2D table** or **3D table**
(default: 3D table), and "Graphics quality" = **Low** (default: no SSAO/glow, 3D scene at 75 %
resolution, smooth on integrated GPUs) or **High** (all effects, full resolution). NeoForge passes
the quality to the table as `-- --graphics=low|high`. "Battlefield" (drop-down) picks the set
piece (`--board=`; random, one of the nine battlefields or the classic procedural one; default
arena). In Adventure duels the battlefield follows where you fight (`NeoDuelBridge.adventureBoard`):
arena tournaments and events → arena, towns → tavern, castles/capitals/boss places → castle,
otherwise the terrain (white → plains, green → forest, blue/ocean → islands, black → swamp,
red → mountains, wastes/deserts → wastes).

**Images:** at the start of a match the bridge sends `preload_images` (all card images and crops
of both decks) and the table decodes them right away. `tools/neoforge_images.sh` downloads all
image crops (tiles) and token images into NeoForge's picture folders in advance (NeoForge's own
"all printings" download only covers the full card images); existing files are skipped. With more than two players the 2D table is
always used, whatever is chosen there. Without a bundled table the section also shows "Godot program" (Godot 4.7) and
"Neo Forge 3D project folder" (`godot/` of Neo Forge 3D) for when the start script does not provide
them. Applies from the next match.

Each value is taken from the first source that has it:

| System property (this start only) | NeoForge setting | Environment variable (default) | Meaning |
|---|---|---|---|
| `neo.clash=false` | `clash3d` | `NEOFORGE_CLASH=false` | use the 3D table (default: yes) |
| `neo.clash.graphics=high` | `clash3d.graphics` | `CLASH_GRAPHICS=high` | graphics quality `low` (default) or `high` |
| `neo.clash.table` | `clash3d.table` | `CLASH_TABLE` | exported table to start (otherwise the bundled one in `clash3d/`) |
| `neo.clash.godot` | `clash3d.godot` | `CLASH_GODOT` | Godot executable (otherwise `godot`) |
| `neo.clash.project` | `clash3d.project` | `CLASH_PROJECT` | Godot project folder of Neo Forge 3D (only without an exported table) |

If the table does not connect within 2 minutes, NeoForge plays with its own table.

**Look from *Customise*:** your seat shows your face from *Customise* in its medallion and your
card sleeve on the backs of your cards (the deck's sleeve if it has one, as on NeoForge's own
table); the opponent gets the same face as on NeoForge's 2D table (*Customise → Rivals*, otherwise the
Forge avatar the engine assigns, so it is the same every match); in Adventure duels the enemy's own
sprite from the map instead (`NeoDuelBridge.adventureFace`: cut from the enemy's sprite atlas, scaled
up pixel-sharp, passed via `ClashTable.sendLook(players, opponentFace)`) and the sleeve NeoForge gives that
rival (*Customise → Rivals*, otherwise the engine's). `ClashTable.sendLook`, called by `NeoGame`
and `NeoDuelBridge` once the seats are set; images from Forge's sprite sheets are written as PNG to
`<tmp>/neoforge-clash-look/`.

**Window handoff:** the table opens where NeoForge's window is (`ClashTable.open(Stage)`, also
maximised or full screen) and NeoForge hides its window once the table is connected; after the
match NeoForge comes back where the table window was last (`ClashTable.close(Session, Stage)`,
from the table's `window` reports). Adventure duels go from the Adventure window straight to the
table and back (`NeoWindow.tableWindow()`, `hideAdventure()`, `giveBack(…, TableWindow)` in
`neoforge-host.patch`); NeoForge's 2D duel window is not shown.

## Updating the patch

When NeoForge's classes change in the repository (committed locally on a branch, never pushed),
regenerate the patch:

```sh
tools/neoforge_export_patch.sh      # BASE=origin/HEAD, NEOFORGE=… can be overridden
```

Check that it applies to an unmodified NeoForge:
`git -C <fresh NeoForge checkout> apply --check integrations/neoforge/neoforge-host.patch`.

## License

GPL-3.0, like NeoForge and Forge.
