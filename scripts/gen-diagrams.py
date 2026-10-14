#!/usr/bin/env python3
"""Generate the architecture diagrams in docs/assets/.

One small drawing vocabulary (boxes, regions, arrows, labels) shared by every
diagram so they read as a set. Colours are Catppuccin: Latte by default and
Mocha under prefers-color-scheme: dark, which is what GitHub uses for the
README. When the website inlines an SVG, the site's own theme tokens
(--bg-mantle, --accent-mauve, ...) take precedence over those fallbacks, so the
diagrams follow the site's light/dark toggle too.

Usage: scripts/gen-diagrams.py            # writes docs/assets/*.svg
Then copy to website/public/assets/ (the website build does this itself via
website/scripts/sync-content.ts).
"""

from __future__ import annotations

import sys
from html import escape
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "docs" / "assets"

STYLE = """
.pbx{--k-surface:#e6e9ef;--k-base:#eff1f5;--k-border:#acb0be;--k-faint:#ccd0da;--k-text:#4c4f69;--k-sub:#5c5f77;--k-muted:#8c8fa1;--k-mauve:#8839ef;--k-blue:#1e66f5;--k-green:#40a02b;--k-peach:#fe640b}
@media (prefers-color-scheme:dark){.pbx{--k-surface:#181825;--k-base:#1e1e2e;--k-border:#585b70;--k-faint:#313244;--k-text:#cdd6f4;--k-sub:#a6adc8;--k-muted:#7f849c;--k-mauve:#cba6f7;--k-blue:#89b4fa;--k-green:#a6e3a1;--k-peach:#fab387}}
.pbx{--c-surface:var(--bg-mantle,var(--k-surface));--c-base:var(--bg-base,var(--k-base));--c-border:var(--border-focus,var(--k-border));--c-faint:var(--border,var(--k-faint));--c-text:var(--text-primary,var(--k-text));--c-sub:var(--text-subtext,var(--k-sub));--c-muted:var(--text-muted,var(--k-muted));--c-mauve:var(--accent-mauve,var(--k-mauve));--c-blue:var(--accent-blue,var(--k-blue));--c-green:var(--accent-green,var(--k-green));--c-peach:var(--accent-peach,var(--k-peach))}
.pbx text{font-family:Geist,Inter,-apple-system,'Segoe UI',Roboto,Ubuntu,sans-serif;fill:var(--c-text)}
.pbx .mono{font-family:'JetBrains Mono',ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;font-variant-ligatures:none}
.pbx .t{font-size:13px;font-weight:600}
.pbx .s{font-size:10.5px;fill:var(--c-sub)}
.pbx .m{font-size:10.5px;fill:var(--c-muted)}
.pbx .lbl{font-size:11px;fill:var(--c-muted);letter-spacing:.02em}
.pbx .num{font-size:11px;font-weight:600;fill:var(--c-base)}
.pbx .box{fill:var(--c-surface);stroke:var(--c-border);stroke-width:1}
.pbx .opt{stroke-dasharray:4 3}
.pbx .acc-mauve{fill:color-mix(in srgb,var(--c-mauve) 13%,var(--c-surface));stroke:var(--c-mauve)}
.pbx .acc-blue{fill:color-mix(in srgb,var(--c-blue) 13%,var(--c-surface));stroke:var(--c-blue)}
.pbx .acc-peach{fill:color-mix(in srgb,var(--c-peach) 13%,var(--c-surface));stroke:var(--c-peach)}
.pbx .acc-green{fill:color-mix(in srgb,var(--c-green) 13%,var(--c-surface));stroke:var(--c-green)}
.pbx .region{fill:color-mix(in srgb,var(--c-faint) 22%,transparent);stroke:var(--c-faint);stroke-width:1}
.pbx .wire{fill:none;stroke:var(--c-muted);stroke-width:1.4}
.pbx .wire.dash{stroke-dasharray:5 4}
.pbx .wire.acc{stroke:var(--c-mauve)}
.pbx .head{fill:var(--c-muted)}
.pbx .head.acc{fill:var(--c-mauve)}
.pbx .boundary{stroke:var(--c-border);stroke-width:1;stroke-dasharray:6 5}
.pbx .life{stroke:var(--c-faint);stroke-width:1.2;stroke-dasharray:3 4}
.pbx .dot{fill:var(--c-mauve)}
"""


class Svg:
    def __init__(self, ident: str, w: int, h: int, title: str, desc: str):
        self.id, self.w, self.h = ident, w, h
        self.title, self.desc = title, desc
        self.parts: list[str] = []

    # ── primitives ────────────────────────────────────────────────────────
    def add(self, s: str) -> None:
        self.parts.append(s)

    def text(self, x, y, s, cls="s", anchor="start", mono=False):
        c = f"{cls} mono" if mono else cls
        self.add(f'<text x="{x}" y="{y}" class="{c}" text-anchor="{anchor}">{escape(s)}</text>')

    def box(self, x, y, w, h, title, sub=None, kind="box", mono_title=False,
            mono_sub=True, optional=False, align="middle", lines=None, inset=14):
        cls = "box" + ("" if kind == "box" else f" {kind}") + (" opt" if optional else "")
        self.add(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="7" class="{cls}"/>')
        tx = x + w / 2 if align == "middle" else x + inset
        body = [sub] if sub else []
        body += lines or []
        if not body:
            self.text(tx, y + h / 2 + 4.5, title, "t", align, mono_title)
            return
        total = 16 + 15 * len(body)
        ty = y + (h - total) / 2 + 12
        self.text(tx, ty, title, "t", align, mono_title)
        for i, line in enumerate(body):
            self.text(tx, ty + 17 + i * 15, line, "s", align, mono_sub)

    def region(self, x, y, w, h, label):
        self.add(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="10" class="region"/>')
        self.text(x + 14, y + 20, label, "lbl", mono=True)

    def wire(self, pts, dashed=False, accent=False, head=True, both=False):
        d = "M " + " L ".join(f"{px} {py}" for px, py in pts)
        cls = "wire" + (" dash" if dashed else "") + (" acc" if accent else "")
        m = f"{self.id}-h{'a' if accent else ''}"
        end = f' marker-end="url(#{m})"' if head else ""
        start = f' marker-start="url(#{m})"' if both else ""
        self.add(f'<path d="{d}" class="{cls}"{end}{start}/>')

    def num(self, cx, cy, n):
        self.add(f'<circle cx="{cx}" cy="{cy}" r="10" class="dot"/>')
        self.text(cx, cy + 4, str(n), "num", "middle")

    # ── output ────────────────────────────────────────────────────────────
    def render(self) -> str:
        defs = "".join(
            f'<marker id="{self.id}-h{s}" viewBox="0 0 10 10" refX="9" refY="5" '
            f'markerWidth="7" markerHeight="7" orient="auto-start-reverse">'
            f'<path d="M0 1 L10 5 L0 9 z" class="head{" acc" if s else ""}"/></marker>'
            for s in ("", "a")
        )
        return (
            f'<svg xmlns="http://www.w3.org/2000/svg" class="pbx" viewBox="0 0 {self.w} {self.h}" '
            f'width="{self.w}" height="{self.h}" role="img" aria-labelledby="{self.id}-title {self.id}-desc">\n'
            f'<title id="{self.id}-title">{escape(self.title)}</title>\n'
            f'<desc id="{self.id}-desc">{escape(self.desc)}</desc>\n'
            f"<style>{STYLE}</style>\n<defs>{defs}</defs>\n"
            + "\n".join(self.parts)
            + "\n</svg>\n"
        )


# ── diagrams ──────────────────────────────────────────────────────────────

def architecture_build() -> Svg:
    s = Svg("pbx-ab", 960, 300, "podbox at build time",
            "podbox.toml goes into podbox build, which writes a Containerfile that podman builds "
            "into an OCI image, and Quadlet units that are installed for systemd --user.")
    s.box(20, 118, 150, 64, "podbox.toml", "you write this", mono_title=True, mono_sub=False)
    s.box(210, 118, 150, 64, "podbox build", "pure codegen", kind="acc-mauve", mono_title=True, mono_sub=False)
    s.box(410, 40, 170, 64, "Containerfile", "packages, run steps")
    s.box(620, 40, 180, 64, "OCI image", "podman build")
    s.box(410, 196, 170, 64, "Quadlet units", ".container .socket .build")
    s.box(620, 196, 180, 64, "Installed", "~/.config/containers/systemd")
    s.box(840, 118, 104, 64, "systemd", "--user")
    s.wire([(170, 150), (210, 150)])
    s.wire([(360, 150), (385, 150), (385, 72), (410, 72)])
    s.wire([(360, 150), (385, 150), (385, 228), (410, 228)])
    s.wire([(580, 72), (620, 72)])
    s.wire([(580, 228), (620, 228)])
    s.wire([(800, 72), (820, 72), (820, 150), (840, 150)])
    s.wire([(800, 228), (820, 228), (820, 150), (840, 150)])
    s.text(20, 284, "Everything right of podbox.toml is generated. Edit the TOML, run podbox build again.",
           "m")
    return s


def architecture_runtime() -> Svg:
    s = Svg("pbx-ar", 960, 500, "podbox while a container runs",
            "Inside the container, interceptors and the podbox-guest daemon talk to the host over one "
            "Unix socket served by podbox serve. Wayland and D-Bus traffic pass through a Wayland "
            "filter and an optional xdg-dbus-proxy before reaching the desktop.")
    s.region(20, 30, 420, 440, "container")
    s.region(480, 30, 460, 440, "host")
    s.box(40, 70, 380, 56, "Your shell and apps", "runs as you · home is ~/containers/<name>", mono_sub=False)
    s.box(40, 156, 380, 56, "Interceptors on PATH", "notify-send  xdg-open  podbox-clipboard  host-exec")
    s.box(40, 242, 380, 56, "podbox-guest --daemon", "handshake, installs the interceptors",
          kind="acc-mauve", mono_title=True, mono_sub=False)
    s.box(40, 328, 380, 56, "Wayland clients", "$WAYLAND_DISPLAY")
    s.box(40, 396, 380, 56, "D-Bus clients", "$DBUS_SESSION_BUS_ADDRESS", optional=True)

    s.box(500, 156, 230, 56, "<name>.socket", "$XDG_RUNTIME_DIR/podbox/", mono_title=True)
    s.box(500, 242, 230, 56, "podbox serve", "<name>-host.service", kind="acc-mauve", mono_title=True)
    s.box(500, 328, 230, 56, "Wayland filter", "<name>-compositor.service")
    s.box(500, 396, 230, 56, "xdg-dbus-proxy", "<name>-proxy.service", optional=True)
    s.box(770, 156, 150, 296, "Your desktop", lines=[
        "", "compositor", "notifications", "clipboard", "browser", "session bus", "allowed commands", "",
    ])

    s.wire([(230, 126), (230, 156)])
    s.wire([(420, 184), (500, 184)], accent=True)
    s.wire([(420, 270), (460, 270), (460, 196), (500, 196)], accent=True)
    s.wire([(615, 212), (615, 242)])
    s.text(624, 231, "activates", "m")
    s.wire([(730, 270), (770, 270)])
    s.wire([(420, 356), (500, 356)])
    s.wire([(730, 356), (770, 356)])
    s.wire([(420, 424), (500, 424)], dashed=True)
    s.wire([(730, 424), (770, 424)], dashed=True)
    s.text(20, 492, "Dashed: only when enabled in the config. Nothing reaches the host except through these paths.", "m")
    return s


def how_it_works() -> Svg:
    s = Svg("pbx-hw", 960, 330, "How podbox works",
            "Write a TOML file, optionally extending a profile. podbox build generates an OCI image "
            "and Quadlet units. systemd --user then runs the container and its host-side services.")
    s.region(20, 30, 260, 260, "1 · write")
    s.region(310, 30, 320, 260, "2 · build")
    s.region(660, 30, 280, 260, "3 · run")
    s.box(40, 76, 220, 64, "podbox.toml", "image · packages · access", mono_title=True, mono_sub=False)
    s.box(40, 200, 220, 64, "Profile", 'extends = "profile:fedora"', optional=True)
    s.wire([(150, 200), (150, 140)], dashed=True)
    s.text(158, 176, "extends", "m")
    s.box(330, 76, 280, 64, "podbox build", "pure function of the config", kind="acc-mauve",
          mono_title=True, mono_sub=False)
    s.box(330, 200, 132, 64, "OCI image", "packages baked in", mono_sub=False)
    s.box(478, 200, 132, 64, "Quadlet units", ".container .socket")
    s.wire([(470, 140), (470, 170), (396, 170), (396, 200)])
    s.wire([(470, 140), (470, 170), (544, 170), (544, 200)])
    s.box(680, 76, 240, 64, "systemd --user", "starts, restarts, keeps logs", mono_title=True, mono_sub=False)
    s.box(680, 200, 112, 64, "Container", "your shell", mono_sub=False)
    s.box(808, 200, 112, 64, "Host side", "socket, filters", mono_sub=False)
    s.wire([(800, 140), (800, 170), (736, 170), (736, 200)])
    s.wire([(800, 140), (800, 170), (864, 170), (864, 200)])
    s.wire([(792, 232), (808, 232)], both=True)
    s.wire([(260, 108), (330, 108)])
    s.wire([(610, 108), (680, 108)])
    s.text(645, 100, "install", "m", "middle")
    s.text(20, 314, "No podbox process stays running: after the build, it is ordinary systemd units and Podman.", "m")
    return s


def codegen_pipeline() -> Svg:
    rows = [
        ("containerfile::generate", "Containerfile", False),
        ("quadlet::generate_container", "<name>.container", False),
        ("quadlet::generate_build", "<name>.build", False),
        ("quadlet::generate_socket", "<name>.socket", False),
        ("quadlet::generate_host_service", "<name>-host.service", False),
        ("quadlet::generate_compositor_service", "<name>-compositor.service", True),
        ("quadlet::generate_dbus_proxy_service", "<name>-proxy.service", True),
    ]
    top, step = 66, 44
    region_h = 56 + step * len(rows)
    s = Svg("pbx-cg", 960, region_h + 236, "podbox build: codegen pipeline",
            "A parsed and validated Config feeds pure generator functions, each returning one file as "
            "a string. Orchestration then writes the build context, copies the guest binary, runs "
            "podman build and installs the units.")
    s.region(20, 24, 920, region_h, "pure functions · data in, string out, no I/O")
    cy = top + step * (len(rows) - 1) / 2 + 17
    s.box(40, cy - 60, 160, 120, "Config", lines=["parsed", "profiles merged", "validated"], mono_sub=False)
    s.add(f'<line x1="226" y1="{top + 17}" x2="226" y2="{top + step * (len(rows) - 1) + 17}" class="wire"/>')
    s.wire([(200, cy), (226, cy)], head=False)
    for i, (fn, out, opt) in enumerate(rows):
        y = top + i * step
        s.wire([(226, y + 17), (250, y + 17)])
        s.box(250, y, 360, 34, fn + "()", kind="acc-mauve", mono_title=True, optional=opt)
        s.wire([(610, y + 17), (650, y + 17)])
        s.box(650, y, 270, 34, out, mono_title=True, optional=opt)
    oy = 24 + region_h + 30
    s.region(20, oy, 920, 140, "orchestration · writes files, calls podman and systemctl")
    steps = [
        ("Build context", "~/.local/share/podbox/<name>"),
        ("Copy guest", "static musl binary"),
        ("podman build", "skipped when unchanged"),
        ("Install units", "and write the lock file"),
    ]
    for i, (t, sub) in enumerate(steps):
        x = 40 + i * 225
        s.box(x, oy + 44, 200, 64, t, sub, mono_sub=i == 0)
        s.num(x + 2, oy + 44, i + 1)
        if i:
            s.wire([(x - 25, oy + 76), (x, oy + 76)])
    s.text(20, oy + 168, "Dashed rows only run when the config enables the Wayland filter or the D-Bus proxy.", "m")
    return s


def socket_protocol() -> Svg:
    s = Svg("pbx-sp", 960, 470, "Host-guest socket protocol",
            "The guest daemon and every interceptor open a connection to the host socket, send hello "
            "with the capabilities they want, receive hello_ack, then send requests such as notify, "
            "which podbox serve passes to the desktop.")
    lanes = [
        (130, "notify-send", "interceptor"),
        (360, "podbox-guest", "daemon"),
        (610, "podbox serve", "host socket"),
        (850, "Your desktop", "notifications"),
    ]
    for x, t, sub in lanes:
        s.box(x - 95, 24, 190, 56, t, sub, mono_title=x != 850, mono_sub=False,
              kind="acc-mauve" if x == 610 else "box")
        s.add(f'<line x1="{x}" y1="80" x2="{x}" y2="440" class="life"/>')
    s.add('<line x1="490" y1="90" x2="490" y2="440" class="boundary"/>')
    s.text(482, 104, "container", "lbl", "end", True)
    s.text(498, 104, "host", "lbl", "start", True)

    def msg(y, x1, x2, label, sub=None, dashed=False, accent=False):
        s.wire([(x1, y), (x2, y)], dashed=dashed, accent=accent)
        # Label sits just past the sender, so it never lands on a lifeline.
        lx, anchor = (x1 + 14, "start") if x2 > x1 else (x1 - 14, "end")
        s.text(lx, y - 8, label, "s", anchor, True)
        if sub:
            s.text(lx, y + 16, sub, "m", anchor)

    msg(140, 360, 610, "hello", "capabilities it wants", accent=True)
    msg(196, 610, 360, "hello_ack", "accepted · rejected", dashed=True)
    s.add('<rect x="268" y="226" width="184" height="30" rx="6" class="box"/>')
    s.text(360, 245, "installs interceptors", "s", "middle")
    msg(296, 130, 610, "hello", accent=True)
    msg(336, 610, 130, "hello_ack", dashed=True)
    msg(384, 130, 610, "notify", "summary, body, actions", accent=True)
    msg(384, 610, 850, "show")
    s.text(20, 462, "Every frame: 4-byte big-endian length, then that many bytes of JSON. See protocol.md.", "m")
    return s


def runtime_flow() -> Svg:
    steps = [
        ("host", "Login, or podbox start", "systemd --user is running"),
        ("host", "<name>.socket listens", "$XDG_RUNTIME_DIR/podbox/<name>.sock"),
        ("host", "<name>.service starts", "podman run --init, from the .container unit"),
        ("ctr", "catatonit is PID 1", "reaps zombies, forwards signals"),
        ("ctr", "podbox-guest --entry", "first boot: creates your user and home"),
        ("ctr", "Spawns podbox-guest --daemon", "as root, so it can install interceptors"),
        ("ctr", "Drops to your user, execs the shell", "or the command you passed"),
        ("host", "Meanwhile, the daemon connects", "socket activates <name>-host.service"),
        ("ctr", "Hello / HelloAck", "interceptors installed for accepted capabilities"),
    ]
    top, step, bh, bw = 82, 62, 46, 400
    lanes = {"host": 30, "ctr": 530}
    s = Svg("pbx-rf", 960, top + step * len(steps) + 30, "podbox runtime flow",
            "Sequence from login to a ready shell: systemd starts the socket and the container, "
            "catatonit runs podbox-guest --entry, which spawns the daemon, drops privileges and "
            "execs your shell; the daemon connects to the host socket and completes the handshake.")
    s.region(20, 24, 420, top + step * len(steps) - 18, "host · systemd --user")
    s.region(520, 24, 420, top + step * len(steps) - 18, "container")
    prev = None
    for i, (lane, t, sub) in enumerate(steps):
        x, y = lanes[lane], top + i * step
        s.box(x + 10, y, bw - 20, bh, t, sub, align="start", mono_sub=True, inset=44,
              kind="acc-mauve" if i in (4, 8) else "box")
        s.num(x + 32, y + bh / 2, i + 1)
        if prev:
            px, py = prev
            out_y, in_y = py + bh * 0.68, y + bh * 0.32
            if px == x:
                s.wire([(x + bw / 2, py + bh), (x + bw / 2, y)])
            elif px < x:
                s.wire([(px + bw - 10, out_y), (480, out_y), (480, in_y), (x + 10, in_y)])
            else:
                s.wire([(px + 10, out_y), (480, out_y), (480, in_y), (x + bw - 10, in_y)])
        prev = (x, y)
    return s


def wire_format() -> Svg:
    s = Svg("pbx-wf", 860, 270, "Wire format",
            "Each message is a 4-byte big-endian length followed by that many bytes of UTF-8 JSON. "
            "Example: 00 00 00 0F followed by the 15 bytes {\"type\":\"ping\"}.")
    s.text(30, 34, "bytes 0–3", "lbl", mono=True)
    s.text(250, 34, "bytes 4 … N+3", "lbl", mono=True)
    s.box(30, 44, 200, 72, "length = N", "u32, big-endian", kind="acc-blue", mono_title=True)
    s.box(250, 44, 580, 72, "payload", "N bytes of UTF-8 JSON, one message", kind="acc-mauve", mono_sub=False)
    s.text(30, 160, "example", "lbl", mono=True)
    s.box(30, 170, 200, 64, "00 00 00 0F", "= 15", mono_title=True)
    s.box(250, 170, 580, 64, '{"type":"ping"}', "15 bytes", mono_title=True)
    s.text(30, 260, "Frames over 16 MiB are rejected. Both directions use the same framing.", "m")
    return s


DIAGRAMS = {
    "architecture-build": architecture_build,
    "architecture-runtime": architecture_runtime,
    "how_it_works": how_it_works,
    "codegen_pipeline": codegen_pipeline,
    "socket_protocol": socket_protocol,
    "runtime_flow": runtime_flow,
    "wire_format": wire_format,
}


def main(argv: list[str]) -> int:
    out = Path(argv[1]) if len(argv) > 1 else OUT
    out.mkdir(parents=True, exist_ok=True)
    for name, fn in DIAGRAMS.items():
        (out / f"{name}.svg").write_text(fn().render(), encoding="utf-8")
        print(f"wrote {out / name}.svg")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
