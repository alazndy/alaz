#!/bin/sh
# A.L.A.Z — Linux kaldırma. Kayıtlar (~/.local/share/alaz) yerinde kalır.
bin="${XDG_BIN_HOME:-$HOME/.local/bin}"
data="${XDG_DATA_HOME:-$HOME/.local/share}"
rm -f "$bin/alaz" "$data/applications/alaz.desktop" "$data/icons/hicolor/256x256/apps/alaz.png"
echo "A.L.A.Z kaldırıldı. Kayıtların $data/alaz altında duruyor."
