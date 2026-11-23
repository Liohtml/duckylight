#!/bin/sh
# Installs duckylight for the current user. Only the udev rule needs sudo, once: it lets you
# open the keyboard's lighting interface without being root.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
bin="${XDG_BIN_HOME:-$HOME/.local/bin}"
data="${XDG_DATA_HOME:-$HOME/.local/share}"

install -Dm755 "$here/duckylight" "$bin/duckylight"
install -Dm644 "$here/duckylight.png" "$data/icons/hicolor/256x256/apps/duckylight.png"
install -Dm644 "$here/duckylight.desktop" "$data/applications/duckylight.desktop"
echo "duckylight liegt jetzt in $bin."

rule=/etc/udev/rules.d/70-duckylight.rules
if [ ! -f "$rule" ] || ! cmp -s "$here/70-duckylight.rules" "$rule"; then
    echo "Für den Zugriff auf die Tastatur ohne root wird einmalig eine udev-Regel eingerichtet (sudo):"
    sudo install -Dm644 "$here/70-duckylight.rules" "$rule"
    sudo udevadm control --reload
    sudo udevadm trigger
    echo "Regel eingerichtet. Falls die Tastatur schon steckt: einmal ab- und wieder anstecken."
fi

case ":$PATH:" in
    *":$bin:"*) ;;
    *) echo "Hinweis: $bin steht nicht im PATH. Starte duckylight über das Anwendungsmenü oder mit $bin/duckylight." ;;
esac
command -v parec >/dev/null 2>&1 || echo "Hinweis: Für den Musikmodus fehlt »parec« (Paket libpulse bzw. pulseaudio-utils)."
echo "Fertig. Start: duckylight"
