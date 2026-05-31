#!/bin/bash
# podmgr container entrypoint
# Installed at /usr/local/bin/podmgr-entry in the image.
# Launches the podmgr-guest daemon before executing the specified command.

set -e

if [ $# -eq 0 ]; then
    echo "Usage: podmgr-entry <command> [args...]"
    exit 1
fi

# Start the guest daemon in background
podmgr-guest --daemon &
DAEMON_PID=$!

# Wait for daemon to be ready (socket)
for i in $(seq 1 10); do
    if [ -S "$XDG_RUNTIME_DIR/podmgr/guest.sock" ]; then
        break
    fi
    sleep 0.2
done

# Execute the requested command
"$@"
EXIT_CODE=$?

# Cleanup daemon
kill $DAEMON_PID 2>/dev/null || true

exit $EXIT_CODE
