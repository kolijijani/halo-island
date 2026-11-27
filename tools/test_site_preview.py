"""Run with python3 tools/test_site_preview.py (no network)."""
import os
from pathlib import Path
import subprocess
import tempfile
ROOT = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory() as tmp:
    gh = Path(tmp) / 'gh'
    gh.write_text('#!/bin/sh\nprintf "v0.2.54\\n"\n')
    gh.chmod(0o755)
    env = dict(os.environ, PATH=tmp + ':' + os.environ['PATH'])
    result = subprocess.run(['bash', 'tools/ci-site-check.sh', 'gate'], cwd=ROOT, env=env, text=True, capture_output=True, check=True)
    assert 'deploy=true' in result.stdout, result.stdout
page = (ROOT / 'site/index.html').read_text()
assert 'a.name === \'Halo-Island-Setup-\' + version + \'.exe\'' in page
assert 'el.href = setup.browser_download_url' in page
workflow = (ROOT / '.github/workflows/pages.yml').read_text()
assert "inputs.released != ''" not in workflow
assert 'python3 tools/make_update_feed.py latest.json' in workflow
print('PASS: pre-release deploy; versioned download asset; published-release feed retained')
