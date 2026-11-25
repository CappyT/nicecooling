#!/usr/bin/env bash
# Builds the DEB in an Ubuntu 24.04 container. Run from anywhere: it changes to the repository root.
# Usage: packaging/container/build-deb.sh [output-dir]    (default: target/deb)
# The output directory gets nicecooling_<version>-1_amd64.deb. The build needs network: cargo fetches the crates.
# JOBS sets the cargo jobs (default 8).
set -euo pipefail

cd "$(dirname "$0")/../.."
out=${1:-target/deb}
jobs=${JOBS:-8}
image=localhost/nicecooling-deb-build

mkdir -p "$out"
out=$(cd "$out" && pwd)

podman build -f packaging/container/Containerfile.deb -t "$image" packaging/container

# The tracked files and the new files that git does not ignore, as they are on disk.
git ls-files -z --cached --others --exclude-standard \
  | while IFS= read -r -d '' f; do [ -e "$f" ] && printf '%s\0' "$f"; done \
  | tar --null -T - --sort=name --mtime=@0 --owner=0 --group=0 --numeric-owner -cf - \
  | podman run --rm -i -v "$out":/out:Z -e JOBS="$jobs" "$image" \
      sh -c 'tar -x -C /work && /work/packaging/deb/make-deb.sh /out'
