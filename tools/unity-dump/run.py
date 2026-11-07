#!/usr/bin/env python3
"""Run Typesetter's text cases through Unity's TextMesh Pro and write ground truth.

Stages tests/fonts/*.ttf into Assets/Fonts and bench/cases/*.json into
Assets/Cases, then runs Unity in batch mode twice: TypesetterDump.Setup (imports
TMP's essential resources into this harness project on first use) and
TypesetterDump.Run (writes <out>/<case>.json, optional <case>.png renders, and
<out>/atlases/<asset>.json + <asset>_<n>.png). See Assets/Editor/TypesetterDump.cs.

    python tools/unity-dump/run.py [--unity PATH] [--out bench/unity] [--only a,b] [--render]

Unity is black-box here: only its public API is called and only its output is
read. Never kill Unity by image name (other agents run it); this script only
ever waits on the process it started.
"""
import argparse
import os
import shutil
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
DEFAULT_UNITY = r"C:/Program Files/Unity/Hub/Editor/6000.3.24f1/Editor/Unity.exe"


def unity(exe, method, extra, log, render, timeout, quit=True, attempts=5):
    cmd = [exe, "-batchmode", "-projectPath", HERE, "-executeMethod", method, "-logFile", log]
    if not render:
        cmd.append("-nographics")
    cmd += extra + (["-quit"] if quit else [])
    print(" ".join(cmd))
    for attempt in range(attempts):
        p = subprocess.Popen(cmd)
        try:
            rc = p.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            print("Unity timed out; terminating PID %d" % p.pid)
            p.kill()
            return -1
        # On a loaded machine the editor sometimes gives up waiting for its
        # package-manager server before running anything; just retry.
        text = open(log, encoding="utf-8", errors="replace").read() if os.path.exists(log) else ""
        if rc != 0 and "Unity Package Manager local server" in text and attempt + 1 < attempts:
            print("package manager did not start; retrying (%d)" % (attempt + 2))
            continue
        return rc
    return rc


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--unity", default=DEFAULT_UNITY)
    ap.add_argument("--cases", default=os.path.join(REPO, "bench", "cases"))
    ap.add_argument("--out", default=os.path.join(REPO, "bench", "unity"))
    ap.add_argument("--only", default="")
    ap.add_argument("--render", action="store_true")
    ap.add_argument("--probe", action="store_true", help="also write _api.json (public TMP API listing)")
    ap.add_argument("--timeout", type=int, default=1800)
    a = ap.parse_args()

    fonts = os.path.join(HERE, "Assets", "Fonts")
    os.makedirs(fonts, exist_ok=True)
    for fn in os.listdir(os.path.join(REPO, "tests", "fonts")):
        if fn.lower().endswith((".ttf", ".otf")):
            shutil.copyfile(os.path.join(REPO, "tests", "fonts", fn), os.path.join(fonts, fn))
    stage = os.path.join(HERE, "Assets", "Cases")
    if os.path.isdir(stage):
        shutil.rmtree(stage)
    os.makedirs(stage)
    n = 0
    for fn in sorted(os.listdir(a.cases)):
        if fn.endswith(".json"):
            shutil.copyfile(os.path.join(a.cases, fn), os.path.join(stage, fn))
            n += 1
    print("staged %d cases" % n)

    logs = os.path.join(HERE, "Logs")
    os.makedirs(logs, exist_ok=True)
    rc = unity(a.unity, "TypesetterDump.Setup", [], os.path.join(logs, "setup.log"), False, a.timeout, quit=False)
    if rc != 0:
        print("setup failed (%d); see %s" % (rc, os.path.join(logs, "setup.log")))
        return 1
    out = os.path.abspath(a.out)
    extra = ["-dumpCases", stage, "-dumpOut", out]
    if a.only:
        extra += ["-only", a.only]
    if a.render:
        extra.append("-render")
    if a.probe:
        extra.append("-probe")
    rc = unity(a.unity, "TypesetterDump.Run", extra, os.path.join(logs, "run.log"), a.render, a.timeout)
    log = open(os.path.join(logs, "run.log"), encoding="utf-8", errors="replace").read()
    for line in log.splitlines():
        if "[typesetter-dump]" in line or "error CS" in line:
            print(line)
    return 0 if rc == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
