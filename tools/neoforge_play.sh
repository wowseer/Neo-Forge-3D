#!/bin/sh
# Startet das mit tools/neoforge_build.sh gebaute NeoForge (normales Menü). Ob 1-gegen-1-Partien am
# 3D-Tisch laufen, schaltet man in NeoForge unter Einstellungen → Tisch → „3D-Tisch (Neo Forge 3D)“.
# Godot-Programm und Projektordner gibt dieses Skript als Vorgabe mit (Umgebungsvariablen
# CLASH_GODOT / CLASH_PROJECT); was dort in den Einstellungen eingetragen ist, geht vor.
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
[ -f "$REPO/rust/target/debug/libneo_forge_3d.so" ] \
  || echo "Hinweis: Godot-Erweiterung fehlt (cd rust && cargo build) – ohne sie startet der 3D-Tisch nicht"
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
