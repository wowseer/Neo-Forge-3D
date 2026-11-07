#!/bin/sh
# Startet die Brücke von Hand (zum Testen). Protokoll über stdin/stdout, siehe
# docs/Forge-Protocol.md. FORGE_DIR = Forge-Installationsordner (mit res/).
BRIDGE_DIR="$(cd "$(dirname "$0")" && pwd)"
if [ -z "$FORGE_DIR" ]; then
  # Projektkopie (forge-bridge/forge/) bevorzugen, sonst ~/Games/Forge
  if [ -d "$BRIDGE_DIR/forge/res" ]; then FORGE_DIR="$BRIDGE_DIR/forge"; else FORGE_DIR="$HOME/Games/Forge"; fi
fi
FORGE_JAR="${FORGE_JAR:-$(ls "$FORGE_DIR"/forge-gui-desktop-*-jar-with-dependencies.jar 2>/dev/null | sort | tail -n 1)}"
cd "$FORGE_DIR" || exit 1
# Speicher: ungenutzten Heap zurückgeben (wie clash-forge-client, JVM_MEMORY_ARGS)
exec java -Xmx2g -XX:+UseG1GC -XX:MinHeapFreeRatio=10 -XX:MaxHeapFreeRatio=40 -XX:G1PeriodicGCInterval=10000 \
  -Dfile.encoding=UTF-8 -Djava.awt.headless=true \
  -cp "$BRIDGE_DIR/build/forge-bridge.jar:$FORGE_JAR" clash.forgebridge.app.BridgeMain
