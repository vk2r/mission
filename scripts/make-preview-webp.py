#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pillow==13.0.0"]
# ///
"""Build preview.webp: the animated preview the README shows.

It is the slide in assets/presentation.pptx with the MausBot pill stretched to
hold the crew (Atlas, Nova, Quill, Bolt and Sage) animating after the name:
"MausBot [crew]". preview.png keeps the plain pill. The crew comes from
assets/preview/crew.webp, a 10 s loop at 20 fps recorded from the real panel.
Usage: scripts/make-preview-webp.py [output]   (default: preview.webp)
"""
import sys

sys.dont_write_bytecode = True  # keep scripts/ free of __pycache__

from PIL import Image, ImageSequence

from preview_slide import ROOT, render

CREW = ROOT / "assets" / "preview" / "crew.webp"
PILL = "pill-mausbot"
CREW_SCALE = 0.8  # crew height against the pill's text
CELL, CELL_PITCH = 53, 66  # each avatar's cell in crew.webp, and how far apart they are
PITCH = 46        # how far apart they go in the pill, before CREW_SCALE
TEXT_END = 171    # first pill column past the "t" of "MausBot"
GAP = 14          # between "MausBot" and Atlas
CAP = 24          # the pill's rounded end, short of the "M"
END_PAD = 28      # Sage to the pill's edge, as the edge to "M" on the left


def visible_box(frames):
    """Union of the crew's visible pixels over the whole loop."""
    box = None
    for f in frames:
        b = f.getchannel("A").point(lambda v: 255 if v > 40 else 0).getbbox()
        box = b if box is None else (min(box[0], b[0]), min(box[1], b[1]),
                                     max(box[2], b[2]), max(box[3], b[3]))
    return box


def stretch_pill(pill, inner):
    """Make room for inner px after the text: the text part, a straight run
    of border and fill, and the left end mirrored as the right end."""
    w, h = pill.size
    run = pill.crop((TEXT_END - 10, 0, TEXT_END - 9, h))
    fill = run.getpixel((0, h // 2 - 20))
    for y in range(6, h - 6):
        if run.getpixel((0, y))[3] and abs(y - h / 2) < h / 2 - 6:
            run.putpixel((0, y), fill)
    right = pill.crop((0, 0, CAP, h)).transpose(Image.FLIP_LEFT_RIGHT)
    out = Image.new("RGBA", (TEXT_END + inner + CAP, h), (0, 0, 0, 0))
    out.paste(pill.crop((0, 0, TEXT_END, h)), (0, 0))
    out.paste(run.resize((inner, h)), (TEXT_END, 0))
    # the original end starts to curve under the "t": straighten its border
    for x in range(TEXT_END - 4, TEXT_END):
        for y in [*range(0, 6), *range(h - 6, h)]:
            out.putpixel((x, y), run.getpixel((0, y)))
    out.paste(right, (TEXT_END + inner, 0))
    return out


out = sys.argv[1] if len(sys.argv) > 1 else ROOT / "preview.webp"
kept = {}
slide = render(skip=(PILL,), keep=kept)
pill, (px_, py_) = kept[PILL]

source = Image.open(CREW)
durations = [f.info.get("duration", 50) for f in ImageSequence.Iterator(source)]


def tighten(strip):
    """Re-space the five avatars of a crew.webp frame PITCH apart."""
    n = (strip.width - CELL) // CELL_PITCH + 1
    out = Image.new("RGBA", ((n - 1) * PITCH + CELL, strip.height), (0, 0, 0, 0))
    for j in range(n):
        out.alpha_composite(strip.crop((j * CELL_PITCH, 0, j * CELL_PITCH + CELL, strip.height)), (j * PITCH, 0))
    return out


crew = []
for f in ImageSequence.Iterator(source):
    f = tighten(f.convert("RGBA"))
    crew.append(f.resize((round(f.width * CREW_SCALE), round(f.height * CREW_SCALE)), Image.LANCZOS))
l, t, r, b = visible_box(crew)
inner = GAP + (r - l) + END_PAD - CAP
slide.alpha_composite(stretch_pill(pill, inner), (px_, py_))
crew_at = (px_ + TEXT_END + GAP - l, py_ + round((pill.height - (b - t)) / 2) - t)

frames = []
for f in crew:
    frame = slide.copy()
    frame.alpha_composite(f, crew_at)
    frames.append(frame.convert("RGB"))
frames[0].save(out, save_all=True, append_images=frames[1:], duration=durations, loop=0,
               lossless=True, quality=100, method=4, minimize_size=True)
print(f"make-preview-webp: wrote {out} ({len(frames)} frames)")
