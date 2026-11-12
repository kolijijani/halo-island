#!/usr/bin/env python3
"""Draw mockups/halo-island-mascot-eyes-only.png: the Halo mascot (eyes only, no ring)
on the notch pill, dark and light, in five states.

Every pill is drawn 4x larger and shrunk with LANCZOS so the edges come out smooth.
Sizes are in notch px (pill 32 px tall), as in docs/A2Z-07-mascot-ideas.md.

    python3 mockups/render_mascot_eyes.py
"""
import math
import pathlib

from PIL import Image, ImageDraw, ImageFont

HERE = pathlib.Path(__file__).resolve().parent
SS = 4                      # supersampling
ZOOM = 5                    # big pill = 5x notch px
PILL_W, PILL_H = 104, 32
EYE_DX = 9                  # eye centre from pill centre: 8 px eye + 10 px gap

THEMES = {
    "dark": dict(pill="#0B0D12", eye="#F2F5FA", band="#15181E", text="#F2F5FA", dim="#8A93A3", edge=None),
    "light": dict(pill="#F3F5F9", eye="#141821", band="#FFFFFF", text="#141821", dim="#6B7484", edge="#D5DAE3"),
}
STATES = [
    ("awake", "jaga", "kapsul 8×16, radius 4"),
    ("blink", "kerdip", "kapsul 8×2, 120 ms"),
    ("doze", "mengantuk", "kelopak separuh 12×4"),
    ("sleep", "tidur", "dua lengkung tertutup"),
    ("wave", "melambai", "mata gembira + tangan ±20°"),
]


def rgba(hex_, a=255):
    h = hex_.lstrip("#")
    return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4)) + (a,)


def capsule(d, x, y, w, h, s, fill):
    d.rounded_rectangle([x - w * s / 2, y - h * s / 2, x + w * s / 2, y + h * s / 2], radius=min(w, h) * s / 2, fill=fill)


def arc(d, x, y, rx, ry, start, end, stroke, s, fill):
    """Elliptic arc with round caps; Pillow draws the stroke inside the box."""
    w = stroke * s
    d.arc([x - rx * s, y - ry * s, x + rx * s, y + ry * s], start, end, fill=fill, width=round(w))
    for ang in (start, end):
        t = math.radians(ang)
        px, py = x + (rx * s - w / 2) * math.cos(t), y + (ry * s - w / 2) * math.sin(t)
        d.ellipse([px - w / 2, py - w / 2, px + w / 2, py + w / 2], fill=fill)


def hand(im, x, y, s, angle, fill):
    """4x9 capsule pivoting at its bottom end (x, y)."""
    layer = Image.new("RGBA", im.size, (0, 0, 0, 0))
    capsule(ImageDraw.Draw(layer), x, y - 4.5 * s, 4, 9, s, fill)
    im.alpha_composite(layer.rotate(angle, center=(x, y), resample=Image.BICUBIC))


def pill(state, theme, zoom):
    t, s = THEMES[theme], zoom * SS
    im = Image.new("RGBA", (PILL_W * s, PILL_H * s), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    d.rounded_rectangle([0, 0, im.width - 1, im.height - 1], radius=im.height / 2, fill=t["pill"],
                        outline=t["edge"], width=SS if t["edge"] else 0)
    cx, cy, eye = im.width / 2, im.height / 2, rgba(t["eye"])
    for side in (-1, 1):
        ex = cx + side * EYE_DX * s
        if state == "awake":
            capsule(d, ex, cy, 8, 16, s, eye)
        elif state == "blink":
            capsule(d, ex, cy, 8, 2, s, eye)
        elif state == "doze":
            capsule(d, ex, cy + 2 * s, 12, 4, s, eye)
        elif state == "sleep":
            arc(d, ex, cy - 1 * s, 5, 4, 0, 180, 2.5, s, eye)      # ∪
        elif state == "wave":
            arc(d, ex, cy + 3 * s, 5, 5, 180, 360, 2.5, s, eye)    # ∩
    if state == "wave":
        hx, hy = cx + 25 * s, cy + 7 * s
        hand(im, hx, hy, s, 20, rgba(t["eye"], 70))   # where the wave swings back to
        hand(im, hx, hy, s, -20, eye)
    return im.resize((im.width // SS, im.height // SS), Image.LANCZOS)


def main():
    font = lambda w, px: ImageFont.truetype(f"/usr/share/fonts/opentype/inter/Inter-{w}.otf", px)
    title, label, small = font("SemiBold", 44), font("SemiBold", 30), font("Regular", 22)
    left, colw, top, rowh, gap = 200, 600, 190, 400, 30
    W, H = left + colw * len(STATES) + 60, top + 2 * rowh + gap + 60
    img = Image.new("RGBA", (W, H), rgba("#ECEEF2"))
    d = ImageDraw.Draw(img)
    d.text((60, 56), "Halo Island · maskot rasmi “Halo”, mata sahaja (tanpa cincin)", font=title, fill="#141821")
    d.text((60, 120), "Dua kapsul tegak 8×16 px, radius penuh, jarak 10 px, pada notch 32 px. "
           "Pill besar dilukis 5×; pill kecil ialah saiz sebenar 1×.", font=small, fill="#5A6272")

    for r, theme in enumerate(THEMES):
        t, y0 = THEMES[theme], top + r * (rowh + gap)
        d.rounded_rectangle([40, y0, W - 40, y0 + rowh], radius=28, fill=t["band"],
                            outline="#D5DAE3" if theme == "light" else None)
        d.text((70, y0 + 100), theme.capitalize(), font=label, fill=t["text"])
        for i, (name, bm, note) in enumerate(STATES):
            x0 = left + i * colw + (colw - PILL_W * ZOOM) // 2
            img.alpha_composite(pill(name, theme, ZOOM), (x0, y0 + 36))
            d.text((x0 + 8, y0 + 222), f"{name} · {bm}", font=label, fill=t["text"])
            d.text((x0 + 8, y0 + 266), note, font=small, fill=t["dim"])
            img.alpha_composite(pill(name, theme, 1), (x0 + 8, y0 + 320))
            d.text((x0 + 128, y0 + 322), "1×", font=small, fill=t["dim"])

    out = HERE / "halo-island-mascot-eyes-only.png"
    img.convert("RGB").save(out, optimize=True)
    print(out, out.stat().st_size)


if __name__ == "__main__":
    main()
