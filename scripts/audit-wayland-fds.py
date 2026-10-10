#!/usr/bin/env python3
"""Audit Wayland protocols for file-descriptor-bearing messages.

The compositor proxy must attach each SCM_RIGHTS descriptor to the exact
message that sent it. That requires knowing *which* (interface, opcode)
pairs carry an fd — and hand-maintaining that list is how shm, keymap, and
dmabuf-feedback each killed an app exactly once.

Usage:
    scripts/audit-wayland-fds.py [--check]

Without flags it prints the complete bearer table: every request/event in
wayland.xml + wayland-protocols whose args include type="fd", with opcodes
(numbered per interface/kind, append-only upstream).

With --check it diffs that table against the BEARERS table in
crates/podbox/src/compositor/tracker.rs and fails on any gap. Re-run after
wayland-protocols updates; add entries + socket-pair tests for anything new.

Only stdlib. No output files.
"""

import glob
import re
import sys
import xml.etree.ElementTree as ET

TRACKER = "crates/podbox/src/compositor/tracker.rs"


def scan():
    """Return {(interface, kind, opcode, name)} for every fd-bearing message."""
    found = set()
    files = ["/usr/share/wayland/wayland.xml"] + sorted(
        glob.glob("/usr/share/wayland-protocols/**/*.xml", recursive=True)
    )
    for path in files:
        try:
            root = ET.parse(path).getroot()
        except ET.ParseError:
            continue
        for iface in root.iter("interface"):
            iname = iface.get("name")
            for kind in ("request", "event"):
                for opcode, msg in enumerate(iface.iter(kind)):
                    if any(a.get("type") == "fd" for a in msg.iter("arg")):
                        found.add((iname, kind, opcode, msg.get("name")))
    return found


def code_table():
    """Parse the BEARERS table out of tracker.rs.

    Expected line shape (one per entry, exact):
        ("iface", OP, true|false[, COUNT]),
    """
    pat = re.compile(r'^\s*\("([^"]+)",\s*(\d+),\s*(true|false)(?:,\s*\d+)?\),')
    entries = set()
    with open(TRACKER) as fh:
        for line in fh:
            m = pat.match(line)
            if m:
                iface, opcode, direction = m.groups()
                kind = "request" if direction == "true" else "event"
                entries.add((iface, kind, int(opcode)))
    return entries


def main():
    scanned = scan()
    if "--check" not in sys.argv:
        print(f"{'interface':45} {'kind':8} {'op':3} name")
        for iface, kind, opcode, name in sorted(scanned):
            print(f"{iface:45} {kind:8} {opcode:<3} {name}")
        print(f"\n{len(scanned)} fd-bearing messages total.")
        return 0
    coded = code_table()
    scanned_keys = {(i, k, o) for i, k, o, _ in scanned}
    missing = sorted(scanned_keys - coded)
    if missing:
        print("BEARER TABLE GAPS (in protocols, missing from tracker.rs):")
        for iface, kind, opcode in missing:
            names = sorted(n for i, k, o, n in scanned if (i, k, o) == (iface, kind, opcode))
            print(f"  {iface} {kind} #{opcode} ({', '.join(names)})")
        return 1
    print(f"OK: tracker covers all {len(scanned_keys)} fd-bearing messages.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
