#!/bin/sh
# Spielfelder aus "Set Pieces/<Name>_Spielfeld.blend" für den Tisch exportieren:
# godot/setpieces/<name>.glb (Materialien in Texturen gebacken, tools/setpiece_export.py).
# Ohne Argument alle, sonst nur die genannten (z. B. "ebene wald"). Vorhandene .glb werden
# übersprungen, außer mit FORCE=1. Dauer: ca. 15–20 min je Spielfeld (Cycles auf der CPU).
set -e
cd "$(dirname "$0")/.."
BLENDER="${BLENDER:-blender}"
mkdir -p godot/setpieces
for f in "Set Pieces"/*_Spielfeld.blend; do
  name=$(basename "$f" _Spielfeld.blend | tr '[:upper:]' '[:lower:]')
  if [ $# -gt 0 ] && ! echo " $* " | grep -q " $name "; then continue; fi
  out="godot/setpieces/$name.glb"
  if [ -f "$out" ] && [ "${FORCE:-0}" != 1 ]; then echo "vorhanden: $out"; continue; fi
  echo "== $name"
  "$BLENDER" -b "$f" --python tools/setpiece_export.py -- "$PWD/$out" 2>&1 | grep '\[setpiece\]' || true
  [ -f "$out" ] || { echo "Export fehlgeschlagen: $f"; exit 1; }
done
echo "OK: $(ls godot/setpieces/*.glb 2>/dev/null | wc -l) Spielfelder in godot/setpieces/"
