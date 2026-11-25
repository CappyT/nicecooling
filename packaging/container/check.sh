#!/usr/bin/env bash
# Builds the RPM and checks it in containers. Run from anywhere: it changes to the repository root.
#   1. `build-rpm.sh` builds the RPM with `rpmbuild --network none`.
#   2. The RPM content: exactly one rpm file, the mode of /var/lib/nicecooling, rpmlint.
#   3. A Fedora container with systemd as PID 1 installs the RPM, starts the daemon, reads the unit values,
#      stops the daemon and removes the RPM.
# JOBS sets the cargo jobs of the build (default 8). SKIP_BUILD=1 reuses the RPM in target/rpm.
# The daemon runs on an empty fake sysfs and with no Leakshield, so it never reads the sysfs or the USB bus of the host.
set -euo pipefail

cd "$(dirname "$0")/../.."
rpmdir=target/rpm
build_image=localhost/nicecooling-rpm-build
name=nc-rpm-check

if [ "${SKIP_BUILD:-0}" != 1 ]; then
  packaging/container/build-rpm.sh "$rpmdir"
fi

# 1. Content of the RPM.
mapfile -t files < <(find "$rpmdir/RPMS" -type f)
[ "${#files[@]}" -eq 1 ] || { echo "RPMS/ holds ${#files[@]} files, expected 1: ${files[*]}" >&2; exit 1; }
rpm_file=${files[0]}
case "$(basename "$rpm_file")" in
  nicecooling-*.rpm) echo "RPMS/: exactly one file: $(basename "$rpm_file")" ;;
  *) echo "unexpected rpm file name: $rpm_file" >&2; exit 1 ;;
esac

# A pre-release 0.2.0-beta.1 of Cargo.toml is the RPM version 0.2.0~beta.1, which sorts before 0.2.0.
cargo_version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
rpm_version=$(podman run --rm -v "$PWD/$rpmdir":/rpm:ro,Z "$build_image" \
  rpm -qp --qf '%{VERSION}' "/rpm/RPMS/$(basename "$(dirname "$rpm_file")")/$(basename "$rpm_file")")
[ "$rpm_version" = "${cargo_version//-/\~}" ] \
  || { echo "rpm version '$rpm_version' does not match Cargo.toml version '$cargo_version'" >&2; exit 1; }
echo "rpm version: $rpm_version (Cargo.toml: $cargo_version)"

mode=$(podman run --rm -v "$PWD/$rpmdir":/rpm:ro,Z "$build_image" \
  sh -c "rpm -qp --dump /rpm/RPMS/$(basename "$(dirname "$rpm_file")")/$(basename "$rpm_file")" \
  | awk '$1 == "/var/lib/nicecooling" { print $5 }')
[ "$mode" = 040700 ] || { echo "/var/lib/nicecooling has mode '$mode', expected 040700" >&2; exit 1; }
echo "rpm -qp --dump: /var/lib/nicecooling mode $mode"

lint=$(podman run --rm -v "$PWD/$rpmdir":/rpm:ro,Z "$build_image" \
  sh -c "rpmlint /rpm/RPMS/$(basename "$(dirname "$rpm_file")")/$(basename "$rpm_file")" || true)
echo "$lint" | grep -E '^[a-z].*: [EW]: |packages and'
echo "$lint" | grep -qE '^ *[0-9]+ packages and [0-9]+ specfiles checked; 0 errors,'
echo "rpmlint: 0 errors"

# 2. Install, run, stop and remove in a container with systemd.
podman build -f packaging/container/Containerfile -t localhost/nicecooling-unit .
podman rm -f "$name" >/dev/null 2>&1 || true
trap 'podman rm -f "$name" >/dev/null 2>&1 || true' EXIT
podman run -d --name "$name" --systemd=always localhost/nicecooling-unit >/dev/null
podman exec "$name" sh -c 'until [ "$(systemctl is-system-running 2>/dev/null)" = running ] || [ "$(systemctl is-system-running 2>/dev/null)" = degraded ]; do sleep 0.5; done'

podman cp "$rpm_file" "$name":/tmp/nicecooling.rpm
podman exec "$name" dnf -y install /tmp/nicecooling.rpm >/dev/null
podman exec "$name" rpm -ql nicecooling | sort > "$rpmdir/installed-paths.txt"
echo "installed: $(wc -l < "$rpmdir/installed-paths.txt") paths"

# The preset of the package enables the unit, so the daemon starts at the next boot.
enabled=$(podman exec "$name" systemctl is-enabled nicecoolingd.service || true)
[ "$enabled" = enabled ] || { echo "nicecoolingd.service is '$enabled' after the install, expected enabled" >&2; exit 1; }
echo "install: unit enabled by the preset"

podman exec "$name" systemd-analyze verify /usr/lib/systemd/system/nicecoolingd.service
echo "verify nicecoolingd.service: exit 0"
# The Fedora image has no man pages, and `verify` fails on the `man:` links of poweroff.target.
podman exec "$name" systemd-analyze verify --man=no poweroff.target
echo "verify poweroff.target: exit 0"

# The drop-in only points the daemon at an empty fake sysfs (no Board, no Channel). The unit values stay those of the package.
podman exec "$name" sh -c '
  mkdir -p /run/nc-sysfs/class/hwmon /etc/systemd/system/nicecoolingd.service.d
  printf "[Service]\nExecStart=\nExecStart=/usr/libexec/nicecoolingd --sysfs-root /run/nc-sysfs --no-leakshield\nExecStopPost=\nExecStopPost=/usr/libexec/nicecoolingd --restore-bios --sysfs-root /run/nc-sysfs\n" \
    > /etc/systemd/system/nicecoolingd.service.d/test.conf
  systemctl daemon-reload
  systemctl start nicecoolingd
  systemctl show nicecoolingd -p ActiveState
'
podman exec "$name" sh -c '
  systemctl show nicecoolingd -p WatchdogUSec -p TimeoutAbortUSec -p Restart -p RestartUSec \
    -p StartLimitBurst -p StartLimitIntervalUSec -p StateDirectoryMode -p Type -p ActiveState
' | sort > "$rpmdir/unit-actual.txt"
cat > "$rpmdir/unit-expected.txt" <<'EXPECTED'
ActiveState=active
Restart=on-failure
RestartUSec=2s
StartLimitBurst=5
StartLimitIntervalUSec=1min
StateDirectoryMode=0700
TimeoutAbortUSec=5s
Type=notify
WatchdogUSec=10s
EXPECTED
diff "$rpmdir/unit-expected.txt" "$rpmdir/unit-actual.txt"
echo "show nicecoolingd: properties match"
echo "bus name: $(podman exec "$name" busctl --system list --no-legend 2>&1 | grep -c '^io.github.cappyt.NiceCooling1 ') owner(s) of io.github.cappyt.NiceCooling1 (info only)"

podman exec "$name" systemctl show poweroff.target -p JobTimeoutUSec -p JobTimeoutAction \
  | sort > "$rpmdir/poweroff-actual.txt"
cat > "$rpmdir/poweroff-expected.txt" <<'EXPECTED'
JobTimeoutAction=poweroff-force
JobTimeoutUSec=55s
EXPECTED
diff "$rpmdir/poweroff-expected.txt" "$rpmdir/poweroff-actual.txt"
echo "show poweroff.target: properties match"

echo "stat /var/lib/nicecooling: $(podman exec "$name" stat -c %a /var/lib/nicecooling)"
[ "$(podman exec "$name" stat -c %a /var/lib/nicecooling)" = 700 ]

podman exec "$name" systemctl stop nicecoolingd
echo "systemctl stop nicecoolingd: exit 0"

podman exec "$name" rpm -e nicecooling
echo "rpm -e nicecooling: exit 0"
left=0
while IFS= read -r path; do
  [ "$path" = /var/lib/nicecooling ] && continue
  if podman exec "$name" test -e "$path"; then echo "remains after rpm -e: $path" >&2; left=1; fi
done < "$rpmdir/installed-paths.txt"
[ "$left" -eq 0 ]
echo "after rpm -e: no install path exists"
podman exec "$name" test -d /var/lib/nicecooling
echo "after rpm -e: /var/lib/nicecooling remains: $(podman exec "$name" ls /var/lib/nicecooling | tr '\n' ' ')"

# 3. README.md (task 9.4): run its install, watchdog and rollback commands as written in a clean container.
# The commands come out of the README text, so a change of the README changes the check. The build step and the app
# start need podman and a display, so they do not run here. The drop-in only points the daemon at the fake sysfs.
grep -q '^## Install' README.md
grep -q '^## Hardware watchdog' README.md && grep -q 'RuntimeWatchdogSec' README.md
grep -q '^## Rollback' README.md && grep -q 'openspec/changes/add-nicecooling-v1/design.md' README.md
readme_cmd() { grep -oP "(?<=$1: \`)[^\`]+" README.md; }
install_cmd=$(readme_cmd 'Install it')
enable_cmd=$(readme_cmd 'Start the daemon')
check_cmd=$(readme_cmd 'Check it' | head -n 1)
stop_cmd=$(grep -oP '(?<=Stop the service: `)[^`]+' README.md)
remove_cmd=$(grep -oP '(?<=Remove the package: `)[^`]+' README.md)
watchdog_block=$(awk '/^```sh$/ { on = 1; next } /^```$/ { on = 0 } on' README.md)
for c in "$install_cmd" "$enable_cmd" "$check_cmd" "$stop_cmd" "$remove_cmd" "$watchdog_block"; do
  [ -n "$c" ] || { echo "a README command is missing" >&2; exit 1; }
done

readme=nc-readme-check
trap 'podman rm -f "$name" "$readme" >/dev/null 2>&1 || true' EXIT
podman rm -f "$readme" >/dev/null 2>&1 || true
podman run -d --name "$readme" --systemd=always localhost/nicecooling-unit >/dev/null
podman exec "$readme" sh -c 'until [ "$(systemctl is-system-running 2>/dev/null)" = running ] || [ "$(systemctl is-system-running 2>/dev/null)" = degraded ]; do sleep 0.5; done'
podman exec "$readme" mkdir -p /root/repo/target/rpm/RPMS/x86_64
podman cp "$rpm_file" "$readme":/root/repo/target/rpm/RPMS/x86_64/
podman exec "$readme" sh -c '
  mkdir -p /run/nc-sysfs/class/hwmon /etc/systemd/system/nicecoolingd.service.d
  printf "[Service]\nExecStart=\nExecStart=/usr/libexec/nicecoolingd --sysfs-root /run/nc-sysfs --no-leakshield\nExecStopPost=\nExecStopPost=/usr/libexec/nicecoolingd --restore-bios --sysfs-root /run/nc-sysfs\n" \
    > /etc/systemd/system/nicecoolingd.service.d/test.conf'

readme_run() { echo "README: $1"; podman exec -w /root/repo "$readme" bash -c "yes | $1" >/dev/null; }
readme_run "$install_cmd"
readme_run "$enable_cmd"
active=$(podman exec "$readme" bash -c "$check_cmd")
[ "$active" = active ]
echo "README: $check_cmd -> $active"
podman exec "$readme" test -f /var/lib/nicecooling/bios-snapshot.json
echo "README: bios-snapshot.json exists after the first start"

echo "README: watchdog block"
wd=$(podman exec "$readme" bash -ec "$watchdog_block" | tail -n 1)
[ "$wd" = RuntimeWatchdogUSec=30s ]
echo "README: watchdog block -> $wd"

readme_run "$stop_cmd"
readme_run "$remove_cmd"
podman exec "$readme" rpm -q nicecooling >/dev/null 2>&1 && { echo "nicecooling is still installed" >&2; exit 1; }
left=0
while IFS= read -r path; do
  [ "$path" = /var/lib/nicecooling ] && continue
  if podman exec "$readme" test -e "$path"; then echo "remains after the README rollback: $path" >&2; left=1; fi
done < "$rpmdir/installed-paths.txt"
[ "$left" -eq 0 ]
podman exec "$readme" test -d /var/lib/nicecooling
echo "README: rollback removed every install path, /var/lib/nicecooling remains"
