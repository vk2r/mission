#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
engine="${ENGINE:-$root/bin/engine}"
[[ -x $engine ]] || { echo "check-silent: no engine at $engine" >&2; exit 1; }

work="$(mktemp -d "${TMPDIR:-/tmp}/omb-silent.XXXXXX")"
trap 'jobs -p | xargs -r kill 2>/dev/null; rm -rf "$work"' EXIT
mkdir -p "$work/bin" "$work/home/.openmausbot"
cat >"$work/bin/notify-send" <<'STANDIN'
#!/usr/bin/env bash
echo called >>"$NOTIFY_LOG"
STANDIN
chmod 700 "$work/bin/notify-send"

run() {
  local label=$1; shift
  local dir="$work/$label"
  mkdir -p "$dir/rt"; chmod 700 "$dir/rt"
  : >"$dir/server.log"; : >"$dir/calls"
  (
    export HOME="$work/home" OPENMAUSBOT_HOME="$work/home/.openmausbot"
    export XDG_RUNTIME_DIR="$dir/rt" XDG_DATA_HOME="$work/data" XDG_CONFIG_HOME="$work/config"
    export OPENMAUSBOT_LOG="$dir/server.log" OPENMAUSBOT_LOG_POLL=0.05
    export NOTIFY_LOG="$dir/calls" PATH="$work/bin:$PATH"
    exec 3<> <(:)
    sleep 30 | exec "$engine" "$@" >/dev/null 2>&1
  ) &
  local pid=$!
  sleep 1
  printf '%s\n' '[2026-11-01T00:00:40.000Z] [out] [omb-turn] bot=x text="silent check" images=0 depth=1 card=false' >>"$dir/server.log"
  sleep 1.5
  kill "$pid" 2>/dev/null || true; pkill -P "$pid" 2>/dev/null || true
  wc -l <"$dir/calls"
}

off=$(run off --no-notify)
on=$(run on)
echo "check-silent: --no-notify -> $off call(s); default -> $on call(s)"
[[ $off -eq 0 ]] || { echo "check-silent: the daemon notified with the switch off" >&2; exit 1; }
[[ $on -ge 1 ]] || { echo "check-silent: the control run never notified, so the check proves nothing" >&2; exit 1; }
