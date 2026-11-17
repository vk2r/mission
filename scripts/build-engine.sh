#!/usr/bin/env bash
# Build the engine. By default it goes to bin/engine.local, which git ignores and
# bin/run-engine uses while it matches the manifest version; release.yml passes the path of the
# file it publishes.
#   scripts/build-engine.sh [output-file]
set -Eeuo pipefail

root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/openmausbot-engine/target}"
out="${1:-$root/bin/engine.local}"

cargo build --locked --release --manifest-path "$root/engine/Cargo.toml"

mkdir -p "$(dirname -- "$out")"
out="$(cd -- "$(dirname -- "$out")" && pwd -P)/$(basename -- "$out")"

old_pids=()
for exe in /proc/[0-9]*/exe; do
  [[ "$(readlink "$exe" 2>/dev/null || true)" == "$out" ]] || continue
  pid="${exe#/proc/}"; old_pids+=("${pid%/exe}")
done

install -m 755 "$CARGO_TARGET_DIR/release/engine" "$out"

# bin/run-engine only trusts the local build for the version it was built for.
if [[ $out == "$root/bin/engine.local" ]]; then
  sed -n 's/^  "version": "\([^"]*\)",$/\1/p' "$root/manifest.json" | head -n 1 >"$root/bin/engine.local.version"
fi

if ((${#old_pids[@]})); then
  echo "restarting running engine: ${old_pids[*]}"
  kill "${old_pids[@]}" 2>/dev/null || true
fi

ls -l "$(dirname -- "$out")"
