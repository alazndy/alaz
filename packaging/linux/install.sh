#!/bin/sh
# A.L.A.Z — Linux kurulumu (yalnız bu kullanıcı için, sudo gerekmez).
# Oyunu ~/.local/bin altına, menü kaydını ve simgeyi ~/.local/share altına koyar.
set -e
here="$(cd "$(dirname "$0")" && pwd)"
bin="${XDG_BIN_HOME:-$HOME/.local/bin}"
data="${XDG_DATA_HOME:-$HOME/.local/share}"
mkdir -p "$bin" "$data/applications" "$data/icons/hicolor/256x256/apps"
install -m 755 "$here/alaz" "$bin/alaz"
install -m 644 "$here/alaz.png" "$data/icons/hicolor/256x256/apps/alaz.png"
sed "s|^Exec=alaz|Exec=$bin/alaz|" "$here/alaz.desktop" > "$data/applications/alaz.desktop"
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$data/applications" >/dev/null 2>&1 || true
echo "A.L.A.Z kuruldu: $bin/alaz (menüde de var)."
case ":$PATH:" in *":$bin:"*) ;; *) echo "Not: $bin PATH içinde değil; menüden ya da $bin/alaz ile açabilirsin." ;; esac
