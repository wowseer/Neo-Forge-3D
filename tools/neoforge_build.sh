#!/bin/sh
# Baut NeoForge aus dem Quellcode MIT dem 3D-Tisch (Weg aus NeoForges BUILDING.md, „macOS and Linux“):
#   1. Brücke + ClashTable ins NeoForge-Repo kopieren (tools/sync_neoforge.sh); fehlt dort noch der
#      Patch an NeoForges Klassen, wird integrations/neoforge/neoforge-host.patch angewendet
#   2. NeoForges Modul forge-gui-neo in den Forge-Checkout kopieren (ohne dessen target/)
#   3. mit Maven bauen (-pl forge-gui-neo -am)
#   4. den Spieltisch exportieren (tools/export_table.sh) und nach forge-gui-neo/target/clash3d/
#      legen: NeoForge startet dann ihn statt „Godot + Projektordner“ (ClashTable.bundledTable)
# Danach starten: tools/neoforge_play.sh
#   NEOFORGE   NeoForge-Repo (Standard ~/OProgrammieren/Java/NeoForge)
#   FORGE_SRC  Forge-Checkout auf NeoForges FORGE_REF mit <module>forge-gui-neo</module> im pom.xml
#              (Standard ~/OProgrammieren/Java/forge)
#   MVN        Maven (Standard: mvn im PATH, sonst ~/.local/opt/apache-maven-*/bin/mvn)
set -e
cd "$(dirname "$0")/.."
REPO="$(pwd)"
NEOFORGE="${NEOFORGE:-$HOME/OProgrammieren/Java/NeoForge}"
FORGE_SRC="${FORGE_SRC:-$HOME/OProgrammieren/Java/forge}"
if [ -z "$MVN" ]; then
  MVN="$(command -v mvn || true)"
  if [ -z "$MVN" ]; then
    MVN="$(ls -d "$HOME"/.local/opt/apache-maven-*/bin/mvn 2>/dev/null | tail -n 1)"
  fi
fi
[ -x "$MVN" ] || { echo "Maven nicht gefunden (MVN=…)"; exit 1; }
grep -q '<module>forge-gui-neo</module>' "$FORGE_SRC/pom.xml" \
  || { echo "$FORGE_SRC/pom.xml: <module>forge-gui-neo</module> fehlt (BUILDING.md, Schritt 3)"; exit 1; }

# 1. NeoForge-Repo
if ! grep -q 'ClashTable' "$NEOFORGE/forge-gui-neo/src/main/java/forge/neo/match/NeoGame.java"; then
  git -C "$NEOFORGE" apply "$REPO/integrations/neoforge/neoforge-host.patch"
  echo "Patch angewendet: $NEOFORGE"
fi
NEOFORGE="$NEOFORGE" tools/sync_neoforge.sh

# 2. Modul in den Forge-Checkout (target/ bleibt, damit Maven nur Geändertes neu übersetzt)
mkdir -p "$FORGE_SRC/forge-gui-neo"
find "$FORGE_SRC/forge-gui-neo" -mindepth 1 -maxdepth 1 ! -name target -exec rm -rf {} +
cp -R "$NEOFORGE/forge-gui-neo/." "$FORGE_SRC/forge-gui-neo/"
rm -rf "$FORGE_SRC/forge-gui-neo/target/classes/clash" "$FORGE_SRC/forge-gui-neo/target/classes/forge/neo/clash"

# 3. Bauen (ohne Checkstyle, launch4j und das große Jar von forge-gui-mobile-dev, wie linux/empaquetar.sh)
cd "$FORGE_SRC"
MAVEN_OPTS="${MAVEN_OPTS:--Dfile.encoding=UTF-8 -Xmx2g}" "$MVN" -B -ntp package -DskipTests \
  -DskipLaunch4j=true -Dcheckstyle.skip=true -Dassembly.skipAssembly=true -pl forge-gui-neo -am

# 4. Spieltisch exportieren und mitliefern
cd "$REPO"
tools/export_table.sh
rm -rf "$FORGE_SRC/forge-gui-neo/target/clash3d"
cp -R export/linux "$FORGE_SRC/forge-gui-neo/target/clash3d"
echo "OK: $FORGE_SRC/forge-gui-neo/target mit Spieltisch in target/clash3d (starten mit tools/neoforge_play.sh)"
