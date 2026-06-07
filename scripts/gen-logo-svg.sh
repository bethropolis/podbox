#!/usr/bin/env bash
set -euo pipefail

# podbox logo SVG generator (Vector-Shape Optimized)
# Reads raw ASCII art from stdin, outputs a font-independent, high-fidelity vector SVG.
#
# Usage:
#   1. Install figlet + a font inside a running container:
#        podbox -C <name> exec --root pacman -S figlet       # Arch/Cachy
#        podbox -C <name> exec --root apt-get install figlet  # Debian/Ubuntu
#
#   2. Download the "DOS Rebel" figlet font into the container:
#        podbox -C <name> exec --root curl -sLo /usr/share/figlet/rebel.flf \
#          "https://raw.githubusercontent.com/xero/figlet-fonts/main/DOS%20Rebel.flf"
#
#   3. Generate the ASCII art and pipe it into this script:
#        podbox -C <name> exec figlet -f rebel "podbox" 2>/dev/null \
#          | bash scripts/gen-logo-svg.sh > docs/assets/podbox-logo.svg
#
#   4. The SVG is written to stdout — redirect as needed.

CW=8         # Grid Cell Width (pixels)
CH=10.5      # Grid Cell Height (pixels)
PAD_X=24     # Horizontal padding (pixels)
PAD_Y=18     # Vertical padding (pixels)

readarray -t LINES

MAX_LEN=0
for line in "${LINES[@]}"; do
    (( ${#line} > MAX_LEN )) && MAX_LEN=${#line}
done

# Calculate absolute dimension bounds
W=$(python3 -c "import math; print(math.ceil($PAD_X * 2 + $MAX_LEN * $CW))")
H=$(python3 -c "import math; print(math.ceil($PAD_Y * 2 + ${#LINES[@]} * $CH))")

# Define absolute linear gradient bounds matching printable content
X1_GRAD=$PAD_X
X2_GRAD=$(( W - PAD_X ))

cat <<EOF
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${W} ${H}" width="100%" height="auto">
  <defs>
    <linearGradient id="g" x1="${X1_GRAD}" y1="0" x2="${X2_GRAD}" y2="0" gradientUnits="userSpaceOnUse">
      <stop offset="0%"   stop-color="#3b82f6"/>
      <stop offset="18%"  stop-color="#6366f1"/>
      <stop offset="36%"  stop-color="#8b5cf6"/>
      <stop offset="54%"  stop-color="#a855f7"/>
      <stop offset="72%"  stop-color="#d946ef"/>
      <stop offset="100%" stop-color="#ec4899"/>
    </linearGradient>
  </defs>
EOF

for row in "${!LINES[@]}"; do
    line="${LINES[row]}"
    len=${#line}
    
    current_char=""
    start_col=0
    run_length=0
    
    # Calculate row top position
    Y=$(python3 -c "print(round($PAD_Y + $row * $CH, 2))")
    
    for (( col=0; col<=len; col++ )); do
        if (( col < len )); then
            char="${line:col:1}"
        else
            char=""
        fi
        
        if [[ "$char" != "$current_char" ]]; then
            if [[ -n "$current_char" && "$current_char" != " " ]]; then
                X=$(python3 -c "print(round($PAD_X + $start_col * $CW, 2))")
                WIDTH=$(python3 -c "print(round($run_length * $CW, 2))")
                
                # Map standard ASCII block characters to correct shade opacities
                opacity=""
                if [[ "$current_char" == "▒" ]]; then
                    opacity=' opacity="0.45"'
                elif [[ "$current_char" == "░" ]]; then
                    opacity=' opacity="0.25"'
                elif [[ "$current_char" == "▓" ]]; then
                    opacity=' opacity="0.75"'
                fi
                
                printf '  <rect x="%s" y="%s" width="%s" height="%s" fill="url(#g)"%s />\n' \
                    "$X" "$Y" "$WIDTH" "$CH" "$opacity"
            fi
            
            current_char="$char"
            start_col=$col
            run_length=1
        else
            (( run_length++ ))
        fi
    done
done

echo '</svg>'
