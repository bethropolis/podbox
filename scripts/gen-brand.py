#!/usr/bin/env python3
"""Generate the podbox brand assets in docs/assets/.

  podbox-mark.svg         the mark alone (outlined box + split pod)
  favicon.svg             the mark as a solid tile, light (default)
  favicon-light.svg       the same tile for light browser chrome
  favicon-dark.svg        the same tile for dark browser chrome
  podbox-logo.svg         mark + pixel wordmark lockup (README header, navbar)
  podbox-cover-image.svg  1200x600 social card (og:image source)

Colours are Catppuccin: Latte by default, Mocha under
prefers-color-scheme: dark. The site's own <PodboxLogo> component draws the
same geometry with the live theme tokens instead.

Usage: scripts/gen-brand.py
Then website/scripts/sync-content.ts copies docs/assets into the site.
The raster copies (favicon.ico, podbox-logo.png) are rendered from these
SVGs; see the comment at the bottom of this file.
"""

from __future__ import annotations

from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "docs" / "assets"

# The mark, on a 512 grid. Shared with website/src/design/components/PodboxLogo.tsx.
BOX = dict(x=56, y=56, w=400, h=400, rx=104, stroke=44)
POD_X, POD_W, POD_R = 181, 150, 75
POD_TOP, POD_BOTTOM, SEAM = 122, 390, 22  # seam: gap between the halves
MID = (POD_TOP + POD_BOTTOM) / 2


def pod_paths() -> tuple[str, str]:
    """Top and bottom halves of the split pod as SVG path data."""
    l, r = POD_X, POD_X + POD_W
    t_end, b_start = MID - SEAM / 2, MID + SEAM / 2
    top = f"M{l} {t_end} V{POD_TOP + POD_R} A{POD_R} {POD_R} 0 0 1 {r} {POD_TOP + POD_R} V{t_end} Z"
    bottom = f"M{l} {b_start} H{r} V{POD_BOTTOM - POD_R} A{POD_R} {POD_R} 0 0 1 {l} {POD_BOTTOM - POD_R} Z"
    return top, bottom


# Pixel wordmark "podbox" (figlet "DOS Rebel", see scripts/gen-logo-svg.sh):
# (x, y, w, h, is_shadow) on a 528x152 grid.
WORDMARK = [(216,18,40,10.5,0),(264,18,40,10.5,0),(208,28.5,16,10.5,1),(224,28.5,24,10.5,0),(256,28.5,16,10.5,1),(272,28.5,24,10.5,0),(32,39,64,10.5,0),(120,39,48,10.5,0),(192,39,56,10.5,0),(264,39,8,10.5,1),(272,39,56,10.5,0),(352,39,48,10.5,0),(416,39,40,10.5,0),(464,39,40,10.5,0),(24,49.5,16,10.5,1),(40,49.5,24,10.5,0),(64,49.5,16,10.5,1),(80,49.5,24,10.5,0),(112,49.5,24,10.5,0),(136,49.5,16,10.5,1),(152,49.5,24,10.5,0),(184,49.5,24,10.5,0),(208,49.5,16,10.5,1),(224,49.5,24,10.5,0),(264,49.5,8,10.5,1),(272,49.5,24,10.5,0),(296,49.5,16,10.5,1),(312,49.5,24,10.5,0),(344,49.5,24,10.5,0),(368,49.5,16,10.5,1),(384,49.5,24,10.5,0),(408,49.5,16,10.5,1),(424,49.5,24,10.5,0),(456,49.5,16,10.5,1),(472,49.5,24,10.5,0),(32,60,8,10.5,1),(40,60,24,10.5,0),(72,60,8,10.5,1),(80,60,24,10.5,0),(104,60,8,10.5,1),(112,60,24,10.5,0),(144,60,8,10.5,1),(152,60,24,10.5,0),(176,60,8,10.5,1),(184,60,24,10.5,0),(216,60,8,10.5,1),(224,60,24,10.5,0),(264,60,8,10.5,1),(272,60,24,10.5,0),(304,60,8,10.5,1),(312,60,24,10.5,0),(336,60,8,10.5,1),(344,60,24,10.5,0),(376,60,8,10.5,1),(384,60,24,10.5,0),(416,60,24,10.5,1),(440,60,40,10.5,0),(480,60,8,10.5,1),(32,70.5,8,10.5,1),(40,70.5,24,10.5,0),(72,70.5,8,10.5,1),(80,70.5,24,10.5,0),(104,70.5,8,10.5,1),(112,70.5,24,10.5,0),(144,70.5,8,10.5,1),(152,70.5,24,10.5,0),(176,70.5,8,10.5,1),(184,70.5,24,10.5,0),(216,70.5,8,10.5,1),(224,70.5,24,10.5,0),(264,70.5,8,10.5,1),(272,70.5,24,10.5,0),(304,70.5,8,10.5,1),(312,70.5,24,10.5,0),(336,70.5,8,10.5,1),(344,70.5,24,10.5,0),(376,70.5,8,10.5,1),(384,70.5,24,10.5,0),(424,70.5,24,10.5,0),(448,70.5,24,10.5,1),(472,70.5,24,10.5,0),(32,81,8,10.5,1),(40,81,56,10.5,0),(104,81,16,10.5,1),(120,81,48,10.5,0),(176,81,16,10.5,1),(192,81,64,10.5,0),(264,81,64,10.5,0),(336,81,16,10.5,1),(352,81,48,10.5,0),(416,81,40,10.5,0),(464,81,40,10.5,0),(32,91.5,8,10.5,1),(40,91.5,24,10.5,0),(64,91.5,24,10.5,1),(112,91.5,48,10.5,1),(184,91.5,64,10.5,1),(256,91.5,64,10.5,1),(344,91.5,48,10.5,1),(408,91.5,40,10.5,1),(456,91.5,40,10.5,1),(32,102,8,10.5,1),(40,102,24,10.5,0),(32,112.5,40,10.5,0),(24,123,40,10.5,1)]

LIGHT = dict(mauve="#8839ef", lavender="#7287fd", text="#4c4f69", shadow="#acb0be", base="#eff1f5", crust="#dce0e8", sub="#6c6f85", border="#ccd0da")
DARK = dict(mauve="#cba6f7", lavender="#b4befe", text="#cdd6f4", shadow="#45475a", base="#1e1e2e", crust="#11111b", sub="#a6adc8", border="#313244")


def palette_css(selector: str, rules: dict[str, str]) -> str:
    """CSS that maps classes to colours, Latte by default and Mocha in dark mode."""
    def block(p):
        return "".join(f"{selector} .{cls}{{{prop}:{p[key]}}}" for cls, (prop, key) in rules.items())
    return f"{block(LIGHT)}@media (prefers-color-scheme:dark){{{block(DARK)}}}"


def mark_group(dx=0, dy=0, scale=1.0) -> str:
    top, bottom = pod_paths()
    b = BOX
    return (
        f'<g transform="translate({dx} {dy}) scale({scale})">'
        f'<rect x="{b["x"]}" y="{b["y"]}" width="{b["w"]}" height="{b["h"]}" rx="{b["rx"]}" fill="none" stroke-width="{b["stroke"]}" class="box"/>'
        f'<path d="{top}" class="top"/><path d="{bottom}" class="bottom"/>'
        "</g>"
    )


def wordmark_group(dx=0, dy=0, scale=1.0) -> str:
    rects = "".join(
        f'<rect x="{x}" y="{y}" width="{w}" height="{h}" class="{"wm-shadow" if s else "wm"}"/>'
        for x, y, w, h, s in WORDMARK
    )
    return f'<g transform="translate({dx} {dy}) scale({scale})" shape-rendering="crispEdges">{rects}</g>'


MARK_RULES = {"box": ("stroke", "mauve"), "top": ("fill", "mauve"), "bottom": ("fill", "lavender")}
WORD_RULES = {"wm": ("fill", "text"), "wm-shadow": ("fill", "shadow")}


def mark_svg() -> str:
    css = palette_css(".pbx-mark", MARK_RULES)
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" class="pbx-mark" viewBox="0 0 512 512" width="512" height="512" role="img" aria-label="podbox">'
        f"<style>{css}</style>{mark_group()}</svg>\n"
    )


FAVICON_COLOURS = {
    # tile, pod: the light tile is the brand mauve with a pale pod. The dark
    # pod is light too: a near-black pod made the icon look dim on a dark tab bar.
    "light": dict(tile="#8839ef", pod="#eff1f5"),
    "dark": dict(tile="#7a6aa8", pod="#a6adc8"),
}


def favicon_svg(theme: str) -> str:
    # Solid tile: an outline turns to mush at 16px, a filled shape does not.
    # One file per theme, selected by the page's <link media>. Chrome ignores
    # prefers-color-scheme inside SVG favicons, so it is not used here.
    top, bottom = pod_paths()
    c = FAVICON_COLOURS[theme]
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="512" height="512">'
        f'<rect x="16" y="16" width="480" height="480" rx="120" fill="{c["tile"]}"/>'
        # The pod is drawn larger here than in the outlined mark; at 16px the
        # tile's own edge already says "box".
        f'<g transform="translate(256 256) scale(1.3) translate(-256 -256)">'
        f'<path d="{top}" fill="{c["pod"]}"/><path d="{bottom}" fill="{c["pod"]}" opacity=".72"/></g>'
        "</svg>\n"
    )


# The navbar lockup: the mark beside the pixel wordmark, as in the original
# README header. Colours are CSS variables with the light/dark values as
# fallbacks. Inlined on the site, the site's own tokens win and the logo follows
# the theme toggle. Used as an <img> (README), the fallbacks apply and the
# prefers-color-scheme media query switches them.
LOCKUP_COLOURS = {
    # class: (property, site token, light fallback, dark fallback)
    "box": ("stroke", "accent-mauve", "#8839ef", "#cba6f7"),
    "top": ("fill", "accent-mauve", "#8839ef", "#cba6f7"),
    "bottom": ("fill", "accent-lavender", "#7287fd", "#b4befe"),
    "wm": ("fill", "text-primary", "#4c4f69", "#cdd6f4"),
    "wm-shadow": ("fill", "border-focus", "#acb0be", "#45475a"),
}


def token_css(selector: str) -> str:
    def block(idx: int) -> str:
        return "".join(
            f"{selector} .{cls}{{{prop}:var(--{tok},{vals[idx]})}}"
            for cls, (prop, tok, *vals) in LOCKUP_COLOURS.items()
        )
    return f"{block(0)}@media (prefers-color-scheme:dark){{{block(1)}}}"


def logo_svg() -> str:
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" class="pbx-logo" viewBox="0 0 680 152" width="680" height="152" role="img" aria-label="podbox">'
        f"<style>{token_css('.pbx-logo')}</style>"
        f"{mark_group(8, 14, 0.24)}{wordmark_group(140, 0)}</svg>\n"
    )


def cover_svg() -> str:
    # Social card: always the dark palette (scrapers don't do media queries).
    d = DARK
    chips = ["one TOML file", "systemd runs it", "no host daemon"]
    chip_svg, x = "", 96
    for c in chips:
        w = 26 + len(c) * 10.2
        chip_svg += (
            f'<rect x="{x}" y="430" width="{w:.0f}" height="40" rx="8" fill="none" stroke="{d["border"]}" stroke-width="2"/>'
            f'<text x="{x + 13}" y="456" font-family="JetBrains Mono, DejaVu Sans Mono, monospace" font-size="17" fill="{d["sub"]}">{c}</text>'
        )
        x += w + 14
    top, bottom = pod_paths()
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1200 600" width="1200" height="600">'
        f'<rect width="1200" height="600" fill="{d["base"]}"/>'
        f'<rect x="0" y="560" width="1200" height="40" fill="{d["crust"]}"/>'
        f'<g transform="translate(96 168) scale(1.05)" shape-rendering="crispEdges">'
        + "".join(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{d["shadow"] if s else d["text"]}"/>' for x, y, w, h, s in WORDMARK)
        + "</g>"
        f'<text x="98" y="384" font-family="Geist, Inter, DejaVu Sans, sans-serif" font-size="34" fill="{d["sub"]}">Define once. Run anywhere. '
        f'<tspan fill="{d["mauve"]}">No host daemon.</tspan></text>'
        f"{chip_svg}"
        f'<g transform="translate(850 150) scale(0.6)">'
        f'<rect x="{BOX["x"]}" y="{BOX["y"]}" width="{BOX["w"]}" height="{BOX["h"]}" rx="{BOX["rx"]}" fill="none" stroke="{d["mauve"]}" stroke-width="{BOX["stroke"]}"/>'
        f'<path d="{top}" fill="{d["mauve"]}"/><path d="{bottom}" fill="{d["lavender"]}"/></g>'
        f'<text x="96" y="587" font-family="JetBrains Mono, DejaVu Sans Mono, monospace" font-size="16" fill="{d["sub"]}">bethropolis.github.io/podbox</text>'
        "</svg>\n"
    )


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    for name, fn in {
        "podbox-mark.svg": mark_svg,
        "favicon.svg": lambda: favicon_svg("light"),
        "favicon-light.svg": lambda: favicon_svg("light"),
        "favicon-dark.svg": lambda: favicon_svg("dark"),
        "podbox-logo.svg": logo_svg,
        "podbox-cover-image.svg": cover_svg,
    }.items():
        (OUT / name).write_text(fn(), encoding="utf-8")
        print(f"wrote {OUT / name}")


# Rasters (no extra tooling needed beyond a Chromium and ImageMagick):
#   chrome --headless --screenshot=tile.png --window-size=512,512 \
#     --default-background-color=00000000 docs/assets/favicon.svg
#   magick tile.png -resize 180x180 website/public/podbox-logo.png
#   magick tile.png -define icon:auto-resize=48,32,16 website/public/favicon.ico

if __name__ == "__main__":
    main()
