#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pillow==13.0.0"]
# ///
"""Record the engine's Open Bot Avatar animations for the README and the preview.

Runs the real engine (bin/engine.local, `mise run build`) on a throwaway team of five bots that all
work, each with a different face, so each shows a different animation: Atlas works, Quill celebrates,
Nova idles, Sage is happy and hops, and Bolt waits. It asks for the moving frames the way the panel
does (`animate`), keeps the picture of every frame, and renders them with rsvg-convert.

Writes, into the repository:
  assets/preview/crew.webp        the five avatars in a row, a 10 s loop at 20 fps (the preview's pill)
  assets/preview/crew-still.png   the same row at rest, no motion (preview.png's pill)
  screenshots/motion.webp         the README's Usage animation: the panel's mark (assets/preview/mark.webp)
                                  and the five avatars

Usage: scripts/make-crew.py [engine]   (default: bin/engine.local)
"""
import io
import json
import os
import re
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import time
from pathlib import Path

from PIL import Image, ImageDraw, ImageSequence

ROOT = Path(__file__).resolve().parent.parent
ENGINE = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "bin" / "engine.local"
CREW_OUT = ROOT / "assets" / "preview" / "crew.webp"
STILL_OUT = ROOT / "assets" / "preview" / "crew-still.png"
MARK = ROOT / "assets" / "preview" / "mark.webp"
MOTION_OUT = ROOT / "screenshots" / "motion.webp"

# name, body, colour, face. The face decides the animation (src/avatar/looks.rs): working turns a band
# of ribbons round the body, celebrate turns a cage of three ribbons, happy hops, alerting is a waiting bot
# (it leans, squints and sighs) and idle blinks and glances about. Thinking and sad are left out: they swap
# the body for three dots and an exclamation mark. The order is the pill's.
CREW = (
    ("Atlas", "star", "yellow", "working"),
    ("Quill", "drop", "pink", "celebrate"),
    ("Nova", "blob", "cyan", "idle"),
    ("Sage", "circle", "green", "happy"),
    ("Bolt", "hexagon", "orange", "alerting"),
)
FPS, LOOP_S = 20, 10          # crew.webp and motion.webp: 200 frames of 50 ms
TAKE_S = 24                   # how long the engine is recorded; the loop is cut from the best 10 s of it
FIRST_S = 1.5                 # the first seconds are the faces settling in

# crew.webp: one CELL square per avatar, CELL_PITCH apart, the body SLOT wide in the middle of it (the
# picture spills past the body by the ribbons: its extent is about 165% of the slot).
CELL, CELL_PITCH, SLOT = 80, 82, 46
# motion.webp: the panel's mark, then the five avatars, centred in equal columns.
MOTION_SIZE, MOTION_SLOT, MOTION_RADIUS, MOTION_BG = (908, 150), 62, 18, (13, 7, 15, 255)


def say(text):
    print(f"make-crew: {text}", file=sys.stderr)


def team(home, now):
    bots = [{"id": f"b{i}", "name": name, "title": "Crew", "mascotBody": body, "color": colour,
             "mascotFaceColor": "white", "mascotExpression": face, "chiefOfStaff": i == 0} for i, (name, body, colour, face) in enumerate(CREW)]
    (home / "bots.json").write_text(json.dumps(bots))
    db = sqlite3.connect(home / "runtime.db")
    db.execute("CREATE TABLE assignments (id INTEGER PRIMARY KEY, assignee_bot_id TEXT, status TEXT, json TEXT,"
               " created_at INTEGER, finished_at INTEGER)")
    for i in range(len(CREW)):
        db.execute("INSERT INTO assignments (assignee_bot_id, status, json, created_at) VALUES (?, 'running', ?, ?)",
                   (f"b{i}", json.dumps({"startedAt": now - 20000, "origin": {"quote": "Crew"}}), now - 30000))
    db.commit()
    db.close()


def engine_env(work, name):
    home, runtime = work / f"{name}-home", work / f"{name}-run"
    (home / "usage").mkdir(parents=True)
    runtime.mkdir()
    return home, dict(os.environ, OPENMAUSBOT_HOME=str(home), XDG_RUNTIME_DIR=str(runtime))


def picture(url):
    return Path(url.removeprefix("file://")).read_text()


def record(work):
    """Every line of moving frames the engine sends for TAKE_S seconds: [(seconds, {bot id: svg})]."""
    home, env = engine_env(work, "moving")
    team(home, int(time.time() * 1000))
    engine = subprocess.Popen([ENGINE, "--no-notify"], stdin=subprocess.PIPE, stdout=subprocess.PIPE, env=env, text=True)
    engine.stdin.write("animate 53\n")
    engine.stdin.flush()
    start, lines = time.time(), []
    while time.time() - start < TAKE_S:
        line = engine.stdout.readline()
        if not line:
            raise SystemExit("make-crew: the engine stopped")
        data = json.loads(line)
        if "frames" in data:  # read now: the engine deletes frames nobody shows after a few seconds
            lines.append((time.time() - start,
                          {k: picture(v["avatar"]) for k, v in data["frames"].items() if "avatar" in v}))
    engine.kill()
    if len(lines) < FPS * LOOP_S or any(len(frames) != len(CREW) for _, frames in lines[-5:]):
        raise SystemExit("make-crew: the engine did not move all five avatars")
    say(f"recorded {len(lines)} frame lines in {TAKE_S} s")
    return lines


def rest(work):
    """The avatars at rest, from a snapshot of the same bots idle: {bot id: svg}."""
    home, env = engine_env(work, "still")
    bots = [{"id": f"b{i}", "name": name, "title": "Crew", "mascotBody": body, "color": colour}
            for i, (name, body, colour, _) in enumerate(CREW)]
    (home / "bots.json").write_text(json.dumps(bots))
    out = subprocess.run([ENGINE, "--once", "--no-notify"], capture_output=True, text=True, env=env, check=True).stdout
    return {bot["id"]: picture(bot["avatar"]) for bot in json.loads(out)["bots"]}


def render(svg, slot):
    """An avatar picture drawn `slot` wide for its 0..100 body box, as an RGBA image and the offset of its
    corner from the body's corner (the picture's viewBox is the body box widened by the ribbons' room)."""
    x, y, w, _ = map(float, re.search(r'viewBox="([^"]+)"', svg).group(1).split())
    px = round(slot * w / 100)
    png = subprocess.run(["rsvg-convert", "-f", "png", "-w", str(px), "-h", str(px)], input=svg.encode(),
                         capture_output=True, check=True).stdout
    return Image.open(io.BytesIO(png)).convert("RGBA"), (round(slot * x / 100), round(slot * y / 100))


def place(canvas, svg, slot, centre):
    im, (dx, dy) = render(svg, slot)
    canvas.alpha_composite(im, (round(centre[0] - slot / 2) + dx, round(centre[1] - slot / 2) + dy))


def crew_row(pictures):
    row = Image.new("RGBA", ((len(CREW) - 1) * CELL_PITCH + CELL, CELL), (0, 0, 0, 0))
    for i in range(len(CREW)):
        place(row, pictures[f"b{i}"], SLOT, (i * CELL_PITCH + CELL / 2, CELL / 2))
    return row


def best_loop(lines):
    """The start, in 20 fps frames, of the 10 s whose end looks most like its start, so the loop does not jump."""
    frames = []  # the picture each 50 ms tick shows: the last line sent at or before it
    tick, i = FIRST_S, 0
    while tick + LOOP_S + 0.05 < lines[-1][0]:
        while i + 1 < len(lines) and lines[i + 1][0] <= tick:
            i += 1
        frames.append(lines[i][1])
        tick += 1 / FPS
    small = [crew_row_small(f) for f in frames]
    span = FPS * LOOP_S
    start = min(range(len(frames) - span), key=lambda s: sum(abs(a - b) for a, b in zip(small[s], small[s + span])))
    say(f"loop starts {FIRST_S + start / FPS:.2f} s in")
    return frames[start:start + span]


def crew_row_small(pictures):
    """A cheap fingerprint of one tick, to compare two of them."""
    row = Image.new("RGBA", (len(CREW) * 24, 24), (0, 0, 0, 0))
    for i in range(len(CREW)):
        place(row, pictures[f"b{i}"], 14, (i * 24 + 12, 12))
    return row.tobytes()


def save_loop(path, frames):
    frames[0].save(path, save_all=True, append_images=frames[1:], duration=1000 // FPS, loop=0,
                   lossless=True, quality=100, method=4, minimize_size=True)
    say(f"wrote {path} ({len(frames)} frames, {path.stat().st_size // 1024} KiB)")


def motion_frames(pictures, marks):
    """The Usage animation: the mark, then the avatars, in a panel-coloured rounded card."""
    w, h = MOTION_SIZE
    card = Image.new("L", (w * 4, h * 4), 0)
    ImageDraw.Draw(card).rounded_rectangle((0, 0, w * 4 - 1, h * 4 - 1), MOTION_RADIUS * 4, fill=255)
    mask = card.resize(MOTION_SIZE, Image.LANCZOS)
    columns = len(CREW) + 1
    out = []
    for tick, mark in enumerate(marks):
        frame = Image.new("RGBA", MOTION_SIZE, MOTION_BG)
        frame.alpha_composite(mark, (round(w / columns / 2 - mark.width / 2), round((h - mark.height) / 2)))
        for i in range(len(CREW)):
            place(frame, pictures[tick][f"b{i}"], MOTION_SLOT, (w * (i + 1.5) / columns, h / 2))
        frame.putalpha(mask)
        out.append(frame)
    return out


def main():
    if not ENGINE.is_file():
        raise SystemExit(f"make-crew: {ENGINE} is missing (run 'mise run build')")
    if shutil.which("rsvg-convert") is None:
        raise SystemExit("make-crew: rsvg-convert is needed (librsvg)")
    with tempfile.TemporaryDirectory(prefix="mission-crew.") as tmp:
        work = Path(tmp)
        lines = record(work)
        still = rest(work)
    loop = best_loop(lines)
    save_loop(CREW_OUT, [crew_row(f) for f in loop])
    crew_row(still).save(STILL_OUT, optimize=True)
    say(f"wrote {STILL_OUT}")
    marks = [f.convert("RGBA") for f in ImageSequence.Iterator(Image.open(MARK))]
    save_loop(MOTION_OUT, motion_frames(loop, marks))


main()
