#!/bin/sh
# Trägt das mit tools/neoforge_build.sh gebaute NeoForge (mit 3D-Tisch) ins Anwendungsmenü ein und
# lässt Wiederholungen (.frp) damit öffnen – wie NeoForges linux/add-to-menu.sh, das aber nur in
# der gepackten itch.io-Fassung funktioniert (es zeigt auf bin/NeoForge, das es im Quellcode nicht
# gibt). Schreibt nur im eigenen Benutzerordner:
#   ~/.local/share/applications/neoforge.desktop
#   ~/.local/share/mime/packages/neoforge-replay.xml
# Entfernen: beide Dateien löschen.
#   NEOFORGE  NeoForge-Repo, für das Symbol (Standard ~/OProgrammieren/Java/NeoForge)
set -e
cd "$(dirname "$0")/.."
REPO="$(pwd)"
NEOFORGE="${NEOFORGE:-$HOME/OProgrammieren/Java/NeoForge}"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
APPS="$DATA/applications"
MIME="$DATA/mime"
ICON="$NEOFORGE/forge-gui-neo/src/main/resources/forge/neo/logo/logo-256.png"
mkdir -p "$APPS" "$MIME/packages"

cat > "$MIME/packages/neoforge-replay.xml" <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<mime-info xmlns="http://www.freedesktop.org/standards/shared-mime-info">
  <mime-type type="application/x-neoforge-replay">
    <comment>Neo Forge replay</comment>
    <glob pattern="*.frp" weight="80"/>
  </mime-type>
</mime-info>
EOF

cat > "$APPS/neoforge.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Neo Forge
Comment=Magic: The Gathering against AI, 1 vs 1 on the Neo Forge 3D table
Exec="$REPO/tools/neoforge_play.sh" %f
Path=$REPO
Icon=$ICON
Terminal=false
Categories=Game;CardGame;
MimeType=application/x-neoforge-replay;
EOF

chmod +x "$APPS/neoforge.desktop"
command -v update-mime-database >/dev/null 2>&1 && update-mime-database "$MIME" >/dev/null 2>&1 || true
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$APPS" >/dev/null 2>&1 || true
command -v xdg-mime >/dev/null 2>&1 && xdg-mime default neoforge.desktop application/x-neoforge-replay >/dev/null 2>&1 || true
echo "OK: $APPS/neoforge.desktop → $REPO/tools/neoforge_play.sh"
