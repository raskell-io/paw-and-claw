#!/usr/bin/env bash
# Draw the app icons for the web build (home screen, PWA, favicon) into web/icons/.
#
#   tools/web/icons.sh [PORTRAIT]        # default: assets/sprites/portraits/bandit.png
#
# Pixel-art icon: the 64x64 commander portrait, its brown backdrop keyed out, over
# forest-green bands and a warm glow, scaled up with nearest-neighbour so every art
# pixel stays crisp. Needs ImageMagick 7 (`magick`).
set -euo pipefail
cd "$(dirname "$0")/../.."
PORTRAIT="${1:-assets/sprites/portraits/bandit.png}"
OUT=web/icons
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$OUT"

# Key out the portrait's flat backdrop from the corners that touch it
magick "$PORTRAIT" -fill none -fuzz 4% \
    -draw 'color 63,0 floodfill' -draw 'color 0,40 floodfill' \
    -draw 'color 63,63 floodfill' -draw 'color 0,63 floodfill' "$TMP/portrait.png"

# Light moss at the top to deep forest at the bottom, in hard bands
draw=()
bands=8
for i in $(seq 0 $((bands - 1))); do
    r=$(( 95 - (95 - 28) * i / (bands - 1) ))
    g=$(( 135 - (135 - 39) * i / (bands - 1) ))
    b=$(( 82 - (82 - 30) * i / (bands - 1) ))
    y0=$(( 64 * i / bands )); y1=$(( 64 * (i + 1) / bands - 1 ))
    draw+=(-fill "rgb($r,$g,$b)" -draw "rectangle 0,$y0 63,$y1")
done
magick -size 64x64 xc:black "${draw[@]}" "$TMP/bg.png"
magick -size 64x64 xc:none -fill 'rgba(240,207,110,0.20)' -draw 'circle 32,27 32,3' \
    -fill 'rgba(240,207,110,0.16)' -draw 'circle 32,27 32,11' "$TMP/glow.png"
magick "$TMP/bg.png" "$TMP/glow.png" -composite "$TMP/portrait.png" -composite "$TMP/icon64.png"

# Full-bleed icons (iOS and most launchers round the corners themselves)
magick "$TMP/icon64.png" -filter point -resize 1024x1024 "$TMP/icon1024.png"
for size in 512 192 180 32; do
    name="icon-$size.png"; [ "$size" = 180 ] && name="apple-touch-icon.png"
    magick "$TMP/icon1024.png" -filter Lanczos -resize "${size}x${size}" -alpha off "$OUT/$name"
done
# Maskable (Android): keep the art inside the central safe zone
magick "$TMP/bg.png" -filter point -resize 512x512 \
    \( "$TMP/glow.png" "$TMP/portrait.png" -background none -flatten -filter point -resize 384x384 \) \
    -gravity south -composite -background "rgb(28,39,30)" -flatten -alpha off "$OUT/icon-maskable-512.png"
echo "icons in $OUT"
