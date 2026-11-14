"""Render the GitHub repo social preview from tools/social_preview.html.

    python3 tools/make_social_preview.py

Writes docs/brand/social-preview.png at 1200x630. The card is drawn at 2x and
scaled down so the type and the notch stay sharp. Needs Playwright + Pillow,
the same as make_share_card.py.
"""
import io
from pathlib import Path

from PIL import Image
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "tools" / "social_preview.html"
OUT = ROOT / "docs" / "brand" / "social-preview.png"
W, H = 1200, 630


def main() -> None:
    with sync_playwright() as p:
        browser = p.chromium.launch()
        page = browser.new_page(viewport={"width": W, "height": H}, device_scale_factor=2)
        page.goto(SRC.as_uri(), wait_until="networkidle")
        page.evaluate("document.fonts.ready.then(() => true)")
        if not page.evaluate("document.fonts.check('600 78px Outfit')"):
            raise SystemExit("Outfit did not load (the card needs fonts.googleapis.com)")
        broken = page.evaluate("[...document.images].filter(i => !i.naturalWidth).map(i => i.src)")
        if broken:
            raise SystemExit(f"images did not load: {broken}")
        png = page.screenshot(type="png")
        browser.close()
    img = Image.open(io.BytesIO(png)).convert("RGB").resize((W, H), Image.LANCZOS)
    OUT.parent.mkdir(parents=True, exist_ok=True)
    img.save(OUT, "PNG")
    print(f"{OUT.relative_to(ROOT)}  {W}x{H}  {OUT.stat().st_size // 1024} KB")


if __name__ == "__main__":
    main()
