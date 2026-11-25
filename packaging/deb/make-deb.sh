#!/usr/bin/env bash
# Runs inside the image of Containerfile.deb. `packaging/container/build-deb.sh` starts it.
# Usage: packaging/deb/make-deb.sh <output-dir>    (run from the source tree root; JOBS sets the cargo jobs)
# Builds the release binaries, stages the files of nicecooling.spec at the Debian paths and builds the deb.
set -euo pipefail

out=$1
jobs=${JOBS:-8}
cd "$(dirname "$0")/../.."
src=$PWD
# A tag like 0.2.0-beta.1 becomes 0.2.0~beta.1, which sorts before 0.2.0.
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1 | tr - '~')
[ -n "$version" ] || { echo "make-deb.sh: no workspace version in Cargo.toml" >&2; exit 1; }
revision=1
deb="nicecooling_${version}-${revision}_amd64.deb"

cargo build --release --locked -j "$jobs" -p nicecoolingd -p nicecooling

stage=$(mktemp -d)
chmod 0755 "$stage"
trap 'rm -rf "$stage"' EXIT
install -D -s -m 0755 target/release/nicecooling "$stage/usr/bin/nicecooling"
install -D -s -m 0755 target/release/nicecoolingd "$stage/usr/libexec/nicecoolingd"
install -D -m 0644 packaging/nicecoolingd.service "$stage/usr/lib/systemd/system/nicecoolingd.service"
install -D -m 0644 packaging/80-nicecooling.preset "$stage/usr/lib/systemd/system-preset/80-nicecooling.preset"
install -D -m 0644 packaging/poweroff.target.d/50-nicecooling.conf \
  "$stage/usr/lib/systemd/system/poweroff.target.d/50-nicecooling.conf"
install -D -m 0644 packaging/dbus/io.github.cappyt.NiceCooling1.conf \
  "$stage/usr/share/dbus-1/system.d/io.github.cappyt.NiceCooling1.conf"
install -D -m 0644 packaging/polkit/io.github.cappyt.NiceCooling1.policy \
  "$stage/usr/share/polkit-1/actions/io.github.cappyt.NiceCooling1.policy"
# The administrator group of Debian and Ubuntu is `sudo`. The group `wheel` does not exist there.
install -D -m 0644 packaging/polkit/50-nicecooling.rules "$stage/usr/share/polkit-1/rules.d/50-nicecooling.rules"
sed -i 's/\bwheel\b/sudo/g' "$stage/usr/share/polkit-1/rules.d/50-nicecooling.rules"
grep -q 'isInGroup("sudo")' "$stage/usr/share/polkit-1/rules.d/50-nicecooling.rules"
if grep -q wheel "$stage/usr/share/polkit-1/rules.d/50-nicecooling.rules"; then
  echo "make-deb.sh: the polkit rule still names wheel" >&2
  exit 1
fi
install -D -m 0644 packaging/io.github.cappyt.NiceCooling.autostart.desktop \
  "$stage/etc/xdg/autostart/io.github.cappyt.NiceCooling.desktop"
install -D -m 0644 packaging/io.github.cappyt.NiceCooling.desktop \
  "$stage/usr/share/applications/io.github.cappyt.NiceCooling.desktop"
install -D -m 0644 docs/ui/icons/nicecooling.svg \
  "$stage/usr/share/icons/hicolor/scalable/apps/io.github.cappyt.NiceCooling.svg"
install -d "$stage/usr/share/icons/hicolor/symbolic/apps" "$stage/usr/share/nicecooling/boards" \
  "$stage/usr/share/nicecooling/calibration"
install -m 0644 docs/ui/icons/nicecooling-*symbolic.svg "$stage/usr/share/icons/hicolor/symbolic/apps/"
install -m 0644 data/boards/*.toml "$stage/usr/share/nicecooling/boards/"
install -m 0644 data/calibration/*.toml "$stage/usr/share/nicecooling/calibration/"

install -D -m 0644 README.md "$stage/usr/share/doc/nicecooling/README.md"
install -D -m 0644 packaging/deb/copyright "$stage/usr/share/doc/nicecooling/copyright"
{
  printf 'nicecooling (%s-%s) unstable; urgency=medium\n\n  * Package built from the upstream tree.\n\n' "$version" "$revision"
  printf ' -- NiceCooling <nicecooling@localhost.invalid>  %s\n' "$(date -u -R -d "@${SOURCE_DATE_EPOCH:-$(date +%s)}")"
} | gzip -9n > "$stage/usr/share/doc/nicecooling/changelog.Debian.gz"
chmod 0644 "$stage/usr/share/doc/nicecooling/changelog.Debian.gz"

# Depends: the libraries that the binaries link, and what the spec needs besides them.
# dpkg-shlibdeps reads debian/control of the current directory, an empty file is enough.
shl=$(mktemp -d)
mkdir "$shl/debian"
: > "$shl/debian/control"
shlibs=$(cd "$shl" && dpkg-shlibdeps -O -e"$stage/usr/bin/nicecooling" -e"$stage/usr/libexec/nicecoolingd" \
  | sed -n 's/^shlibs:Depends=//p')
rm -rf "$shl"
[ -n "$shlibs" ] || { echo "make-deb.sh: dpkg-shlibdeps found no dependency" >&2; exit 1; }
# The app loads these libraries at run time, so dpkg-shlibdeps does not find them.
depends="$shlibs, dbus-system-bus-common, hicolor-icon-theme, init-system-helpers (>= 1.52), polkitd, systemd, \
libegl1, libfontconfig1, libvulkan1, libwayland-client0, libwayland-cursor0, libwayland-egl1"

install -d -m 0755 "$stage/DEBIAN"
install -m 0755 packaging/deb/postinst packaging/deb/prerm packaging/deb/postrm "$stage/DEBIAN/"
echo /etc/xdg/autostart/io.github.cappyt.NiceCooling.desktop > "$stage/DEBIAN/conffiles"
(cd "$stage" && find usr etc -type f -exec md5sum {} + | sort -k 2 > DEBIAN/md5sums)
size=$(du -sk --exclude=DEBIAN "$stage" | cut -f 1)
cat > "$stage/DEBIAN/control" <<CONTROL
Package: nicecooling
Version: ${version}-${revision}
Architecture: amd64
Maintainer: NiceCooling <nicecooling@localhost.invalid>
Installed-Size: ${size}
Depends: ${depends}
Section: utils
Priority: optional
Homepage: https://github.com/CappyT/nicecooling
Description: Fan, pump and Leakshield control for a water-cooled desktop
 nicecooling controls the fans and the pump of a water-cooled desktop through
 the hardware monitoring interface of the Linux kernel, and it reads and
 writes the Leakshield. The package holds the daemon nicecoolingd, which runs
 as a system service, and the nicecooling app, which shows a tray icon and a
 window.
CONTROL

mkdir -p "$out"
rm -f "$out"/nicecooling_*.deb
dpkg-deb --root-owner-group -Zxz --build "$stage" "$out/$deb"
echo "$out/$deb"
