#!/usr/bin/env bash
# Builds the RPM in a Fedora container. Run from anywhere: it changes to the repository root.
# Usage: packaging/container/build-rpm.sh [output-dir]    (default: target/rpm)
# The output directory gets SOURCES/ (the two tarballs), RPMS/ and SRPMS/.
# `rpmbuild` runs with `--network none`: only the vendor step and the image build use the network.
# JOBS sets the cargo jobs (default 8).
set -euo pipefail

cd "$(dirname "$0")/../.."
out=${1:-target/rpm}
jobs=${JOBS:-8}
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
image=localhost/nicecooling-rpm-build

mkdir -p "$out/SOURCES"
out=$(cd "$out" && pwd)
rm -rf "$out/RPMS" "$out/SRPMS" "$out/BUILD" "$out/BUILDROOT"

podman build -f packaging/container/Containerfile.build -t "$image" .

# Source0: the tracked files and the new files that git does not ignore, as they are on disk.
git ls-files -z --cached --others --exclude-standard \
  | while IFS= read -r -d '' f; do [ -e "$f" ] && printf '%s\0' "$f"; done \
  | tar --null -T - --transform "s,^,nicecooling-$version/," --sort=name --mtime=@0 \
        --owner=0 --group=0 --numeric-owner -cf - \
  | zstd -q -T4 -19 -o "$out/SOURCES/nicecooling-$version.tar.zst" -f

# Source1: the vendored crates, made from the Source0 tree. This step needs network.
podman run --rm -v "$out":/rpm:Z "$image" sh -c "
  set -e
  rm -rf /tmp/tree && mkdir /tmp/tree
  tar -C /tmp/tree -xf /rpm/SOURCES/nicecooling-$version.tar.zst
  /tmp/tree/nicecooling-$version/packaging/vendor.sh /rpm/SOURCES"

mkdir -p "$out/SPECS"
cp packaging/nicecooling.spec "$out/SPECS/"
podman run --rm --network none -v "$out":/rpm:Z "$image" \
  rpmbuild -ba --define "_topdir /rpm" --define "_smp_build_ncpus $jobs" /rpm/SPECS/nicecooling.spec
