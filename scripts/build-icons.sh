#!/bin/sh
# Genera los íconos rasterizados a partir de packaging/simplestl.svg:
# - packaging/simplestl.ico: varios tamaños, incrustado en el .exe de Windows (build.rs);
# - packaging/simplestl-64.png: ícono de la ventana (barra de título y de tareas).
# Requiere Inkscape e ImageMagick. Volver a ejecutarlo tras editar el SVG.
set -e
cd "$(dirname "$0")/.."
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
for size in 16 24 32 48 64 128 256; do
    inkscape packaging/simplestl.svg -o "$tmp/$size.png" -w "$size" -h "$size" >/dev/null 2>&1
done
convert "$tmp/16.png" "$tmp/24.png" "$tmp/32.png" "$tmp/48.png" "$tmp/64.png" "$tmp/128.png" "$tmp/256.png" \
    packaging/simplestl.ico
# PNG32: RGBA de 8 bits, el formato que espera `window_icon` en main.rs.
convert "$tmp/64.png" PNG32:packaging/simplestl-64.png
