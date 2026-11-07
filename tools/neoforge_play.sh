#!/bin/sh
# Startet das mit tools/neoforge_build.sh gebaute NeoForge (normales Menü). Ob 1-gegen-1-Partien am
# 3D-Tisch laufen, schaltet man in NeoForge unter Einstellungen → Tisch → „3D-Tisch (Neo Forge 3D)“.
# Gespielt wird mit dem exportierten Tisch aus target/clash3d (legt tools/neoforge_build.sh an); nur
# ohne ihn mit Godot + Projektordner, die dieses Skript als Vorgabe mitgibt (CLASH_GODOT /
# CLASH_PROJECT; was in den Einstellungen eingetragen ist, geht vor).
#   FORGE_SRC      Forge-Checkout mit gebautem forge-gui-neo (Standard ~/OProgrammieren/Java/forge)
#   NEOFORGE_DATA  Decks, Einstellungen, Spielstände, Kartenbilder (Standard ~/.local/share/NeoForge)
#   CLASH_JAVA     Java ab 17 (Standard: java im PATH)
#   CLASH_GODOT    Godot 4.7 (Standard: godot im PATH)
# Weitere Argumente gehen an NeoForge.
set -e
cd "$(dirname "$0")/.."
REPO="$(pwd)"
FORGE_SRC="${FORGE_SRC:-$HOME/OProgrammieren/Java/forge}"
DATA="${NEOFORGE_DATA:-${XDG_DATA_HOME:-$HOME/.local/share}/NeoForge}"
NEO="$FORGE_SRC/forge-gui-neo"
[ -d "$NEO/target/classes" ] || { echo "Nicht gebaut: $NEO/target – erst tools/neoforge_build.sh"; exit 1; }
[ -x "$NEO/target/clash3d/neo-forge-3d.x86_64" ] \
  || echo "Hinweis: kein exportierter Spieltisch in $NEO/target/clash3d (tools/neoforge_build.sh) – NeoForge nimmt dann Godot + Projektordner"
export CLASH_GODOT="${CLASH_GODOT:-godot}"
export CLASH_PROJECT="${CLASH_PROJECT:-$REPO/godot}"
mkdir -p "$DATA"
cd "$NEO"
exec "${CLASH_JAVA:-java}" \
  -Dfile.encoding=UTF-8 -XX:MaxRAMPercentage=50 -Dneo.dataDir="$DATA" \
  --add-opens=java.base/java.util=ALL-UNNAMED --add-opens=java.base/java.lang=ALL-UNNAMED \
  --add-opens=java.base/java.lang.reflect=ALL-UNNAMED --add-opens=java.base/java.text=ALL-UNNAMED \
  --add-opens=java.base/java.util.concurrent=ALL-UNNAMED --add-opens=java.base/java.math=ALL-UNNAMED \
  --add-opens=java.base/java.net=ALL-UNNAMED \
  -cp "target/classes:target/lib/*" forge.neo.NeoMain "${NEOFORGE_CMD:-ui}" "$@"
