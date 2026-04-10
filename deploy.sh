#!/usr/bin/env bash
set -euo pipefail

HOST="vm-gtfs"
REMOTE_BIN="/usr/local/bin"
REMOTE_UNIT="/etc/systemd/system"
REMOTE_STATE="/var/lib/gtfs-realtime-tbilisi"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

echo "==> Building release binaries…"
cargo build --release --manifest-path "$SCRIPT_DIR/Cargo.toml"

TARGET_DIR="$SCRIPT_DIR/target/release"

echo "==> Copying binaries to $HOST…"
scp "$TARGET_DIR/gtfs-static" "$TARGET_DIR/gtfs-realtime" "$HOST:/tmp/"
ssh "$HOST" "install -m 755 /tmp/gtfs-static /tmp/gtfs-realtime $REMOTE_BIN/ && rm /tmp/gtfs-static /tmp/gtfs-realtime"

echo "==> Copying systemd units to $HOST…"
scp "$SCRIPT_DIR/gtfs-realtime-tbilisi.service" \
    "$SCRIPT_DIR/gtfs-update-tbilisi.service" \
    "$SCRIPT_DIR/gtfs-update-tbilisi.timer" \
    "$HOST:/tmp/"
ssh "$HOST" "install -m 644 /tmp/gtfs-realtime-tbilisi.service /tmp/gtfs-update-tbilisi.service /tmp/gtfs-update-tbilisi.timer $REMOTE_UNIT/ && rm /tmp/gtfs-realtime-tbilisi.service /tmp/gtfs-update-tbilisi.service /tmp/gtfs-update-tbilisi.timer"

echo "==> Ensuring state directory and initial feed exist…"
ssh "$HOST" "mkdir -p $REMOTE_STATE && [ -f $REMOTE_STATE/gtfs.zip ] || curl -fL --retry 3 -o $REMOTE_STATE/gtfs.zip https://jbb.ghsq.de/gtfs/ge-tbilisi.gtfs.zip"

echo "==> Reloading systemd and (re)starting services…"
ssh "$HOST" "systemctl daemon-reload \
    && systemctl enable --now gtfs-update-tbilisi.timer \
    && systemctl restart gtfs-realtime-tbilisi.service"

echo "==> Verifying…"
ssh "$HOST" "systemctl is-active gtfs-realtime-tbilisi.service && systemctl is-enabled gtfs-update-tbilisi.timer"

echo "==> Done."
