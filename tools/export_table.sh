#!/bin/sh
# Exportiert den Spieltisch als eigenständiges Programm (Godot-Export „Linux“ aus
# godot/export_presets.cfg, Spieldaten eingebettet) nach export/linux/:
#   neo-forge-3d.x86_64 + libneo_forge_3d.so (Rust im Release-Modus)
# NeoForge startet ihn statt „Godot + Projektordner“, wenn er in forge-gui-neo/target/clash3d/
# liegt (tools/neoforge_build.sh kopiert ihn dorthin; ClashTable.bundledTable).
# Braucht die Godot-Exportvorlagen der Godot-Version (~/.local/share/godot/export_templates/<version>/,
# Editor: Editor → Exportvorlagen verwalten).
#   GODOT  Godot-Editor (Standard: godot im PATH)
set -e
cd "$(dirname "$0")/.."
GODOT="${GODOT:-godot}"
VER="$("$GODOT" --version | sed -E 's/^([0-9]+\.[0-9]+(\.[0-9]+)?\.[a-z0-9]+).*/\1/')"
TPL="${XDG_DATA_HOME:-$HOME/.local/share}/godot/export_templates/$VER/linux_release.x86_64"
[ -f "$TPL" ] || { echo "Exportvorlage fehlt: $TPL (Godot-Editor → Exportvorlagen verwalten)"; exit 1; }
# ohne eingebettete Klassen-Doku (nur im Editor nützlich)
(cd rust && cargo build --release --no-default-features)
rm -rf export/linux && mkdir -p export/linux
"$GODOT" --headless --path godot --export-release "Linux" ../export/linux/neo-forge-3d.x86_64
[ -x export/linux/neo-forge-3d.x86_64 ] && [ -f export/linux/libneo_forge_3d.so ] \
  || { echo "Export unvollständig: export/linux/"; exit 1; }
echo "OK: export/linux/ ($(du -sh export/linux | cut -f1))"
