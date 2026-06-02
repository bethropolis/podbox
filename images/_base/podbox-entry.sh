#!/bin/bash
set -euo pipefail

# ============================================================
# podbox — Container Entry
# ============================================================

DIR=/run/podbox/bin
export PATH="$DIR:$PATH"

# Wait for the host to start and expose the mount
# (the directory is bind-mounted into the container; Podman
# creates it lazily so we must spin until /run/podbox exists)
for i in $(seq 1 10); do
    if [ -d /run/podbox ]; then
        break
    fi
    sleep 0.1
done

if ! [ -d /run/podbox ]; then
    echo "podbox-entry: /run/podbox did not appear after 1s — is the host tool installed?" >&2
    exit 1
fi

# The host binary is expected inside /run/podbox/bin/
if ! [ -f "$DIR/podbox-host" ]; then
    echo "podbox-entry: $DIR/podbox-host not found" >&2
    exit 1
fi

exec "$@"
