#!/usr/bin/env python3
"""Shared work ledger — one list of jobs for the maintainer, Qi-Cha and Antigravity.

Every job is a line in docs/ledger.jsonl (append-only JSON events, so git merges
never fight over it). An agent claims a job before it codes, so two agents can't
quietly work the same item, and nothing gets dropped.

  python3 tools/ledger.py add "Short title" --who qc [--ref L1] [--note "..."]
  python3 tools/ledger.py claim L1 --who ag --branch ag/L1
  python3 tools/ledger.py done  L1 --who ag --commit 949c0d3 --note "shipped in 0.2.55"
  python3 tools/ledger.py block L1 --who qc --note "waiting on owner decision"
  python3 tools/ledger.py list [--open] [--who ag]
  python3 tools/ledger.py check          # exit 1 on overlap, duplicate or stale claim
  python3 tools/ledger.py selftest
"""
import json
import os
import re
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LEDGER = os.path.join(ROOT, "docs", "ledger.jsonl")
STALE_HOURS = 72
OPEN = ("open", "doing", "blocked")


def who_default():
    return os.environ.get("AGENT_NAME") or "unknown"


def load():
    """Fold the event log into {id: job}, last event per field wins."""
    jobs = {}
    if not os.path.exists(LEDGER):
        return jobs
    with open(LEDGER, encoding="utf-8") as f:
        for n, line in enumerate(f, 1):
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            try:
                ev = json.loads(line)
            except json.JSONDecodeError:
                print(f"ledger.jsonl:{n}: not JSON, skipped", file=sys.stderr)
                continue
            job = jobs.setdefault(ev["id"], {"id": ev["id"], "title": "", "status": "open",
                                             "owner": "", "branch": "", "history": []})
            job["history"].append(ev)
            job["status"] = ev.get("ev", job["status"]) if ev.get("ev") in (
                "open", "doing", "done", "dropped", "blocked") else job["status"]
            for k in ("title", "owner", "branch", "note", "commit", "ref"):
                if ev.get(k):
                    job[k] = ev[k]
            job["ts"] = ev.get("ts", job.get("ts", 0))
    return jobs


def append(ev):
    ev.setdefault("ts", int(time.time()))
    line = json.dumps(ev, ensure_ascii=False) + "\n"
    with open(LEDGER, "a", encoding="utf-8") as f:
        f.write(line)  # one write, O_APPEND: safe for concurrent agents


def slug(title):
    return re.sub(r"[^a-z0-9]+", "-", title.lower()).strip("-")[:40]


def new_id(title, jobs):
    base = slug(title) or "job"
    if base not in jobs:
        return base
    i = 2
    while f"{base}-{i}" in jobs:
        i += 1
    return f"{base}-{i}"


def find(token, jobs):
    if token in jobs:
        return jobs[token]
    hits = [j for j in jobs.values() if token.lower() in j["title"].lower()]
    if len(hits) == 1:
        return hits[0]
    if not hits:
        sys.exit(f"no job matches {token!r}")
    sys.exit("ambiguous: " + ", ".join(j["id"] for j in hits))


def cmd_add(a, jobs):
    a["who"] = a["who"] or who_default()
    title = " ".join(a["args"]).strip()
    if not title:
        sys.exit("usage: add <title> [--who W] [--ref R] [--note N]")
    for job in jobs.values():
        if job["status"] in OPEN and slug(job["title"]) == slug(title):
            sys.exit(f"already in the ledger: {job['id']} ({job['status']}, {job['owner'] or 'unclaimed'})")
    jid = a.get("ref") or new_id(title, jobs)
    if jid in jobs:
        sys.exit(f"id {jid} already used")
    append({"id": jid, "ev": "open", "title": title, "owner": "", "who": a["who"],
            "ref": a.get("ref", ""), "note": a.get("note", "")})
    print(f"added {jid}: {title}")


def cmd_claim(a, jobs):
    a["who"] = a["who"] or who_default()
    job = find(a["args"][0], jobs)
    if job["status"] == "done" or job["status"] == "dropped":
        sys.exit(f"{job['id']} is {job['status']}; reopen it first")
    if job["status"] == "doing" and job["owner"] and job["owner"] != a["who"]:
        sys.exit(f"OVERLAP: {job['id']} already claimed by {job['owner']} on {job['branch'] or '?'}")
    append({"id": job["id"], "ev": "doing", "owner": a["who"],
            "branch": a.get("branch") or f"{a['who']}/{job['id']}", "who": a["who"]})
    print(f"{a['who']} owns {job['id']} ({job['title']})")


def cmd_done(a, jobs):
    a["who"] = a["who"] or who_default()
    job = find(a["args"][0], jobs)
    if job["status"] == "doing" and job["owner"] and job["owner"] != a["who"]:
        sys.exit(f"OVERLAP: {job['id']} is {job['owner']}'s; ask them or use block")
    append({"id": job["id"], "ev": "done", "owner": a["who"], "who": a["who"],
            "commit": a.get("commit", ""), "note": a.get("note", "")})
    print(f"done {job['id']}")


def cmd_block(a, jobs):
    a["who"] = a["who"] or who_default()
    job = find(a["args"][0], jobs)
    append({"id": job["id"], "ev": "blocked", "owner": a["who"], "who": a["who"],
            "note": a.get("note", "")})
    print(f"blocked {job['id']}: {a.get('note', '')}")


def cmd_list(a, jobs):
    rows = sorted(jobs.values(), key=lambda j: (j["status"] != "doing", j["id"]))
    if a.get("open"):
        rows = [j for j in rows if j["status"] in OPEN]
    if a.get("who_set"):
        rows = [j for j in rows if j["owner"] == a["who"]]
    if not rows:
        print("nothing open — ledger is clear")
        return
    for j in rows:
        age = int((time.time() - j.get("ts", 0)) / 3600)
        print(f"{j['id']:<28} {j['status']:<8} {(j['owner'] or '-'):<4} {age:>4}h  {j['title']}"
              + (f"  [{j['branch']}]" if j.get("branch") and j["status"] == "doing" else ""))


def cmd_check(a, jobs):
    bad = 0
    seen = {}
    for j in jobs.values():
        key = slug(j["title"])
        if key and j["status"] in OPEN:
            if key in seen:
                print(f"DUPLICATE: {seen[key]} and {j['id']} are the same job")
                bad += 1
            seen[key] = j["id"]
        if j["status"] == "doing":
            owners = {e["owner"] for e in j["history"] if e.get("ev") == "doing" and e.get("owner")}
            if len(owners) > 1:
                print(f"OVERLAP: {j['id']} claimed by {', '.join(sorted(owners))}")
                bad += 1
            age = (time.time() - j.get("ts", 0)) / 3600
            if age > STALE_HOURS:
                print(f"STALE: {j['id']} with {j['owner']} for {int(age)}h — still theirs?")
                bad += 1
            if j.get("branch"):
                has = subprocess.run(["git", "rev-parse", "--verify", "-q", j["branch"]],
                                     cwd=ROOT, capture_output=True)
                if has.returncode != 0:
                    print(f"NOTE: {j['id']} claims branch {j['branch']} that does not exist yet")
    print(f"{bad} problem(s); {sum(1 for j in jobs.values() if j['status'] in OPEN)} open")
    return 1 if bad else 0


def cmd_merge(a, jobs):
    """Union another copy of the ledger in (used when two machines each hold one)."""
    import hashlib
    def key(ev):
        return hashlib.sha1(json.dumps([ev.get("id"), ev.get("ev"), ev.get("ts"),
                                        ev.get("who"), ev.get("title")]).encode()).hexdigest()
    have = set()
    with open(LEDGER, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line:
                try:
                    have.add(key(json.loads(line)))
                except json.JSONDecodeError:
                    pass
    added = 0
    for path in a["args"]:
        with open(path, encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                try:
                    ev = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if key(ev) in have:
                    continue
                have.add(key(ev))
                append(ev)
                added += 1
                print(f"+ {ev['id']} {ev.get('ev')} {ev.get('who', '')}")
    print(f"merged {added} new event(s)")
    return 0


def parse(argv):
    a, args, i = {"who": None}, [], 0
    while i < len(argv):
        if argv[i].startswith("--"):
            k = argv[i][2:]
            if k == "open":
                a["open"] = True
                i += 1
                continue
            if i + 1 >= len(argv):
                sys.exit(f"--{k} needs a value")
            a[k] = argv[i + 1]
            if k == "who":
                a["who_set"] = True
            i += 2
        else:
            args.append(argv[i])
            i += 1
    a["args"] = args
    return a


def selftest():
    global LEDGER
    import tempfile
    keep, LEDGER = LEDGER, os.path.join(tempfile.mkdtemp(), "ledger.jsonl")
    try:
        cmd_add({"args": ["Fix", "uninstall", "license"], "who": "qc"}, load())
        jobs = load()
        jid = next(iter(jobs))
        try:
            cmd_add({"args": ["fix uninstall license"], "who": "ag"}, jobs)
            raise AssertionError("duplicate add allowed")
        except SystemExit as e:
            assert "already" in str(e)
        cmd_claim({"args": [jid], "who": "qc", "branch": "qc/x"}, load())
        import contextlib
        import io
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            cmd_list(parse([]), load())
        assert jid in buf.getvalue(), "list hid a job because who defaulted"
        try:
            cmd_claim({"args": [jid], "who": "ag"}, load())
            raise AssertionError("overlap allowed")
        except SystemExit as e:
            assert "OVERLAP" in str(e)
        cmd_done({"args": [jid], "who": "qc", "commit": "abc"}, load())
        jobs = load()
        assert jobs[jid]["status"] == "done", jobs[jid]
        assert jobs[jid]["commit"] == "abc"
    finally:
        LEDGER = keep
    print("selftest ok")


def main():
    argv = sys.argv[1:]
    if not argv:
        sys.exit(__doc__)
    cmd, a = argv[0], parse(argv[1:])
    if cmd == "selftest":
        return selftest()
    jobs = load()
    fn = {"add": cmd_add, "claim": cmd_claim, "done": cmd_done, "block": cmd_block,
          "list": cmd_list, "check": cmd_check, "merge": cmd_merge}.get(cmd)
    if not fn:
        sys.exit(__doc__)
    sys.exit(fn(a, jobs) or 0)


if __name__ == "__main__":
    main()
