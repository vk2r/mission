#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pillow==13.0.0"]
# ///
"""Build preview.png and preview.webp from the first slide of assets/presentation.pptx.

Both are the slide with the MausBot pill stretched to hold the crew (Atlas,
Quill, Nova, Sage and Bolt) after the name, "MausBot [crew]". preview.webp is
the animated one the README shows, with the crew from assets/preview/crew.webp,
a 10 s loop at 20 fps where each avatar moves differently; preview.png is the
still the plugin marketplace shows, with the crew at rest from
assets/preview/crew-still.png. scripts/make-crew.py records both from the engine.

Usage: scripts/make-preview.py [output-dir]   (default: the repository root)
"""
import math
import re
import sys
import zipfile
from io import BytesIO
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter, ImageSequence

ROOT = Path(__file__).resolve().parent.parent
PPTX = ROOT / "assets" / "presentation.pptx"
CREW = ROOT / "assets" / "preview" / "crew.webp"
CREW_STILL = ROOT / "assets" / "preview" / "crew-still.png"
EMU_PER_PX = 7620  # 1920x1080 pt slide exported at 2400x1350 px

PILL = "pill-mausbot"
CREW_COUNT = 5  # Atlas, Quill, Nova, Sage and Bolt, in the order crew.webp has them
CREW_SCALE = 0.64  # crew height against the pill's text
CELL, CELL_PITCH = 80, 82  # each avatar's cell in crew.webp, and how far apart they are
PITCH = 58        # how far apart they go in the pill, before CREW_SCALE
TEXT_END = 171    # first pill column past the "t" of "MausBot"
GAP = 14          # between "MausBot" and the first avatar
CAP = 24          # the pill's rounded end, short of the "M"
END_PAD = 28      # last avatar to the pill's edge, as the edge to "M" on the left


def px(emu):
    return emu / EMU_PER_PX


def render(skip=(), keep=None):
    """Draw the slide. It understands only what the slide uses: a picture
    background and placed pictures, with optional crop and outer shadow.
    Pictures named in skip are left out; if keep is a dict, each of them is
    stored there as (image, (x, y)) instead."""
    with zipfile.ZipFile(PPTX) as z:
        slide = z.read("ppt/slides/slide1.xml").decode()
        rels = dict(re.findall(
            r'Id="([^"]*)"[^>]*Target="\.\./media/([^"]*)"',
            z.read("ppt/slides/_rels/slide1.xml.rels").decode()))

        def media(rid):
            return Image.open(BytesIO(z.read("ppt/media/" + rels[rid]))).convert("RGBA")

        bg = re.search(r'<p:bg>.*?r:embed="([^"]*)"', slide, re.S).group(1)
        canvas = media(bg)
        for pic in re.findall(r"<p:pic>.*?</p:pic>", slide, re.S):
            name = re.search(r'name="([^"]*)"', pic).group(1)
            im = media(re.search(r'r:embed="([^"]*)"', pic).group(1))
            x, y, cx, cy = map(int, re.search(
                r'<a:off x="(-?\d+)" y="(-?\d+)"/><a:ext cx="(\d+)" cy="(\d+)"', pic).groups())
            crop = re.search(r"<a:srcRect([^>]*)/>", pic)
            if crop:
                a = {k: int(v) for k, v in re.findall(r'(\w)="(-?\d+)"', crop.group(1))}
                w, h = im.size
                im = im.crop((round(w * a.get("l", 0) / 1e5), round(h * a.get("t", 0) / 1e5),
                              round(w - w * a.get("r", 0) / 1e5), round(h - h * a.get("b", 0) / 1e5)))
            X, Y, W, H = round(px(x)), round(px(y)), round(px(cx)), round(px(cy))
            im = im.resize((W, H), Image.LANCZOS)
            if name in skip:
                if keep is not None:
                    keep[name] = (im, (X, Y))
                continue
            shadow = re.search(
                r'<a:outerShdw blurRad="(\d+)" dist="(\d+)" dir="(\d+)".*?<a:alpha val="(\d+)"', pic, re.S)
            if shadow:
                blur, dist, angle, alpha = map(int, shadow.groups())
                rad = math.radians(angle / 60000)
                dx, dy = px(dist) * math.cos(rad), px(dist) * math.sin(rad)
                mask = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
                ImageDraw.Draw(mask).rectangle([X + dx, Y + dy, X + dx + W, Y + dy + H],
                                               fill=(0, 0, 0, round(255 * alpha / 1e5)))
                canvas.alpha_composite(mask.filter(ImageFilter.GaussianBlur(px(blur) / 2)))
            canvas.alpha_composite(im, (X, Y))
    return canvas


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


def arrange(strip):
    """Lay out the avatars of a crew.webp frame, PITCH apart."""
    out = Image.new("RGBA", ((CREW_COUNT - 1) * PITCH + CELL, strip.height), (0, 0, 0, 0))
    for j in range(CREW_COUNT):
        x = j * CELL_PITCH
        out.alpha_composite(strip.crop((x, 0, x + CELL, strip.height)), (j * PITCH, 0))
    return out


def make_frames():
    """The slide with the crew in the pill, one frame per crew.webp frame, and the still one with the crew at
    rest where the loop's avatars stand."""
    kept = {}
    slide = render(skip=(PILL,), keep=kept)
    pill, (pill_x, pill_y) = kept[PILL]

    source = Image.open(CREW)
    durations = [f.info.get("duration", 50) for f in ImageSequence.Iterator(source)]
    def fit(f):
        f = arrange(f.convert("RGBA"))
        return f.resize((round(f.width * CREW_SCALE), round(f.height * CREW_SCALE)), Image.LANCZOS)

    crew = [fit(f) for f in ImageSequence.Iterator(source)]
    l, t, r, b = visible_box(crew)
    inner = GAP + (r - l) + END_PAD - CAP
    slide.alpha_composite(stretch_pill(pill, inner), (pill_x, pill_y))
    crew_at = (pill_x + TEXT_END + GAP - l, pill_y + round((pill.height - (b - t)) / 2) - t)

    frames = []
    for f in crew:
        frame = slide.copy()
        frame.alpha_composite(f, crew_at)
        frames.append(frame.convert("RGB"))
    still = slide.copy()
    still.alpha_composite(fit(Image.open(CREW_STILL)), crew_at)
    return frames, durations, still.convert("RGB")


def make_webp(out, frames, durations):
    frames[0].save(out, save_all=True, append_images=frames[1:], duration=durations, loop=0,
                   lossless=True, quality=100, method=4, minimize_size=True)
    print(f"make-preview: wrote {out} ({len(frames)} frames)")


def make_png(out, frame):
    frame.save(out, optimize=True)
    print(f"make-preview: wrote {out}")


dest = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT
frames, durations, still = make_frames()
make_png(dest / "preview.png", still)
make_webp(dest / "preview.webp", frames, durations)
