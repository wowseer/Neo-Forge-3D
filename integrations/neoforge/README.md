# NeoForge integration

Lets NeoForge (JavaFX front end for Forge, github.com/AdrianLopez98/NeoForge) play its 1 vs 1
matches on the 3D table of Neo Forge 3D. Menus, decks and modes stay with NeoForge; for the human
seat NeoForge starts Godot as the game table and drives it through the Forge bridge (protocol v3,
local socket). A switch in NeoForge's settings turns the 3D table on and off.

The 3D table is only used for **two-player matches** (you against one opponent, Commander 1 vs 1
too). Matches with more players, two people on one computer (hot seat), Planechase, Archenemy and
Vanguard keep using NeoForge's own table.

Everything here is kept separate from the bridge itself, so it can be handed to any NeoForge
repository.

## Contents

| File | What |
|---|---|
| `src/forge/neo/clash/ClashTable.java` | Wrapper for NeoForge: reads the settings (system property → NeoForge setting → environment variable), decides whether a match runs on the 3D table (`useFor`: switched on, 1 vs 1, no hot seat, no unsupported variant), starts the table via `clash.forgebridge.TableLauncher`, provides the remote-controlled player (`ClashBridge.remotePlayer`) |
| `neoforge-host.patch` | Changes to NeoForge's own classes against NeoForge upstream: `NeoGame` (3D table for the human seat of a 1 vs 1 match, `NeoMatchUI` runs invisibly as a spectator, NeoForge's window says the match runs in the 3D table window), `adventure/NeoDuelBridge` (the same for Adventure duels: they are set up by Forge's Adventure and start there instead of in `NeoGame`; the human, last in the Adventure's seat list, moves to seat 0 – `ClashTable.openForDuel`), `SafeAi` (do not replace the remote seat with the AI fallback), `NeoMain` (test command `clash`), `SettingsPanel` (section "3D table (Neo Forge 3D)" at the top of the *Table* tab), language files |

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
`~/.local/opt/apache-maven-*/bin/mvn`, or `MVN=…`). Java 17 or newer.

`neoforge_play.sh` passes the Godot executable (`godot` from the `PATH`, or `CLASH_GODOT`) and
the project folder (`godot/` of this repository) as defaults, so only the switch needs to be
turned on. The Godot extension has to be built (`cd rust && cargo build`).

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

In NeoForge under *Settings → Table → 3D table (Neo Forge 3D)*: "Play 1 vs 1 games on the 3D
table" = yes; "Godot program" (Godot 4.7) and "Neo Forge 3D project folder" (`godot/` of Neo
Forge 3D) if the start script does not provide them. Applies from the next match.

Each value is taken from the first source that has it:

| System property (this start only) | NeoForge setting | Environment variable (default) | Meaning |
|---|---|---|---|
| `neo.clash=true` | `clash3d` | `NEOFORGE_CLASH=true` | use the 3D table |
| `neo.clash.godot` | `clash3d.godot` | `CLASH_GODOT` | Godot executable (otherwise `godot`) |
| `neo.clash.project` | `clash3d.project` | `CLASH_PROJECT` | Godot project folder of Neo Forge 3D |

If the table does not connect within 2 minutes, NeoForge plays with its own table.

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
