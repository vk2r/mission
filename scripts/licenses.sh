#!/usr/bin/env bash
# Regenerates licenses/THIRD-PARTY.txt: the crates (cargo-about) plus the ported code (Open Bot Avatar).
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
oba="$root/engine/src/avatar/oba/LICENSE"
out="$root/licenses/THIRD-PARTY.txt"
tmp="$(mktemp "${TMPDIR:-/tmp}/omb-licenses.XXXXXX")"
trap 'rm -f "$tmp"' EXIT

cd "$root"
cargo about generate --locked --fail -m engine/Cargo.toml engine/about.hbs -o "$tmp"
{
  cat <<'HEADER'
================================================================================
Ported code
--------------------------------------------------------------------------------
Open Bot Avatar, ported to Rust in engine/src/avatar/oba/
  Project: Open Bot Avatar, by Dominik Scholz
  URL:     https://github.com/DominikScholz/open-bot-avatar
  Version: @open-bot-avatar/core 0.3.0
  Commit:  5e64c08
  Licence: MIT

HEADER
  cat "$oba"
} >>"$tmp"
install -m 644 "$tmp" "$out"
