#!/usr/bin/env bash
# Usage: build.sh NWNX_CHECKOUT DEST
# Copies an nwnxee/unified checkout to DEST, applies reconnect.patch and builds
# NWNX_Redis there; DEST/Binaries then holds the patched NWNX_Redis.so and links
# to the checkout's other plugins, so a server uses it with NWNX_ROOT=DEST.
# The checkout itself is never changed.
set -euo pipefail
(( $# == 2 )) || { echo 'Expected NWNX_CHECKOUT DEST' >&2; exit 2; }
src=$(realpath -- "$1")
dest=$2
patch=$(realpath -- "$(dirname -- "$0")/reconnect.patch")
[[ -d "$src/Plugins/Redis" && -d "$src/Binaries" ]] || { echo "$src is not a built NWNX checkout" >&2; exit 2; }
[[ ! -e "$dest" ]] || { echo "$dest exists; remove it first" >&2; exit 1; }
cp -a -- "$src" "$dest"
rm -rf -- "$dest/Binaries" "$dest"/build*
git -C "$dest" apply -- "$patch"
cmake -S "$dest" -B "$dest/build-redis" -DCMAKE_BUILD_TYPE=RelWithDebInfo >/dev/null
cmake --build "$dest/build-redis" --target Redis -j "$(nproc)"
for f in "$src"/Binaries/*; do
    [[ "$(basename -- "$f")" == NWNX_Redis.so ]] || ln -sfn -- "$f" "$dest/Binaries/$(basename -- "$f")"
done
echo "Built $dest/Binaries/NWNX_Redis.so"
