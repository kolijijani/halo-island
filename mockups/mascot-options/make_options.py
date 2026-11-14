"""Writes the four Halo mascot options as a grey British Shorthair with black
eyes: SVG, then 512x512 transparent PNG through headless Chrome.

Design phase only. The head is drawn here, not taken from halo-core's face:
that face is the old Oyenza mascot (orange, a tall pointed right ear, the left
ear's tip folded down, a flat-bottomed head). This one avoids all of it: both
ears small, round and level with inner shading, a head that widens into round
jowls below the eyes, no fold anywhere.
Run from the repo root: python3 -I mockups/mascot-options/make_options.py
"""
import pathlib
import subprocess

OUT = pathlib.Path(__file__).resolve().parent

# Fur, inner ear, eyes and outline, light grey for the ring's core.
FUR, INNER, INK, RING_CORE = "#7A8798", "#4A5562", "#171717", "#C9D1DA"

# Narrow crown, round jowls widest below the eye line, round chin.
HEAD = (
    "M0 -36C28 -36 45 -26 49 -9C52 4 58 13 56 26C53 40 30 47 0 47"
    "C-30 47 -53 40 -56 26C-58 13 -52 4 -49 -9C-45 -26 -28 -36 0 -36Z"
)
# Small rounded ears set wide, tipped outward, the same on both sides.
EARS = (
    "M18 -33C25 -45 36 -51 42 -48C47 -45 47 -33 45 -17Z"
    "M-18 -33C-25 -45 -36 -51 -42 -48C-47 -45 -47 -33 -45 -17Z"
)
INNERS = (
    "M26 -33C30 -40 35 -44 39 -43C42 -41 42 -33 41 -24Z"
    "M-26 -33C-30 -40 -35 -44 -39 -43C-42 -41 -42 -33 -41 -24Z"
)

ROUND = "".join(f'<circle cx="{x}" cy="8" r="8.5" fill="{INK}"/>' for x in (-21, 21))
CAPSULE = "".join(
    f'<rect x="{x - 5.5}" y="-3" width="11" height="22" rx="5.5" fill="{INK}"/>' for x in (-21, 21)
)
SMILE = (
    f'<path d="M-30 12Q-21 1 -12 12M12 12Q21 1 30 12" fill="none" stroke="{INK}" '
    'stroke-width="4.5" stroke-linecap="round"/>'
)


def head(eyes, outline=0):
    # Sticker outline: every shape stroked first, fills on top, so no seams show inside.
    under = (
        f'<path d="{HEAD}{EARS}" fill="{INK}" stroke="{INK}" '
        f'stroke-width="{outline * 2}" stroke-linejoin="round"/>'
        if outline else ""
    )
    return (
        f'{under}<path d="{EARS}" fill="{FUR}"/><path d="{INNERS}" fill="{INNER}"/>'
        f'<path d="{HEAD}" fill="{FUR}"/>{eyes}'
    )


RING = (
    f'<ellipse cx="0" cy="-54" rx="24" ry="6.5" fill="none" stroke="{INK}" stroke-width="9"/>'
    f'<ellipse cx="0" cy="-54" rx="24" ry="6.5" fill="none" stroke="{RING_CORE}" stroke-width="4.5"/>'
)

OPTIONS = {
    "01-mata-bulat": head(ROUND),
    "02-mata-kapsul": head(CAPSULE),
    # ponytail: ring and smaller head hand-placed so the ring clears the ears.
    "03-cincin-outline": RING + f'<g transform="translate(0 6) scale(0.82)">{head(ROUND, 3)}</g>',
    "04-gembira": head(SMILE),
    # The owner's pick: B in their own cat's colours (slate fur, charcoal inner
    # ear, light olive-yellow eyes; 4.17:1 on the fur).
    "chosen-b": head(CAPSULE).replace(FUR, "#626270").replace(INNER, "#29272D").replace(INK, "#E4D98A"),
}

for name, body in OPTIONS.items():
    svg = OUT / f"{name}.svg"
    svg.write_text(
        '<svg xmlns="http://www.w3.org/2000/svg" width="512" height="512" '
        f'viewBox="-60 -66 120 120">{body}</svg>\n'
    )
    subprocess.run(
        ["google-chrome", "--headless=new", "--disable-gpu", "--hide-scrollbars",
         "--default-background-color=00000000", "--window-size=512,512",
         f"--screenshot={OUT / (name + '.png')}", svg.as_uri()],
        check=True, capture_output=True,
    )
    print(name)
