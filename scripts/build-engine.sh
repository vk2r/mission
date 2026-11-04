#!/usr/bin/env bash
set -Eeuo pipefail

root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/openmausbot-engine/target}"

cargo build --locked --release --manifest-path "$root/engine/Cargo.toml"

mkdir -p "$root/bin"

old_pids=()
for exe in /proc/[0-9]*/exe; do
  [[ "$(readlink "$exe" 2>/dev/null || true)" == "$(cd "$root" && pwd -P)/bin/engine" ]] || continue
  pid="${exe#/proc/}"; old_pids+=("${pid%/exe}")
done

install -m 755 "$CARGO_TARGET_DIR/release/engine" "$root/bin/engine"

if ((${#old_pids[@]})); then
  echo "restarting running engine: ${old_pids[*]}"
  kill "${old_pids[@]}" 2>/dev/null || true
fi

ls -l "$root/bin"
