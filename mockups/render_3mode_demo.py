#!/usr/bin/env python3
"""Render mockups/halo-3mode-demo.html to a short MP4 + poster.

Same pipeline as render_full.py: deterministic seek(t) frames, parallel
browsers, ffmpeg concat (stream copy). See that file for full notes.

    python3 mockups/render_3mode_demo.py [--fps 30] [--jobs 6] [--poster-at 9.0]
"""
import argparse
import concurrent.futures
import pathlib
import subprocess
import tempfile

from playwright.sync_api import sync_playwright

HERE = pathlib.Path(__file__).resolve().parent
PAGE_URL = (HERE / "halo-3mode-demo.html").as_uri() + "?capture"
FFMPEG_OUT = ["-c:v", "libx264", "-preset", "slow", "-crf", "18", "-pix_fmt", "yuv420p", "-movflags", "+faststart"]


def open_page(p, errors):
    browser = p.chromium.launch()
    page = browser.new_page(viewport={"width": 1920, "height": 1080}, device_scale_factor=1)
    page.on("pageerror", lambda e: errors.append(str(e)))
    page.goto(PAGE_URL)
    page.evaluate("document.fonts.ready")
    return browser, page


def render_part(first, last, fps, out):
    errors = []
    with sync_playwright() as p:
        browser, page = open_page(p, errors)
        ff = subprocess.Popen(
            ["ffmpeg", "-y", "-loglevel", "error", "-f", "image2pipe", "-framerate", str(fps), "-c:v", "png", "-i", "-",
             *FFMPEG_OUT, "-r", str(fps), out],
            stdin=subprocess.PIPE,
        )
        try:
            for i in range(first, last):
                page.evaluate(f"seek({i / fps})")
                ff.stdin.write(page.screenshot(type="png"))
                if errors:
                    break
                if (i - first) % (fps * 5) == 0:
                    print(f"part {out.name}: frame {i - first}/{last - first}", flush=True)
        finally:
            ff.stdin.close()
            ff.wait()
            browser.close()
    if ff.returncode:
        errors.append(f"ffmpeg failed ({ff.returncode}) on {out.name}")
    return errors


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--fps", type=int, default=30)
    ap.add_argument("--poster-at", type=float, default=9.0, help="seconds; mini handle pulsing on a maximized app")
    ap.add_argument("--out", default=str(HERE / "halo-3mode-demo.mp4"))
    ap.add_argument("--poster", default=str(HERE / "halo-3mode-demo-poster.png"))
    ap.add_argument("--stills", help="comma-separated seconds: write PNGs of these frames and stop")
    ap.add_argument("--jobs", type=int, default=6)
    ap.add_argument("--stills-dir", default="/tmp/halo-3mode-stills")
    a = ap.parse_args()

    errors = []
    with sync_playwright() as p:
        browser, page = open_page(p, errors)
        duration = page.evaluate("window.DURATION")
        for s in page.evaluate("window.SCENES"):
            print(f"{s['seg']} {s['t0']:6.2f}-{s['t1']:6.2f}  {s['id']:10} {s['title']}")

        if a.stills:
            out = pathlib.Path(a.stills_dir)
            out.mkdir(parents=True, exist_ok=True)
            for t in map(float, a.stills.split(",")):
                page.evaluate(f"seek({t})")
                page.screenshot(path=str(out / f"t{t:06.2f}.png"))
        else:
            page.evaluate(f"seek({a.poster_at})")
            page.screenshot(path=a.poster)
        browser.close()
    if errors:
        raise SystemExit("page errors:\n" + "\n".join(errors))
    if a.stills:
        print(f"wrote stills to {a.stills_dir}")
        return

    frames = round(duration * a.fps)
    cuts = [frames * k // a.jobs for k in range(a.jobs + 1)]
    with tempfile.TemporaryDirectory() as tmp:
        parts = [pathlib.Path(tmp) / f"part{k}.mp4" for k in range(a.jobs)]
        with concurrent.futures.ProcessPoolExecutor(a.jobs) as pool:
            for errs in pool.map(render_part, cuts[:-1], cuts[1:], [a.fps] * a.jobs, parts):
                errors += errs
        if errors:
            raise SystemExit("page errors:\n" + "\n".join(errors))
        lst = pathlib.Path(tmp) / "parts.txt"
        lst.write_text("".join(f"file '{x}'\n" for x in parts))
        subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-f", "concat", "-safe", "0", "-i", str(lst),
                        "-c", "copy", "-movflags", "+faststart", a.out], check=True)
    print(f"wrote {a.out} ({frames} frames, {duration:.2f} s, {a.jobs} parts), {a.poster}; page errors: 0")


if __name__ == "__main__":
    main()
