#!/usr/bin/env bash
# Checks the DEB in clean containers of Ubuntu 24.04 and Debian 13. Run from anywhere: it changes to the repository root.
# Usage: packaging/container/check-deb.sh [deb-file]    (default: the one file in target/deb)
# In each container: `apt-get install ./file.deb`, the files and the unit link, the daemon run on an empty fake sysfs,
# the linked libraries, lintian, then removal and purge. There is no systemd as PID 1, so the unit does not start here.
# `nicecoolingd` and `nicecooling` have no `--version`, and the app needs a display. The nearest checks that need none:
# `nicecoolingd --restore-bios` on an empty fake tree, and `ldd` on both binaries.
# The container gets network for apt only.
set -euo pipefail

cd "$(dirname "$0")/../.."
if [ $# -ge 1 ]; then
  deb=$1
else
  mapfile -t found < <(find target/deb -maxdepth 1 -name 'nicecooling_*.deb')
  [ "${#found[@]}" -eq 1 ] || { echo "target/deb holds ${#found[@]} debs, expected 1: ${found[*]}" >&2; exit 1; }
  deb=${found[0]}
fi
deb=$(readlink -f "$deb")
echo "deb: $deb"
# A pre-release 0.2.0-beta.1 of Cargo.toml is the DEB version 0.2.0~beta.1-1, which sorts before 0.2.0.
cargo_version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
export EXPECTED_VERSION="${cargo_version//-/\~}-1"

check() {
  local image=$1
  echo "=== $image"
  podman run --rm -i -e EXPECTED_VERSION -v "$deb":/in/nicecooling.deb:ro,Z "$image" bash -s <<'INNER'
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
. /etc/os-release
echo "os: $PRETTY_NAME"
apt-get update -qq >/dev/null
# apt reads the file as the user _apt, which cannot read the mount.
install -m 0644 /in/nicecooling.deb /tmp/nicecooling.deb
apt-get install -y --no-install-recommends /tmp/nicecooling.deb >/dev/null
echo "apt-get install ./nicecooling.deb: exit 0"
version=$(dpkg-query -W -f='${Version}' nicecooling)
[ "$version" = "$EXPECTED_VERSION" ] || { echo "deb version '$version', expected '$EXPECTED_VERSION'" >&2; exit 1; }
echo "deb version: $version"

paths="/usr/bin/nicecooling /usr/libexec/nicecoolingd
/usr/lib/systemd/system/nicecoolingd.service /usr/lib/systemd/system-preset/80-nicecooling.preset
/usr/lib/systemd/system/poweroff.target.d/50-nicecooling.conf
/usr/share/dbus-1/system.d/io.github.cappyt.NiceCooling1.conf
/usr/share/polkit-1/actions/io.github.cappyt.NiceCooling1.policy /usr/share/polkit-1/rules.d/50-nicecooling.rules
/etc/xdg/autostart/io.github.cappyt.NiceCooling.desktop /usr/share/applications/io.github.cappyt.NiceCooling.desktop
/usr/share/icons/hicolor/scalable/apps/io.github.cappyt.NiceCooling.svg
/usr/share/nicecooling/boards /usr/share/nicecooling/calibration"
for p in $paths; do test -e "$p" || { echo "missing: $p" >&2; exit 1; }; done
ls /usr/share/icons/hicolor/symbolic/apps/nicecooling-*symbolic.svg >/dev/null
ls /usr/share/nicecooling/boards/*.toml /usr/share/nicecooling/calibration/*.toml >/dev/null
echo "files: all present"

mode=$(stat -c %a /var/lib/nicecooling)
[ "$mode" = 700 ] || { echo "/var/lib/nicecooling has mode $mode, expected 700" >&2; exit 1; }
echo "/var/lib/nicecooling: mode $mode"
test -L /etc/systemd/system/multi-user.target.wants/nicecoolingd.service
echo "unit: enabled (multi-user.target.wants link)"

grep -q 'isInGroup("sudo")' /usr/share/polkit-1/rules.d/50-nicecooling.rules
! grep -q wheel /usr/share/polkit-1/rules.d/50-nicecooling.rules
echo "polkit rule: group sudo, no wheel"

for bin in /usr/bin/nicecooling /usr/libexec/nicecoolingd; do
  if ldd "$bin" | grep 'not found'; then echo "$bin has unresolved libraries" >&2; exit 1; fi
done
echo "ldd: no unresolved library"
tmp=$(mktemp -d)
mkdir -p "$tmp/sysfs/class/hwmon"
/usr/libexec/nicecoolingd --restore-bios --sysfs-root "$tmp/sysfs" --state-dir "$tmp/state"
echo "nicecoolingd --restore-bios on an empty fake tree: exit 0"

apt-get install -y --no-install-recommends lintian >/dev/null
lintian --version
lintian_rc=0
lintian /tmp/nicecooling.deb || lintian_rc=$?
echo "lintian: exit $lintian_rc"
[ "$lintian_rc" -eq 0 ] || exit "$lintian_rc"

apt-get remove -y nicecooling >/dev/null
# dpkg keeps a conffile at `remove` and deletes it at `purge`.
conffile=/etc/xdg/autostart/io.github.cappyt.NiceCooling.desktop
for p in $paths; do
  [ "$p" = "$conffile" ] && continue
  ! test -e "$p" || { echo "remains after remove: $p" >&2; exit 1; }
done
test -e "$conffile"
! test -L /etc/systemd/system/multi-user.target.wants/nicecoolingd.service
test -d /var/lib/nicecooling
echo "remove: install paths gone, the conffile and /var/lib/nicecooling remain, unit link gone"
apt-get install -y --no-install-recommends /tmp/nicecooling.deb >/dev/null
test -L /etc/systemd/system/multi-user.target.wants/nicecoolingd.service
echo "reinstall: unit enabled again"
apt-get purge -y nicecooling >/dev/null
! test -e "$conffile"
! test -e /etc/systemd/system/nicecoolingd.service
! test -L /etc/systemd/system/multi-user.target.wants/nicecoolingd.service
echo "purge: exit 0, no conffile, unit link or mask left"
INNER
}

check docker.io/library/ubuntu:24.04
check docker.io/library/debian:13
echo "check-deb: OK"
