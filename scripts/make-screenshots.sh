#!/usr/bin/env bash
set -Eeuo pipefail

root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
out="$root/screenshots"
id="io.github.vk2r.mission"
shell_json="$HOME/.config/omarchy/shell.json"
work=""
real_engine="$root/bin/engine"
output="${SHOT_OUTPUT:-HDMI-A-1}"

mkdir -p "$out"

say() { printf 'make-screenshots: %s\n' "$*" >&2; }

qs_id() { qs list -a -j | jq -r '.[0].id'; }
ipc() { qs ipc --id "$(qs_id)" call "$id" "$@"; }

engines_of() {
  local exe="$1" link
  for link in /proc/[0-9]*/exe; do
    [[ "$(readlink "$link" 2>/dev/null || true)" == "$exe" ]] || continue
    link="${link#/proc/}"; echo "${link%/exe}"
  done
}

restore() {
  trap - EXIT INT TERM
  [[ -n ${cursor:-} ]] && move_cursor "${cursor%,*}" "${cursor#*,}" 2>/dev/null || true
  hyprctl eval 'hl.config({cursor={invisible=false}})' >/dev/null 2>&1 || true
  [[ -n $work ]] || return 0
  if [[ -f $work/engine.real ]]; then
    cp -p "$work/engine.real" "$real_engine.new" && mv -f "$real_engine.new" "$real_engine"
    local pid
    for pid in $(engines_of "$work/engine.real"); do kill "$pid" 2>/dev/null || true; done
  fi
  if [[ -f $work/shell.json.orig ]]; then
    cp -p "$work/shell.json.orig" "$shell_json.new" && mv -f "$shell_json.new" "$shell_json"
  fi
  rm -rf "$work"
  say "restored bin/engine and shell.json"
}
trap restore EXIT
trap 'exit 130' INT TERM

work="$(mktemp -d "${TMPDIR:-/tmp}/omb-shots.XXXXXX")"
mkdir -p "$work"/{home/usage,rt,bin,hypr,xdg/applications}

cat >"$work/bin/hyprctl" <<'STANDIN'
#!/usr/bin/env bash
case "${1:-}" in
  clients) cat "$HYPR_DIR/clients.json" ;;
  monitors) cat "$HYPR_DIR/monitors.json" ;;
  dispatch) printf '%s\n' "${2:-}" >>"$HYPR_LOG" ;;
  *) exit 1 ;;
esac
STANDIN
cat >"$work/bin/gtk-launch" <<'STANDIN'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$LAUNCH_LOG"
STANDIN
cat >"$work/bin/notify-send" <<'STANDIN'
#!/usr/bin/env bash
exit 0
STANDIN
chmod 700 "$work/bin/"*
cat >"$work/engine.wrapper" <<WRAP
#!/usr/bin/env bash
export OPENMAUSBOT_HOME="$work/home" OPENMAUSBOT_LOG="$work/server.log"
export XDG_RUNTIME_DIR="$work/rt" XDG_DATA_HOME="$work/xdg" HYPRLAND_INSTANCE_SIGNATURE=shots
export HYPR_DIR="$work/hypr" HYPR_LOG="$work/hypr.log" LAUNCH_LOG="$work/launch.log"
export PATH="$work/bin:\$PATH"
exec "$work/engine.real" "\$@"
WRAP
chmod 700 "$work/engine.wrapper"
: >"$work/server.log"; : >"$work/launch.log"

roster='Atlas|Chief of Staff|star|yellow
Nova|Frontend Engineer|blob|cyan
Quill|Technical Writer|drop|pink
Bolt|Backend Engineer|hexagon|orange
Sage|Research Analyst|circle|green
Pixel|Product Designer|squircle|purple
Echo|QA Engineer|diamond|teal
Forge|DevOps|shield|red
Lumen|Data Analyst|capsule|blue
Orbit|Project Planner|cursor|coral
Ranger|Security Auditor|hexagon|chrome
Mira|Customer Support|drop|green
Zephyr|Documentation|blob|silver
Juniper|Finance|squircle|yellow'
longname='Chief Operations Orchestrator Prime|Senior Staff Principal Release Engineering Lead|star|purple'

write_team() {
  local count="$1" long="${2:-}"
  { [[ -n $long ]] && echo "$longname"; echo "$roster"; } | head -n "$count" | python3 -I -c '
import json, sys
bots = []
for index, line in enumerate(sys.stdin.read().splitlines()):
    name, title, body, colour = line.split("|")
    bots.append({"id": "bot-%02d" % index, "name": name, "title": title, "description": title,
                 "chiefOfStaff": index == 0, "section": "General", "mascotBody": body, "color": colour})
print(json.dumps(bots, indent=1))' >"$work/home/bots.json"
}

task() {
  local tasks=("Plan the release" "Refactor the settings panel" "Draft the changelog" "Fix the failing build" "Summarise the research" "Review the new mockups" "Run the regression suite" "Roll out the staging deploy" "Clean the sales dataset" "Schedule the sprint" "Scan the dependencies" "Answer open tickets" "Update the docs" "Reconcile the invoices")
  echo "${tasks[$1]}"
}

write_state() {
  local now_ms spec pair
  now_ms="$(date +%s%3N)"
  rm -f "$work/home/runtime.db"
  sqlite3 "$work/home/runtime.db" "CREATE TABLE assignments (id INTEGER PRIMARY KEY, assignee_bot_id TEXT, status TEXT, json TEXT, created_at INTEGER, finished_at INTEGER);
    CREATE INDEX assignments_status ON assignments(status); CREATE INDEX assignments_live ON assignments(assignee_bot_id, status);"
  for pair in $1; do
    local idx="${pair%%:*}" st="${pair##*:}" bot
    bot="$(printf 'bot-%02d' "$idx")"
    case "$st" in
      running) sqlite3 "$work/home/runtime.db" "INSERT INTO assignments (assignee_bot_id,status,json,created_at) VALUES ('$bot','running','{\"startedAt\": $((now_ms - 20000)), \"origin\": {\"quote\": \"$(task "$idx")\"}}',$((now_ms - 30000)));" ;;
      waiting) sqlite3 "$work/home/runtime.db" "INSERT INTO assignments (assignee_bot_id,status,json,created_at) VALUES ('$bot','waiting','{\"brief\":\"$(task "$idx")\"}',$((now_ms - 30000)));" ;;
      failed) sqlite3 "$work/home/runtime.db" "INSERT INTO assignments (assignee_bot_id,status,json,created_at,finished_at) VALUES ('$bot','failed','{\"brief\":\"$(task "$idx")\"}',$((now_ms - 90000)),$((now_ms - 60000)));" ;;
    esac
  done
}

write_usage() {
  rm -f "$work/home/usage/"*.jsonl
  python3 -I - "$work/home/usage" "$(sed -n 's/^\(.*\)|.*|.*|.*$/\1/p' <<<"$roster" | head -n 8 | paste -sd,)" <<'PY'
import json, random, sys
from datetime import datetime, timedelta, timezone
usage, names = sys.argv[1], sys.argv[2].split(",")
rng = random.Random(7)
models = [("claude-opus-5-5", 0.85), ("claude-sonnet-5-5", 0.32), ("gpt-5.2", 0.27),
          ("gemini-3-pro", 0.18), ("deepseek-v4", 0.04), ("glm-5.3", 0.07)]
weights = [5, 3, 2, 1, 2, 1, 1, 1]
now = datetime.now(timezone.utc)
files = {}
for back in range(30):
    for hour in range(0, 24):
        stamp = (now - timedelta(days=back)).replace(hour=hour, minute=rng.randrange(60), second=0, microsecond=0)
        if stamp > now:
            continue
        if rng.random() > (0.55 if 8 <= (hour - 3) % 24 <= 22 else 0.12):
            continue
        index = rng.choices(range(len(names)), weights=weights[: len(names)])[0]
        model, rate = models[(index + rng.randrange(3)) % len(models)] if index else models[rng.choice([0, 0, 1])]
        cost = round(rate * rng.uniform(0.3, 2.6), 4)
        record = {"at": stamp.isoformat().replace("+00:00", "Z"), "botId": "bot-%02d" % index,
                  "botName": names[index], "model": model, "costUsd": cost, "outcome": {"status": "ok"}}
        files.setdefault(stamp.strftime("%Y-%m"), []).append(json.dumps(record, separators=(",", ":")))
for month, lines in files.items():
    open(f"{usage}/{month}.jsonl", "w").write("\n".join(lines) + "\n")
PY
}

hypr_state() {
  case "$1" in
    open) cp "$root/engine/tests/fixtures/hypr/visible/clients.json" "$work/hypr/clients.json" ;;
    closed) cp "$root/engine/tests/fixtures/hypr/absent/clients.json" "$work/hypr/clients.json" ;;
  esac
  cp "$root/engine/tests/fixtures/hypr/visible/monitors.json" "$work/hypr/monitors.json"
}

app_state() {
  case "$1" in
    installed) printf '[Desktop Entry]\nType=Application\nName=MausBot (stand-in)\nExec=gtk-launch openmausbot\n' >"$work/xdg/applications/openmausbot.desktop" ;;
    missing) rm -f "$work/xdg/applications/openmausbot.desktop" ;;
  esac
}

widget_cfg() {
  local tmp
  tmp="$(mktemp "$shell_json.XXXXXX")"
  jq --arg id "$id" --argjson cost "$1" --argjson pixel "$2" --arg range "$3" --argjson auto "$4" --arg group "${5:-}" '
    (.. | objects | select(.id? == $id))
      |= ((del(.showCost, .pixelAvatars, .autoLaunch, .costGroup)
          + (if $cost then {showCost: true} else {} end)
          + (if $pixel then {pixelAvatars: true} else {} end)
          + (if $auto then {autoLaunch: true} else {} end))
          | .costRange = $range | (if $group != "" then .costGroup = $group else . end) | .language = "en" | .notify = false)' "$work/shell.json.orig" >"$tmp"
  chmod --reference="$shell_json" "$tmp"; mv -f "$tmp" "$shell_json"
}

scene() {
  NOFOCUS=0
  app_state installed
  case "$1" in
    home-notinstalled) write_team 5;   write_state "";                    hypr_state closed; app_state missing; widget_cfg true false week false; PAGE=home ;;
    settings-notinstalled) write_team 6; write_state "";                  hypr_state closed; app_state missing; widget_cfg false false week false; PAGE=settings ;;
    home-launch)       write_team 5;   write_state "0:running 1:running"; hypr_state closed; widget_cfg true false week false; PAGE=home ;;
    home-bots-one)     write_team 1;   write_state "";                    hypr_state open;   widget_cfg true false week false; PAGE=home ;;
    home-bots-few)     write_team 4;   write_state "0:running 2:waiting"; hypr_state open;   widget_cfg true false week false; PAGE=home ;;
    home-bots-mixed)   write_team 7;   write_state "0:running 1:running 2:waiting 3:failed 5:waiting"; hypr_state open; widget_cfg true false week false; PAGE=home ;;
    home-bots-many)    write_team 14;  write_state "0:running 1:running 3:running 4:waiting 6:failed 9:waiting"; hypr_state open; widget_cfg true false week false; PAGE=home ;;
    home-bots-cost)    write_team 6;   write_state "0:running 1:waiting 3:failed"; hypr_state open; widget_cfg true false week false; PAGE=home ;;
    home-bots-nocost)  write_team 6;   write_state "0:running 1:waiting 3:failed"; hypr_state open; widget_cfg false false week false; PAGE=home ;;
    home-bots-pixel)   write_team 6;   write_state "0:running 1:waiting 3:failed"; hypr_state open; widget_cfg true true week false; PAGE=home ;;
    home-bots-longname) write_team 5 long; write_state "0:running 2:waiting"; hypr_state open; widget_cfg true false week false; PAGE=home ;;
    gauge)             write_team 8;   write_state "0:running 2:waiting"; hypr_state open; widget_cfg true false week false; PAGE=costs; NOFOCUS=1 ;;
    gauge-models)      write_team 8;   write_state "0:running 2:waiting"; hypr_state open; widget_cfg true false week false model; PAGE=costs; NOFOCUS=1 ;;
    gauge-focus)       write_team 8;   write_state "0:running 2:waiting"; hypr_state open; widget_cfg true false week false; PAGE=costs ;;
    gauge-day)         write_team 8;   write_state "0:running";           hypr_state open; widget_cfg true false day false;  PAGE=costs ;;
    gauge-month)       write_team 8;   write_state "0:running";           hypr_state open; widget_cfg true false month false; PAGE=costs ;;
    settings)          write_team 6;   write_state "0:running";           hypr_state open; widget_cfg true false week true; PAGE=settings ;;
    bar)               write_team 6;   write_state "0:running 1:running"; hypr_state open; widget_cfg true false week false; PAGE=none ;;
    *) say "unknown scene $1"; exit 2 ;;
  esac
  write_usage
}

all_scenes="home-launch home-notinstalled home-bots-one home-bots-few home-bots-mixed home-bots-many home-bots-cost home-bots-nocost home-bots-pixel home-bots-longname gauge gauge-models gauge-focus gauge-day gauge-month settings settings-notinstalled bar"

swap_in() {
  [[ -x $real_engine ]] || { say "missing $real_engine (run 'mise run build')"; exit 1; }
  [[ "$(head -c2 "$real_engine")" != '#!' ]] || { say "bin/engine is a wrapper left by an interrupted run (run 'mise run build')"; exit 1; }
  cp -p "$real_engine" "$work/engine.real"
  cp -p "$shell_json" "$work/shell.json.orig"
  scene "${1:-home-bots-mixed}" >/dev/null
  local old
  old="$(engines_of "$real_engine" | tr '\n' ' ')"
  cp -p "$work/engine.wrapper" "$real_engine.new" && mv -f "$real_engine.new" "$real_engine"
  for pid in $old; do kill "$pid" 2>/dev/null || true; done
  for _ in $(seq 60); do [[ -n "$(engines_of "$work/engine.real")" ]] && return 0; sleep 0.5; done
  say "the hub did not restart the engine"; exit 1
}

crop() {
  local x0 y0 x1 y1
  read -r x0 y0 x1 y1 < <(frame_of "$1") || { say "panel frame not found"; return 1; }
  python3 -I - "$1" "$2" "$x0" "$y0" "$x1" "$y1" <<'PY'
import sys
from PIL import Image
src, dst, x0, y0, x1, y1 = sys.argv[1], sys.argv[2], *map(int, sys.argv[3:])
Image.open(src).convert("RGB").crop((x0, y0, x1 + 1, y1 + 1)).save(dst)
PY
}

settle() { sleep "${SETTLE:-3.5}"; }

icon_x="${ICON_X:-1574}"; icon_y="${ICON_Y:-15}"

mon_info() { hyprctl monitors -j | jq -r --arg o "$output" '.[] | select(.name == $o) | "\(.x) \(.y) \(.scale)"'; }
move_cursor() { hyprctl dispatch "hl.dsp.cursor.move({x=$(printf '%.0f' "$1"),y=$(printf '%.0f' "$2")})" >/dev/null; }
click_at() {
  move_cursor "$1" "$2"
  sleep 0.3
  ydotool mousemove -x 2 -y 0 >/dev/null 2>&1; sleep 0.15; ydotool mousemove -x -2 -y 0 >/dev/null 2>&1; sleep 0.4
  ydotool click 0x40 >/dev/null 2>&1; sleep 0.1; ydotool click 0x80 >/dev/null 2>&1
}

frame_of() {
  python3 -I - "$1" <<'PY'
import sys
from PIL import Image
im = Image.open(sys.argv[1]).convert("RGB"); w, h = im.size; px = im.load()
near = lambda a, b: sum(abs(p - q) for p, q in zip(a, b)) <= 60
def runs_in(y):
    out, x = [], 0
    while x < w:
        c = px[x, y]; x2 = x
        if max(c) - min(c) > 100:
            while x2 + 1 < w and near(px[x2 + 1, y], c): x2 += 1
            if x2 - x >= 150: out.append((x, x2, c))
        x = x2 + 1
    return out
top = None
for y in range(min(h, 300)):
    for x0, x1, c in runs_in(y):
        if x1 - x0 < 900: top = (y, x0, x1, c); break
    if top: break
if not top: sys.exit(1)
y0, x0, x1, c = top
x1 = max(x for x in range(x0, min(w, x0 + 900)) if near(px[x, y0], c))
bottom = y0
while bottom + 1 < h and near(px[x0 + 1, bottom + 1], c): bottom += 1
if bottom - y0 < 60: sys.exit(1)
if x1 - x0 > 900 or bottom - y0 > 1600: sys.exit(1)
print(x0, y0, x1, bottom)
PY
}

panel_up() { grim -o "$output" "$work/full.png" && frame_of "$work/full.png" >/dev/null 2>&1; }
set_panel() {
  local want="$1" try
  for try in 1 2 3; do
    if panel_up; then [[ $want == up ]] && return 0; else [[ $want == down ]] && return 0; fi
    click_at "$ix" "$iy"; sleep 1.2
  done
  rm -f "$work/full.png"; say "could not get the panel $want"; return 1
}

shoot() {
  local name="$1" file="$out/$1.png" mx my sc x0 y0 x1 y1 ix iy
  NOFOCUS=0
  scene "$name"
  settle
  read -r mx my sc < <(mon_info)
  ix="$(awk -v a="$mx" -v b="$icon_x" 'BEGIN{print a+b}')"; iy="$(awk -v a="$my" -v b="$icon_y" 'BEGIN{print a+b}')"
  if [[ $PAGE == none ]]; then
    set_panel down
    move_cursor "$ix" "$(awk -v a="$iy" 'BEGIN{print a+200}')"
    sleep 0.5
    grim -g "$(awk -v x="$ix" -v y="$my" 'BEGIN{printf "%d,%d 260x32", x-130, y}')" "$file"
    say "wrote $file"; return 0
  fi
  set_panel down; set_panel up
  read -r x0 y0 x1 y1 < <(frame_of "$work/full.png") || { say "panel frame not found for $name"; rm -f "$work/full.png"; return 1; }
  case "$PAGE" in
    costs)    click_at "$(awk -v m="$mx" -v s="$sc" -v x="$x1" 'BEGIN{print m + x/s - 52}')" "$(awk -v m="$my" -v s="$sc" -v y="$y1" 'BEGIN{print m + y/s - 33}')" ;;
    settings) click_at "$(awk -v m="$mx" -v s="$sc" -v x="$x1" 'BEGIN{print m + x/s - 34}')" "$(awk -v m="$my" -v s="$sc" -v y="$y0" 'BEGIN{print m + y/s + 32}')" ;;
  esac
  if [[ ${NOFOCUS:-} == 1 ]]; then
    move_cursor "$(awk -v m="$mx" -v s="$sc" -v x="$x0" -v x1="$x1" 'BEGIN{print m + (x+x1)/2/s}')" "$(awk -v m="$my" -v s="$sc" -v y="$y0" 'BEGIN{print m + y/s + 14}')"
    sleep 0.3; ydotool mousemove -x 2 -y 0 >/dev/null 2>&1; sleep 0.15; ydotool mousemove -x -2 -y 0 >/dev/null 2>&1
  else
    move_cursor "$ix" "$(awk -v a="$my" 'BEGIN{print a+900}')"
  fi
  sleep 1.2
  [[ ${NOFOCUS:-} == 1 ]] && { /usr/bin/hyprctl eval 'hl.config({cursor={invisible=true}})' >/dev/null; sleep 0.8; }
  grim -o "$output" "$work/full.png"
  [[ ${NOFOCUS:-} == 1 ]] && /usr/bin/hyprctl eval 'hl.config({cursor={invisible=false}})' >/dev/null
  crop "$work/full.png" "$file"
  rm -f "$work/full.png"
  set_panel down
  say "wrote $file"
}

main() {
  cursor="$(hyprctl cursorpos | tr -d ' ')"
  swap_in "${1:-home-bots-mixed}"
  local name
  for name in "${@:-$all_scenes}"; do
    for n in $name; do shoot "$n"; done
  done
  [[ -s $work/launch.log ]] && say "gtk-launch stand-in was called (it only logged): $(wc -l <"$work/launch.log") time(s)"
  return 0
}
[[ ${1:-} == --source ]] || { main "$@"; restore; omarchy-restart-shell; }
