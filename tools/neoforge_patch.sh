#!/bin/sh
# Baut ein Patch-Jar für die installierte NeoForge-Version (ohne Maven): kompiliert den
# eingebetteten Brücken-Code und die geänderten NeoForge-Klassen gegen die Jars der Installation.
# NEOFORGE = NeoForge-Repo, NEOFORGE_APP = installierte NeoForge (Ordner mit lib/app)
set -e
cd "$(dirname "$0")/.."
NEOFORGE="${NEOFORGE:-$HOME/OProgrammieren/Java/NeoForge}"
NEOFORGE_APP="${NEOFORGE_APP:-$HOME/Games/NeoForge}"
SRC="$NEOFORGE/forge-gui-neo/src/main/java"
OUT=forge-bridge/build/neoforge
rm -rf "$OUT" && mkdir -p "$OUT/classes"
javac --release 17 -encoding UTF-8 -nowarn -proc:none -cp "$NEOFORGE_APP/lib/app/*" -sourcepath "$SRC" -implicit:none \
  -d "$OUT/classes" \
  $(find "$SRC"/clash/forgebridge -name "*.java") "$SRC"/forge/neo/clash/*.java \
  "$SRC"/forge/neo/match/NeoGame.java "$SRC"/forge/neo/match/SafeAi.java "$SRC"/forge/neo/NeoMain.java \
  "$SRC"/forge/neo/ui/SettingsPanel.java "$SRC"/forge/neo/adventure/NeoDuelBridge.java
# Sprachdateien (Texte der Einstellung „3D-Tisch“)
mkdir -p "$OUT/classes/forge/neo/lang"
cp "$NEOFORGE"/forge-gui-neo/src/main/resources/forge/neo/lang/*.properties "$OUT/classes/forge/neo/lang/"
jar --create --file "$OUT/neoforge-clash-patch.jar" -C "$OUT/classes" .
echo "OK: $OUT/neoforge-clash-patch.jar"
