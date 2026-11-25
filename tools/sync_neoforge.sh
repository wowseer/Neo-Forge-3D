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
# NeoForge baut auf einem neueren Forge als die Brücke (2.0.15): dort bekommt
# PlayerController.chooseCardsToDiscardFrom als letzten Parameter ein DelayedReveal statt der
# CardCollectionView. Der Wert wird nur an super weitergereicht, also genügt der Typ.
RPC="$DEST/engine/RemotePlayerController.java"
sed -i 's/int min, int max, CardCollectionView visibleToChooser) {/int min, int max, forge.game.player.DelayedReveal visibleToChooser) {/' "$RPC"
grep -q 'forge.game.player.DelayedReveal visibleToChooser' "$RPC" \
  || { echo "Anpassung chooseCardsToDiscardFrom fehlgeschlagen: $RPC"; exit 1; }
mkdir -p "$JAVA/forge/neo/clash"
cp integrations/neoforge/src/forge/neo/clash/*.java "$JAVA/forge/neo/clash/"
echo "OK: $(find "$DEST" -name '*.java' | wc -l) Brücken-Dateien und ClashTable nach $JAVA"
