# This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
# copy of the MPL was not distributed with this file, You can obtain one at
# http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

"""Times speed.pwn on pawn-json 1.4.1 and on this plugin, on a real open.mp server.

    python bench/server/compare.py --server <open.mp folder> --pawncc <pawncc folder> [--boots 5]

It builds two throwaway servers under bench/server/work/: one with 1.4.1's json library
as a legacy plugin, downloaded from its GitHub release, and one with this plugin's release
build from target/. speed.pwn is compiled once against each plugin's json.inc, and the
servers are booted in turn - old, new, old, new - so that anything else the machine does
falls on both alike. Each boot runs every workload until a second has passed. The table
gives, for each workload, the median of the boots' per-run times and their range, and
stops if the two plugins' checks differ.

The pawncc folder holds pawncc with its library, as for tools/server-test.py.
"""

import hashlib
import os
import platform
import re
import shutil
import statistics
import subprocess
import sys
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
WORK = os.path.join(HERE, "work")
OLD_RELEASE = "https://github.com/Southclaws/pawn-json/releases/download/1.4.1/"
OLD_INCLUDE = "https://raw.githubusercontent.com/Southclaws/pawn-json/1.4.1/json.inc"
PORT = 7798

if os.name == "nt":
    TARGET, BUILT, COMPONENT, LEGACY = "i686-pc-windows-msvc", "pawn_json.dll", "pawn-json.dll", "json.dll"
    SERVER_EXE, EXT, PAWNCC_EXE = "omp-server.exe", ".dll", "pawncc.exe"
else:
    TARGET, BUILT, COMPONENT, LEGACY = "i686-unknown-linux-gnu", "libpawn_json.so", "pawn-json.so", "json.so"
    SERVER_EXE, EXT, PAWNCC_EXE = "omp-server", ".so", "pawncc"


def option(name, default=None):
    if name in sys.argv:
        return sys.argv[sys.argv.index(name) + 1]
    return default


def sha256(path):
    return hashlib.sha256(open(path, "rb").read()).hexdigest()


def download(url, path):
    if not os.path.exists(path):
        os.makedirs(os.path.dirname(path), exist_ok=True)
        urllib.request.urlretrieve(url, path)
    return path


def prepare(name, server, plugin):
    """A server folder with the Pawn and Console components and one json library."""
    folder = os.path.join(WORK, name)
    for rel in [SERVER_EXE, "components/Pawn" + EXT, "components/Console" + EXT]:
        dst = os.path.join(folder, rel)
        os.makedirs(os.path.dirname(dst), exist_ok=True)
        shutil.copy2(os.path.join(server, rel), dst)
    legacy = name == "old"
    dst = os.path.join(folder, "plugins" if legacy else "components", LEGACY if legacy else COMPONENT)
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    shutil.copy2(plugin, dst)
    with open(os.path.join(folder, "config.json"), "w") as f:
        f.write('{"announce": false, "enable_query": false, "max_players": 2,'
                ' "network": {"port": %d, "bind": "127.0.0.1"},'
                ' "pawn": {"main_scripts": ["speed 1"], "side_scripts": [],'
                ' "legacy_plugins": [%s]},'
                ' "rcon": {"enable": false, "password": ""},'
                ' "logging": {"enable": false}}' % (PORT, '"json"' if legacy else ""))
    subprocess.run([sys.executable, os.path.join(HERE, "make-data.py"), os.path.join(folder, "scriptfiles")],
                   check=True)
    return folder


def build(folder, include, pawncc):
    env = dict(os.environ)
    if os.name != "nt":
        env["LD_LIBRARY_PATH"] = pawncc
    os.makedirs(os.path.join(folder, "gamemodes"), exist_ok=True)
    r = subprocess.run([os.path.join(pawncc, PAWNCC_EXE), os.path.join(HERE, "speed.pwn"), "-i" + include,
                        "-o" + os.path.join(folder, "gamemodes", "speed.amx")],
                       capture_output=True, text=True, env=env)
    if r.returncode != 0:
        sys.exit(r.stdout + r.stderr)
    return r.stdout.splitlines()[0] if r.stdout else ""


def boot(folder):
    r = subprocess.run([os.path.join(folder, SERVER_EXE)], cwd=folder, capture_output=True, text=True,
                       errors="replace", stdin=subprocess.DEVNULL, timeout=900)
    found = {}
    for line in r.stdout.splitlines():
        m = re.search(r"RESULT (\S+) runs=(\d+) ms=(\d+) check=(-?\d+)", line)
        if m:
            found[m[1]] = (int(m[3]) / int(m[2]), int(m[4]), int(m[2]))
    if not found:
        sys.exit(f"no results from {folder}:\n" + r.stdout[-2000:])
    return found, r.stdout.splitlines()[0]


def show(ms):
    if ms >= 100:
        return f"{ms:,.0f}".replace(",", " ")
    if ms >= 10:
        return f"{ms:.1f}"
    if ms >= 1:
        return f"{ms:.2f}"
    return f"{ms:.3f}"


def main():
    server, pawncc = option("--server"), option("--pawncc")
    if not server or not pawncc:
        sys.exit(__doc__)
    boots = int(option("--boots", "5"))
    library = os.path.join(ROOT, "target", TARGET, "release", BUILT)
    old_lib = download(OLD_RELEASE + LEGACY, os.path.join(WORK, "1.4.1", LEGACY))
    old_inc = download(OLD_INCLUDE, os.path.join(WORK, "1.4.1", "json.inc"))

    old = prepare("old", server, old_lib)
    new = prepare("new", server, library)
    compiler = build(old, os.path.dirname(old_inc), pawncc)
    build(new, os.path.join(ROOT, "include"), pawncc)

    times = {"old": {}, "new": {}}
    checks = {"old": {}, "new": {}}
    runs = {"old": {}, "new": {}}
    for i in range(boots):
        for name, folder in (("old", old), ("new", new)):
            found, banner = boot(folder)
            print(f"boot {i + 1} {name}: " + ", ".join(f"{k} {show(v[0])} ms" for k, v in found.items()),
                  flush=True)
            for k, (ms, check, n) in found.items():
                times[name].setdefault(k, []).append(ms)
                checks[name].setdefault(k, set()).add(check)
                runs[name].setdefault(k, []).append(n)

    for k in times["new"]:
        if checks["old"][k] != checks["new"][k] or len(checks["new"][k]) != 1:
            sys.exit(f"{k}: the checks differ, old {checks['old'][k]}, new {checks['new'][k]}")

    print()
    print(f"{platform.platform()}, {platform.processor() or platform.machine()}")
    print(f"{banner}; {compiler}")
    print(f"pawn-json 1.4.1 {LEGACY} sha256 {sha256(old_lib)}")
    print(f"this plugin {BUILT} sha256 {sha256(library)}")
    print(f"{boots} boots each, alternating; per run: median of the boots (min - max)")
    print()
    print("| Workload | pawn-json 1.4.1, ms | this plugin, ms | runs a boot, 1.4.1 / this | speedup |")
    print("| --- | ---: | ---: | ---: | ---: |")
    for k in times["new"]:
        o, n = times["old"][k], times["new"][k]
        mo, mn = statistics.median(o), statistics.median(n)
        print(f"| {k} | {show(mo)} ({show(min(o))} - {show(max(o))}) | {show(mn)} ({show(min(n))} - {show(max(n))})"
              f" | {min(runs['old'][k])} / {min(runs['new'][k])} | {mo / mn:,.0f}x |".replace(",", " "))


if __name__ == "__main__":
    main()
