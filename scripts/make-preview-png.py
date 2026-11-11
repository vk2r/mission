#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pillow==13.0.0"]
# ///
"""Build preview.png: the still preview the plugin marketplace shows.

It is the slide in assets/presentation.pptx as is, without the animated crew.
Usage: scripts/make-preview-png.py [output]   (default: preview.png)
"""
import sys

sys.dont_write_bytecode = True  # keep scripts/ free of __pycache__

from preview_slide import ROOT, render

out = sys.argv[1] if len(sys.argv) > 1 else ROOT / "preview.png"
render().convert("RGB").save(out, optimize=True)
print(f"make-preview-png: wrote {out}")
