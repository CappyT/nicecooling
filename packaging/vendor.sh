#!/usr/bin/env bash
# Vendors the crates of Cargo.lock and writes the tarball that `nicecooling.spec` uses as Source1.
# Usage: packaging/vendor.sh [output-dir]    (default: target/rpm/SOURCES)
# It needs cargo, zstd and network. The RPM build itself needs none of them.
set -euo pipefail

cd "$(dirname "$0")/.."
out=${1:-target/rpm/SOURCES}
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
[ -n "$version" ] || { echo "vendor.sh: no workspace version in Cargo.toml" >&2; exit 1; }
# The spec writes a pre-release as 0.2.0~beta.1, and it names Source1 with that version.
rpm_version=${version//-/\~}

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

cargo vendor --locked "$work/vendor" >/dev/null
mkdir -p "$out"
tar --sort=name --mtime=@0 --owner=0 --group=0 --numeric-owner -C "$work" -cf - vendor \
  | zstd -q -T4 -19 -o "$out/nicecooling-$rpm_version-vendor.tar.zst" -f
echo "$out/nicecooling-$rpm_version-vendor.tar.zst"
