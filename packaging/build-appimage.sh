#!/usr/bin/env bash
# Construit Flatpak_Manager-<version>-<arch>.AppImage dans le dossier dist/.
# Usage : packaging/build-appimage.sh
# Prérequis : cargo, curl (ou appimagetool déjà dans le PATH).
# Astuce : compilez sur une distribution un peu ancienne pour que l'AppImage
# fonctionne sur davantage de systèmes (elle dépend de la glibc de la machine de build).
set -euo pipefail

cd "$(dirname "$0")/.."
ARCH="$(uname -m)"
VERSION="$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -1)"
OUT="dist"
APPDIR="$(mktemp -d)/FlatpakManager.AppDir"
trap 'rm -rf "$(dirname "$APPDIR")"' EXIT

cargo build --release
mkdir -p "$APPDIR/usr/bin" "$OUT"
install -Dm755 target/release/flatpak-manager "$APPDIR/usr/bin/flatpak-manager"
install -Dm644 assets/logo.png "$APPDIR/flatpak-manager.png"
# Une icône par taille : le bureau choisit la plus proche au lieu de réduire un grand fichier.
for icon in assets/icons/*x*.png; do
  size="$(basename "$icon" .png)"
  install -Dm644 "$icon" "$APPDIR/usr/share/icons/hicolor/$size/apps/flatpak-manager.png"
done
install -Dm644 packaging/flatpak-manager.desktop "$APPDIR/flatpak-manager.desktop"
install -Dm644 packaging/flatpak-manager.desktop "$APPDIR/usr/share/applications/flatpak-manager.desktop"
install -Dm755 packaging/flatpak-manager-servicemenu.desktop "$APPDIR/usr/share/kio/servicemenus/flatpak-manager-servicemenu.desktop" 2>/dev/null || true

# Le programme lance flatpak / flatpak-builder de la machine : on ne touche pas à LD_LIBRARY_PATH.
cat > "$APPDIR/AppRun" <<'RUN'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/usr/bin/flatpak-manager" "$@"
RUN
chmod +x "$APPDIR/AppRun"

TOOL="$(command -v appimagetool || true)"
if [ -z "$TOOL" ]; then
  TOOL="$OUT/appimagetool"
  if [ ! -x "$TOOL" ]; then
    curl -fsSL -o "$TOOL" "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-$ARCH.AppImage"
    chmod +x "$TOOL"
  fi
fi

ARCH="$ARCH" "$TOOL" --appimage-extract-and-run "$APPDIR" "$OUT/Flatpak_Manager-$VERSION-$ARCH.AppImage"
echo "AppImage créé : $OUT/Flatpak_Manager-$VERSION-$ARCH.AppImage"
