"""Run with python tools/test_ci_weight.py [revision]."""
import pathlib
import re
import subprocess
import sys

root = pathlib.Path(__file__).resolve().parents[1]

def read(path):
    if len(sys.argv) > 1:
        return subprocess.check_output(
            ["git", "show", f"{sys.argv[1]}:{path}"], cwd=root, text=True
        )
    return (root / path).read_text()

mascot = read("crates/halo-core/src/mascot.rs")
script = read("tools/ci-weight.ps1")

def number(pattern, source):
    match = re.search(pattern, source)
    assert match, f"timing declaration missing: {pattern}"
    return float(match[1])

def timing(name):
    return number(rf"const {name}: f64 = ([\d.]+);", mascot)

warmup = number(r"Start-Sleep -Seconds (\d+)\s+\$result\.rest", script)
sample = number(r"\[int\]\$Seconds = (\d+)", script)
assert warmup > timing("DOZE_AFTER") + timing("DOZE_LEN"), "rest sample includes mascot doze animation"
assert warmup < timing("SLEEP_AFTER") < warmup + sample, "rest sample must retain the sleep wake"
assert sample == 15, "keep the existing measurement duration"
print("rest sampling excludes the doze transition and includes the sleep wake")
