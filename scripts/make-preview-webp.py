#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pillow==13.0.0"]
# ///
"""Build preview.webp: the animated preview the README shows.

It is the slide in assets/presentation.pptx with the crew (Atlas, Nova, Quill,
Bolt and Sage) animating under "See your crew in action". The crew comes from
assets/preview/crew.webp, a 10 s loop at 20 fps recorded from the real panel.
Usage: scripts/make-preview-webp.py [output]   (default: preview.webp)
"""
import sys

sys.dont_write_bytecode = True  # keep scripts/ free of __pycache__

from PIL import Image, ImageSequence

from preview_slide import ROOT, render

CREW = ROOT / "assets" / "preview" / "crew.webp"
CREW_AT = (305, 350)  # Atlas's visible left edge lines up with the "S" of "See"

out = sys.argv[1] if len(sys.argv) > 1 else ROOT / "preview.webp"
slide = render()
crew = Image.open(CREW)
frames, durations = [], []
for f in ImageSequence.Iterator(crew):
    frame = slide.copy()
    frame.alpha_composite(f.convert("RGBA"), CREW_AT)
    frames.append(frame.convert("RGB"))
    durations.append(f.info.get("duration", 50))
frames[0].save(out, save_all=True, append_images=frames[1:], duration=durations, loop=0,
               lossless=True, quality=100, method=4, minimize_size=True)
print(f"make-preview-webp: wrote {out} ({len(frames)} frames)")
