"""Render the first slide of assets/presentation.pptx to a Pillow image.

Shared by make-preview-png.py and make-preview-webp.py. It understands only
what the preview slide uses: a picture background and placed pictures, with
optional crop and outer shadow.
"""
import math
import re
import zipfile
from io import BytesIO
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

ROOT = Path(__file__).resolve().parent.parent
PPTX = ROOT / "assets" / "presentation.pptx"
EMU_PER_PX = 7620  # 1920x1080 pt slide exported at 2400x1350 px


def px(emu):
    return emu / EMU_PER_PX


def render(pptx=PPTX):
    with zipfile.ZipFile(pptx) as z:
        slide = z.read("ppt/slides/slide1.xml").decode()
        rels = dict(re.findall(
            r'Id="([^"]*)"[^>]*Target="\.\./media/([^"]*)"',
            z.read("ppt/slides/_rels/slide1.xml.rels").decode()))

        def media(rid):
            return Image.open(BytesIO(z.read("ppt/media/" + rels[rid]))).convert("RGBA")

        bg = re.search(r'<p:bg>.*?r:embed="([^"]*)"', slide, re.S).group(1)
        canvas = media(bg)
        for pic in re.findall(r"<p:pic>.*?</p:pic>", slide, re.S):
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
