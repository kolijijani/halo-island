#!/usr/bin/env python3
"""Render mockups/halo-island-mockup.html to an MP4 and a poster PNG.

Each frame is drawn with the page's own seek(t) and screenshotted, so no frame
is dropped however slow the machine is. Frames are piped straight to ffmpeg.

    python3 mockups/render_mockup.py [--fps 30] [--poster-at 9.5]
"""
import argparse
import pathlib
import subprocess

from playwright.sync_api import sync_playwright

HERE = pathlib.Path(__file__).resolve().parent


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--fps", type=int, default=30)
    ap.add_argument("--poster-at", type=float, default=9.5, help="seconds; the media player scene")
    ap.add_argument("--out", default=str(HERE / "halo-island-mockup.mp4"))
    ap.add_argument("--poster", default=str(HERE / "halo-island-poster.png"))
    a = ap.parse_args()

    page_url = (HERE / "halo-island-mockup.html").as_uri() + "?capture"
    with sync_playwright() as p:
        browser = p.chromium.launch()
        page = browser.new_page(viewport={"width": 1920, "height": 1080}, device_scale_factor=1)
        page.goto(page_url)
        page.evaluate("document.fonts.ready")
        duration = page.evaluate("window.DURATION")
        frames = round(duration * a.fps)

        ff = subprocess.Popen(
            ["ffmpeg", "-y", "-loglevel", "error", "-f", "image2pipe", "-framerate", str(a.fps), "-c:v", "png", "-i", "-",
             "-c:v", "libx264", "-preset", "slow", "-crf", "18", "-pix_fmt", "yuv420p", "-movflags", "+faststart",
             "-r", str(a.fps), a.out],
            stdin=subprocess.PIPE,
        )
        try:
            for i in range(frames):
                page.evaluate(f"seek({i / a.fps})")
                ff.stdin.write(page.screenshot(type="png"))
                if i % (a.fps * 5) == 0:
                    print(f"frame {i}/{frames}", flush=True)
        finally:
            ff.stdin.close()
            ff.wait()
        if ff.returncode:
            raise SystemExit(f"ffmpeg failed ({ff.returncode})")

        page.evaluate(f"seek({a.poster_at})")
        page.screenshot(path=a.poster)
        browser.close()
    print(f"wrote {a.out} ({frames} frames) and {a.poster}")


if __name__ == "__main__":
    main()
