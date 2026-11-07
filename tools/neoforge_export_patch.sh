#!/bin/sh
# Schreibt die Änderungen an NeoForges eigenen Klassen (alles außer der Brücke clash/forgebridge
# und der Hülle forge/neo/clash, die tools/sync_neoforge.sh kopiert) als Patch nach
# integrations/neoforge/neoforge-host.patch – gegen den Stand von NeoForge upstream (origin).
#   NEOFORGE = NeoForge-Repo (Standard ~/OProgrammieren/Java/NeoForge), BASE = Vergleichsstand (origin/HEAD)
set -e
cd "$(dirname "$0")/.."
NEOFORGE="${NEOFORGE:-$HOME/OProgrammieren/Java/NeoForge}"
BASE="${BASE:-origin/HEAD}"
OUT="$(pwd)/integrations/neoforge/neoforge-host.patch"
git -C "$NEOFORGE" diff "$BASE" HEAD -- . \
  ':(exclude)forge-gui-neo/src/main/java/clash' \
  ':(exclude)forge-gui-neo/src/main/java/forge/neo/clash' > "$OUT"
echo "OK: $OUT ($(grep -c '^diff --git' "$OUT") Dateien, gegen $(git -C "$NEOFORGE" rev-parse --short "$BASE"))"
