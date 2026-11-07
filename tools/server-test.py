# This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
# copy of the MPL was not distributed with this file, You can obtain one at
# http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

"""Runs the Pawn tests in tests/pawn/ on a throwaway open.mp server.

The first run needs to know where to find an open.mp server and a pawncc to copy from:

    python tools/server-test.py --server C:/open.mp --pawncc C:/pawncc

(or set OMP_SERVER and PAWNCC). It copies omp-server.exe with the Pawn and Console
components into test-server/, and pawncc into tools/pawncc/; later runs reuse them.

Each tests/pawn/<name>.pwn becomes the gamemode of its own short server run on
127.0.0.1:7797. The script runs its checks in OnGameModeInit, prints "PASS n FAIL m" and
asks the server to exit; we kill it if it hangs. Scripts in tests/pawn/fs/ are built as
filterscripts the tests can load, and each script in tests/pawn/fail/ has to fail to
build with the messages its `// expect:` lines name.

    python tools/server-test.py [name ...] [--no-build] [--server DIR] [--pawncc DIR]
"""

import json
import os
import re
import shutil
import socket
import subprocess
import sys
import threading
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SERVER = os.path.join(ROOT, "test-server")
PAWNCC = os.path.join(ROOT, "tools", "pawncc")
TESTS = os.path.join(ROOT, "tests", "pawn")
FAILS = os.path.join(TESTS, "fail")
LIBRARY = os.path.join(ROOT, "target", "i686-pc-windows-msvc", "release", "pawn_json.dll")
PORT = 7797
TIMEOUT_S = 60

SERVER_FILES = ["omp-server.exe", "components/Pawn.dll", "components/Console.dll"]
PAWNCC_FILES = ["pawncc.exe", "pawnc.dll"]

# Files a test writes that it can't check from Pawn: we look at the bytes afterwards.
FILE_CHECKS = {
    "codepage": ("scriptfiles/codepage.json", ('{"n":"' + chr(0x151) + chr(0xE1) + 'x"}').encode("utf-8")),
}


def option(name, env):
    if name in sys.argv:
        i = sys.argv.index(name)
        if i + 1 < len(sys.argv):
            return sys.argv[i + 1]
    return os.environ.get(env)


def copy_missing(names, src_dir, dst_dir, what):
    missing = [n for n in names if not os.path.exists(os.path.join(dst_dir, n))]
    if not missing:
        return True
    if not src_dir:
        print(f"{what} is not set up yet: pass its folder to copy from, see the top of this file")
        return False
    for name in missing:
        dst = os.path.join(dst_dir, name)
        os.makedirs(os.path.dirname(dst), exist_ok=True)
        shutil.copy2(os.path.join(src_dir, name), dst)
    return True


def port_free():
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as s:
        try:
            s.bind(("127.0.0.1", PORT))
            return True
        except OSError:
            return False


def compile_script(src, folder, name):
    os.makedirs(os.path.join(SERVER, folder), exist_ok=True)
    out = os.path.join(SERVER, folder, name + ".amx")
    r = subprocess.run(
        [os.path.join(PAWNCC, "pawncc.exe"), src,
         "-i" + os.path.join(ROOT, "include"), "-i" + TESTS, "-o" + out],
        capture_output=True, text=True)
    text = (r.stdout + r.stderr).strip()
    lines = [l for l in text.splitlines() if not l.startswith("Pawn compiler") and l.strip()]
    # A warning counts as a failure too: the include should build clean.
    if r.returncode != 0 or any("error" in l or "warning" in l for l in lines):
        print("\n".join(lines))
        return False
    return True


def compile_test(name):
    fs = os.path.join(TESTS, "fs")
    for f in sorted(os.listdir(fs)) if os.path.isdir(fs) else []:
        if f.endswith(".pwn") and not compile_script(os.path.join(fs, f), "filterscripts", f[:-4]):
            return False
    return compile_script(os.path.join(TESTS, name + ".pwn"), "gamemodes", name)


def write_config(name):
    config = {
        "announce": False,
        "enable_query": False,
        "name": "pawn-json test",
        "max_players": 2,
        "network": {"port": PORT, "bind": "127.0.0.1"},
        "pawn": {"main_scripts": [name + " 1"], "side_scripts": [], "legacy_plugins": []},
        "rcon": {"enable": False, "password": ""},
        "logging": {"enable": True, "file": "log.txt"},
        "json": {"codepage": "windows-1250"},
    }
    with open(os.path.join(SERVER, "config.json"), "w") as f:
        json.dump(config, f, indent=2)


def run_server():
    """Returns the server's output, the PASS/FAIL counts (None if it never printed them)
    and whether we had to kill it."""
    proc = subprocess.Popen([os.path.join(SERVER, "omp-server.exe")], cwd=SERVER,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                            stdin=subprocess.DEVNULL, text=True, errors="replace")
    lines, result = [], [None]

    def read():
        for line in proc.stdout:
            line = line.rstrip()
            lines.append(line)
            m = re.search(r"PASS (\d+) FAIL (\d+)", line)
            if m:
                result[0] = (int(m.group(1)), int(m.group(2)))

    reader = threading.Thread(target=read, daemon=True)
    reader.start()
    deadline = time.time() + TIMEOUT_S
    while proc.poll() is None and result[0] is None and time.time() < deadline:
        time.sleep(0.1)
    if result[0] is not None:
        # The script asked the server to exit; give it a moment to do so.
        try:
            proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            pass
    killed = proc.poll() is None
    if killed:
        proc.kill()
        proc.wait()
    reader.join(timeout=5)
    return lines, result[0], killed


def compile_fails_as_expected(src):
    want = [l[len("// expect:"):].strip() for l in open(src, encoding="utf-8") if l.startswith("// expect:")]
    out = os.path.join(SERVER, "fail.amx")
    r = subprocess.run([os.path.join(PAWNCC, "pawncc.exe"), src, "-i" + os.path.join(ROOT, "include"),
                        "-o" + out], capture_output=True, text=True)
    text = r.stdout + r.stderr
    missing = [w for w in want if w not in text]
    name = os.path.basename(src)
    print(f"fail/{name}: {'compiler says ' + ', '.join(want) if not missing else 'missing ' + ', '.join(missing)}")
    return bool(want) and not missing


def main():
    skip = {"--server", "--pawncc"}
    args = [a for i, a in enumerate(sys.argv[1:], 1) if not a.startswith("--") and sys.argv[i - 1] not in skip]
    names = args or sorted(f[:-4] for f in os.listdir(TESTS) if f.endswith(".pwn"))
    if not copy_missing(SERVER_FILES, option("--server", "OMP_SERVER"), SERVER, "The test server"):
        return 1
    if not copy_missing(PAWNCC_FILES, option("--pawncc", "PAWNCC"), PAWNCC, "pawncc"):
        return 1
    if "--no-build" not in sys.argv:
        r = subprocess.run(["cargo", "build", "--release", "--target", "i686-pc-windows-msvc",
                            "-p", "pawn-json"], cwd=ROOT)
        if r.returncode != 0:
            return 1
    shutil.copy2(LIBRARY, os.path.join(SERVER, "components", "pawn-json.dll"))

    failed = 0
    for name in names:
        if not port_free():
            print(f"{name}: port {PORT} is taken - is another test server still running?")
            return 1
        if not compile_test(name):
            print(f"{name}: does not compile")
            failed += 1
            continue
        write_config(name)
        os.makedirs(os.path.join(SERVER, "scriptfiles"), exist_ok=True)
        lines, result, killed = run_server()
        loaded = [l for l in lines if "pawn-json" in l and "loaded" in l.lower()]
        for l in lines:
            if "FAIL line" in l or "[pawn-json]" in l or "rror" in l:
                print("  " + l)
        status = "no PASS line" if result is None else f"PASS {result[0]} FAIL {result[1]}"
        print(f"{name}: {status}{', killed' if killed else ''}; {loaded[0].strip() if loaded else 'component not loaded'}")
        check = FILE_CHECKS.get(name)
        if check:
            path = os.path.join(SERVER, check[0])
            held = open(path, "rb").read() if os.path.exists(path) else b""
            if held != check[1]:
                print(f"  {check[0]} holds {held!r}, want {check[1]!r}")
                result = None
        if result is None or result[1] != 0 or killed or not loaded:
            failed += 1
            if result is None:
                print("\n".join("  | " + l for l in lines[-30:]))
    fails = [] if args else sorted(f for f in os.listdir(FAILS) if f.endswith(".pwn"))
    for f in fails:
        if not compile_fails_as_expected(os.path.join(FAILS, f)):
            failed += 1
    total = len(names) + len(fails)
    print(f"{total - failed}/{total} scripts passed")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
