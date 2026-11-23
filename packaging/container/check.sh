#!/usr/bin/env bash
# Checks the systemd unit of the daemon in a Fedora container with systemd as PID 1.
# Run from the repository root after `cargo build --release -p nicecoolingd`.
# The daemon itself never runs: a drop-in replaces ExecStart with a probe that sends READY.
# `WatchdogUSec` reads `infinity` while the unit is inactive, so the properties are read from an active unit.
set -euo pipefail

name=nc-unit-check
podman build -f packaging/container/Containerfile -t localhost/nicecooling-unit .
podman rm -f "$name" >/dev/null 2>&1 || true
trap 'podman rm -f "$name" >/dev/null 2>&1 || true' EXIT
podman run -d --name "$name" --systemd=always localhost/nicecooling-unit >/dev/null

podman exec "$name" systemd-analyze verify /usr/lib/systemd/system/nicecoolingd.service
echo "verify: exit 0"

podman exec "$name" sh -c '
  mkdir -p /etc/systemd/system/nicecoolingd.service.d
  printf "[Service]\nNotifyAccess=all\nExecStart=\nExecStart=/bin/sh -c \"systemd-notify --ready; exec sleep infinity\"\nExecStopPost=\nExecStopPost=/bin/true\n" \
    > /etc/systemd/system/nicecoolingd.service.d/probe.conf
  systemctl daemon-reload
  systemctl start nicecoolingd
  systemctl show nicecoolingd -p WatchdogUSec -p TimeoutAbortUSec -p Restart -p RestartUSec \
    -p StartLimitBurst -p StartLimitIntervalUSec -p StateDirectoryMode
' | sort > /tmp/nicecooling-unit-actual.txt

cat > /tmp/nicecooling-unit-expected.txt <<'EXPECTED'
Restart=on-failure
RestartUSec=2s
StartLimitBurst=5
StartLimitIntervalUSec=1min
StateDirectoryMode=0700
TimeoutAbortUSec=5s
WatchdogUSec=10s
EXPECTED
diff /tmp/nicecooling-unit-expected.txt /tmp/nicecooling-unit-actual.txt
echo "show: properties match"
