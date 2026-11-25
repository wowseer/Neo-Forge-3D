#!/bin/sh
# Lädt für NeoForge mit dem 3D-Tisch alle Bildausschnitte (Kacheln) und Spielstein-Bilder vorab,
# damit der Tisch beim Spielen nichts mehr nachladen muss (tools/card_images/BulkImages.java).
# Vorhandene Bilder werden übersprungen. Nicht starten, während NeoForge läuft.
#   NEOFORGE_APP = installierte NeoForge (Standard ~/Games/NeoForge), $1 = parallele Downloads
set -e
cd "$(dirname "$0")/.."
APP="${NEOFORGE_APP:-$HOME/Games/NeoForge}"
OUT="${TMPDIR:-/tmp}/neoforge-images-build"
rm -rf "$OUT" && mkdir -p "$OUT"
javac -nowarn -encoding UTF-8 -cp "$APP/lib/app/*" -d "$OUT" tools/card_images/BulkImages.java
cd "$APP"
exec java -Dfile.encoding=UTF-8 -Xmx3g -Dforge.assetsDir="$APP/" -Dneo.dataDir="$APP/datos" \
  --add-opens=java.base/java.util=ALL-UNNAMED --add-opens=java.base/java.lang=ALL-UNNAMED \
  --add-opens=java.base/java.lang.reflect=ALL-UNNAMED --add-opens=java.base/java.text=ALL-UNNAMED \
  --add-opens=java.base/java.util.concurrent=ALL-UNNAMED --add-opens=java.base/java.math=ALL-UNNAMED \
  --add-opens=java.base/java.net=ALL-UNNAMED \
  -cp "$OUT:$APP/lib/app/*" BulkImages "${1:-16}"
