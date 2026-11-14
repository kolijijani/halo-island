#!/usr/bin/env python3
"""Put the add-ons' files on their GitHub pre-releases, byte for byte the
files the app pins.

Every add-on file the app downloads from its own release is pinned in
crates/halo-core/src/addons.rs by name, size and SHA-256, under its add-on's
tag (`addon-sholat-3`, `addon-ja-1`, ...). This gathers each tag's files from
where the repository and the builds keep them, checks every one against its
pin, and only then publishes. It never changes a pin: a file that doesn't
match is reported and the tag is not published.

Usage:
    python3 tools/publish_addons.py stage [--out DIR] [--from DIR ...]
        Gather every tag's files into DIR/<tag>/ (default dist/addons) and
        check them against the pins. Changes nothing online. Exit 1 when a
        file is missing or doesn't match.
    python3 tools/publish_addons.py commands [--out DIR]
        Print the gh commands that publish the staged tags.
    python3 tools/publish_addons.py publish [--out DIR] [--tag TAG ...]
        Stage, then run those commands (needs gh, signed in with a token that
        may write releases), then download every asset back and check it.
    python3 tools/publish_addons.py verify [--tag TAG ...]
        Download every pinned asset from its release and check it.

Where a tag's files come from:
    addon-sholat-3             addons/sholat/places.json, adzan.m4a, sholat-3/NOTICE.txt
    addon-sholat-4             addons/sholat/adzan-makkah.m4a, adzan-subuh.m4a, NOTICE.txt
    addon-ja-1                 crates/halo-core/data/ja-readings.dic.zst, addons/ja/NOTICE.txt
    addon-downloader-1         ffmpeg.exe (tools/build-ffmpeg-min.sh), addons/downloader/NOTICE.txt
    addon-downloader-mac-1     ffmpeg and qjs (mac.yml's downloader-tools job),
                               addons/downloader-mac/NOTICE.txt
A built file is looked for in each --from DIR (and dist/ffmpeg/windows,
dist/downloader-mac): ffmpeg.exe, ffmpeg-mac or ffmpeg, qjs-mac or qjs.
yt-dlp and QuickJS-ng's Windows build come from their own releases (a `url`
in the pin), so they are no asset of ours.

Never delete or replace an asset on an older tag: older versions download it.
Python 3 standard library only.
"""

import argparse
import hashlib
import os
import re
import shutil
import subprocess
import sys
import tempfile
import urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ADDONS_RS = os.path.join(ROOT, "crates", "halo-core", "src", "addons.rs")
REPO = "kolijijani/halo-island"
RELEASES = f"https://github.com/{REPO}/releases/download"

# Where each asset is kept in the repository, or the names a build gives it.
SOURCES = {
    ("addon-sholat-3", "places.json"): ["addons/sholat/places.json"],
    ("addon-sholat-3", "adzan.m4a"): ["addons/sholat/adzan.m4a"],
    # The notice as addon-sholat-3 pins it; addons/sholat/NOTICE.txt went on
    # to credit the voices, and goes on addon-sholat-4.
    ("addon-sholat-3", "NOTICE.txt"): ["addons/sholat/sholat-3/NOTICE.txt"],
    ("addon-sholat-4", "adzan-makkah.m4a"): ["addons/sholat/adzan-makkah.m4a"],
    ("addon-sholat-4", "adzan-subuh.m4a"): ["addons/sholat/adzan-subuh.m4a"],
    ("addon-sholat-4", "NOTICE.txt"): ["addons/sholat/NOTICE.txt"],
    ("addon-ja-1", "ja-readings.dic.zst"): ["crates/halo-core/data/ja-readings.dic.zst"],
    ("addon-ja-1", "NOTICE.txt"): ["addons/ja/NOTICE.txt"],
    ("addon-downloader-1", "ffmpeg.exe"): ["build:ffmpeg.exe", "dist/ffmpeg/windows/ffmpeg.exe"],
    ("addon-downloader-1", "NOTICE.txt"): ["addons/downloader/NOTICE.txt"],
    ("addon-downloader-mac-1", "ffmpeg"): ["build:ffmpeg-mac", "build:ffmpeg", "dist/downloader-mac/ffmpeg-mac"],
    ("addon-downloader-mac-1", "qjs"): ["build:qjs-mac", "build:qjs", "dist/downloader-mac/qjs-mac"],
    ("addon-downloader-mac-1", "NOTICE.txt"): ["addons/downloader-mac/NOTICE.txt"],
}

# What each pre-release says.
NOTES = {
    "addon-sholat-3": "Prayer times add-on: the place list, the adzan (Aaqib Azeez) and its notice.",
    "addon-sholat-4": "Prayer times add-on: adzan voices, each downloaded only when picked (Makkah; Subuh's own). Licences in addons/sholat/NOTICE.txt.",
    "addon-ja-1": "Japanese readings add-on: Halo Island's reading dictionary and its notice.",
    "addon-downloader-1": "Downloader add-on (Windows): our minimal FFmpeg build and the notice. yt-dlp and QuickJS-ng come from their own releases.",
    "addon-downloader-mac-1": "Downloader add-on (Mac): our FFmpeg and QuickJS-ng builds and the notice. yt-dlp comes from its own release.",
}


def pins():
    """Every pinned file of ours in addons.rs: {tag: [(name, size, sha256)]}.
    Files with a `url` of their own (yt-dlp, QuickJS-ng's Windows build) and
    add-ons without files are left out."""
    src = open(ADDONS_RS, encoding="utf-8").read()
    out = {}
    for block in re.finditer(r"pub const (\w+): Addon = Addon \{(.*?)\n\};", src, re.S):
        body = block.group(2)
        tag = re.search(r'tag: "([^"]*)"', body)
        if not tag or not tag.group(1):
            continue
        for f in re.finditer(r'AddonFile \{\s*name: ([^,]+),\s*size: ([\d_]+),\s*sha256: "([0-9a-f]{64})",\s*url: (None|Some)', body):
            if f.group(4) == "Some":
                continue
            name = f.group(1).strip()
            if name.startswith('"'):
                name = name.strip('"')
            else:  # a constant, as JA_READINGS_FILE
                name = re.search(rf'pub const {name}: &str = "([^"]+)";', src).group(1)
            out.setdefault(tag.group(1), []).append((name, int(f.group(2).replace("_", "")), f.group(3)))
    return out


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 16), b""):
            h.update(chunk)
    return h.hexdigest()


def find(tag, name, size, digest, froms):
    """The bytes for (tag, name) that match the pin, and where they came from;
    else None and what was found instead."""
    tried = []
    for src in SOURCES.get((tag, name), []):
        if src.startswith("build:"):
            cands = [os.path.join(d, src[6:]) for d in froms]
        else:
            cands = [os.path.join(ROOT, src)]
        for c in cands:
            if not os.path.isfile(c):
                continue
            got = (os.path.getsize(c), sha256(c))
            if got == (size, digest):
                with open(c, "rb") as f:
                    return f.read(), os.path.relpath(c, ROOT)
            tried.append(f"{os.path.relpath(c, ROOT)}: {got[0]} bytes, {got[1]}")
    return None, tried


def stage(out, froms):
    ok = True
    for tag, files in sorted(pins().items()):
        folder = os.path.join(out, tag)
        os.makedirs(folder, exist_ok=True)
        for name, size, digest in files:
            data, where = find(tag, name, size, digest, froms)
            if data is None:
                ok = False
                print(f"MISSING  {tag}/{name}: pinned {size} bytes {digest}")
                for t in where:
                    print(f"         not it: {t}")
                continue
            with open(os.path.join(folder, name), "wb") as f:
                f.write(data)
            print(f"ok       {tag}/{name}  {size} bytes  {digest}  (from {where})")
        with open(os.path.join(folder, "SHA256SUMS"), "w") as f:
            for name, _, digest in files:
                f.write(f"{digest}  {name}\n")
    return ok


def ready(out, tag):
    files = pins()[tag]
    return all(os.path.isfile(os.path.join(out, tag, n)) and sha256(os.path.join(out, tag, n)) == d for n, _, d in files)


def commands(out, tags):
    cmds = []
    for tag in tags:
        folder = os.path.relpath(os.path.join(out, tag), ROOT)
        names = [n for n, _, _ in pins()[tag]]
        cmds.append(f"(cd {folder} && sha256sum -c SHA256SUMS)")
        assets = " ".join(f"{folder}/{n}" for n in names)
        cmds.append(f'gh release create {tag} --repo {REPO} --prerelease --title "{tag}" --notes "{NOTES.get(tag, tag)}" {assets}')
    return cmds


def verify(tags):
    ok = True
    for tag in tags:
        for name, size, digest in pins()[tag]:
            url = f"{RELEASES}/{tag}/{name}"
            try:
                req = urllib.request.Request(url, headers={"User-Agent": "halo-island-publish"})
                with urllib.request.urlopen(req, timeout=120) as r:
                    data = r.read()
                got = (r.status, len(data), hashlib.sha256(data).hexdigest())
            except Exception as e:  # noqa: BLE001 (a 404 is an answer here)
                got = (getattr(e, "code", 0), 0, "")
            right = got == (200, size, digest)
            ok &= right
            print(f"{'ok' if right else 'WRONG':8} {url}  HTTP {got[0]}, {got[1]} bytes")
    return ok


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("action", choices=["stage", "commands", "publish", "verify"])
    ap.add_argument("--out", default=os.path.join(ROOT, "dist", "addons"))
    ap.add_argument("--from", dest="froms", action="append", default=[], help="a folder with built files (ffmpeg.exe, ffmpeg-mac, qjs-mac)")
    ap.add_argument("--tag", action="append", help="only these tags")
    a = ap.parse_args()
    froms = a.froms + [os.path.join(ROOT, "dist", "ffmpeg", "windows"), os.path.join(ROOT, "dist", "downloader-mac")]
    tags = a.tag or sorted(pins())
    unknown = [t for t in tags if t not in pins()]
    if unknown:
        sys.exit(f"no add-on pins tag {', '.join(unknown)}")
    if a.action == "verify":
        sys.exit(0 if verify(tags) else 1)
    ok = stage(a.out, froms)
    if a.action == "stage":
        sys.exit(0 if ok else 1)
    todo = [t for t in tags if ready(a.out, t)]
    for t in tags:
        if t not in todo:
            print(f"# {t}: not every file matches its pin, so it isn't published", file=sys.stderr)
    if a.action == "commands":
        print("\n".join(commands(a.out, todo)))
        sys.exit(0 if ok else 1)
    if not shutil.which("gh"):
        sys.exit("gh isn't installed here: run `commands` and publish from a machine where gh is signed in")
    for c in commands(a.out, todo):
        print(f"$ {c}")
        subprocess.run(c, shell=True, check=True, cwd=ROOT)
    sys.exit(0 if verify(todo) and ok else 1)


if __name__ == "__main__":
    main()
