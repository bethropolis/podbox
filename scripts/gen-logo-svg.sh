#!/usr/bin/env bash
set -euo pipefail

# podbox logo SVG generator
# Reads raw ASCII art from stdin, outputs SVG with connected block chars.

FONT_SIZE=14
LINE_H=10.5
CH_W=8
PAD_X=24

readarray -t LINES

MAX_LEN=0
for line in "${LINES[@]}"; do
    (( ${#line} > MAX_LEN )) && MAX_LEN=${#line}
done

for i in "${!LINES[@]}"; do
    while (( ${#LINES[i]} < MAX_LEN )); do
        LINES[i]+=' '
    done
done

W=$(( MAX_LEN * CH_W + PAD_X * 2 ))
LAST_Y=$(python3 -c "import math; y = $PAD_X + (${#LINES[@]} - 1) * $LINE_H; print(math.ceil(y))")
H=$(python3 -c "import math; h = $LAST_Y + $FONT_SIZE; print(math.ceil(h))")

cat <<EOF
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${W} ${H}" width="100%" height="auto">
  <defs>
    <linearGradient id="g" x1="0%" y1="0%" x2="100%" y2="0%" gradientUnits="userSpaceOnUse">
      <stop offset="0%"   stop-color="#3b82f6"/>
      <stop offset="18%"  stop-color="#6366f1"/>
      <stop offset="36%"  stop-color="#8b5cf6"/>
      <stop offset="54%"  stop-color="#a855f7"/>
      <stop offset="72%"  stop-color="#d946ef"/>
      <stop offset="100%" stop-color="#ec4899"/>
    </linearGradient>
  </defs>
  <style>
    text {
      font-family: "Courier New", "Liberation Mono", "Noto Mono", monospace;
      font-size: ${FONT_SIZE}px;
      font-weight: bold;
      fill: url(#g);
      letter-spacing: -0.75px;
      text-rendering: geometricPrecision;
    }
  </style>
EOF

for i in "${!LINES[@]}"; do
    Y=$(python3 -c "print(round($PAD_X + $i * $LINE_H, 1))")
    escaped="${LINES[i]//&/&amp;}"
    escaped="${escaped//</&lt;}"
    printf '  <text x="%d" y="%s" xml:space="preserve">%s</text>\n' "$PAD_X" "$Y" "$escaped"
done

echo '</svg>'
