#!/bin/sh
# Bringt die NeoForge-Anbindung in ein NeoForge-Repo (Standard ~/OProgrammieren/Java/NeoForge):
#   1. Brücken-Bibliothek: forge-bridge/src/clash/forgebridge (ohne den Programmstart app/)
#   2. NeoForge-Hülle:     integrations/neoforge/src/forge/neo/clash/ClashTable.java
# Die Änderungen an NeoForges eigenen Klassen (NeoGame, SafeAi, NeoMain, SettingsPanel,
# Sprachdateien) stehen als Patch in integrations/neoforge/neoforge-host.patch; einmalig anwenden mit
#   git -C "$NEOFORGE" apply integrations/neoforge/neoforge-host.patch
# Maßgeblich ist immer die Fassung in Neo Forge 3D; in NeoForge nicht von Hand ändern.
set -e
cd "$(dirname "$0")/.."
NEOFORGE="${NEOFORGE:-$HOME/OProgrammieren/Java/NeoForge}"
JAVA="$NEOFORGE/forge-gui-neo/src/main/java"
DEST="$JAVA/clash/forgebridge"
rm -rf "$DEST"
mkdir -p "$DEST"
cp -r forge-bridge/src/clash/forgebridge/. "$DEST/"
rm -rf "$DEST/app"
mkdir -p "$JAVA/forge/neo/clash"
cp integrations/neoforge/src/forge/neo/clash/*.java "$JAVA/forge/neo/clash/"
echo "OK: $(find "$DEST" -name '*.java' | wc -l) Brücken-Dateien und ClashTable nach $JAVA"
