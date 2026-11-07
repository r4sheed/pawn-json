# This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
# copy of the MPL was not distributed with this file, You can obtain one at
# http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

"""Checks that a built library exports exactly one symbol, ComponentEntryPoint.

    python tools/exports.py <pawn_json.dll | libpawn_json.so>

For a Windows DLL we read the export table ourselves; for a Linux .so we ask `nm -D`.
"""

import struct
import subprocess
import sys

WANT = ["ComponentEntryPoint"]
LINKER_SYMBOLS = {"_edata", "_end", "__bss_start", "_init", "_fini"}


def pe_exports(data):
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    sections = struct.unpack_from("<H", data, pe + 6)[0]
    optional_size = struct.unpack_from("<H", data, pe + 20)[0]
    optional = pe + 24
    magic = struct.unpack_from("<H", data, optional)[0]
    directories = optional + (96 if magic == 0x10B else 112)
    export_rva = struct.unpack_from("<I", data, directories)[0]
    if export_rva == 0:
        return []
    table = optional + optional_size

    def offset(rva):
        for i in range(sections):
            s = table + 40 * i
            vsize, vaddr, _, raw = struct.unpack_from("<IIII", data, s + 8)
            if vaddr <= rva < vaddr + max(vsize, 1):
                return rva - vaddr + raw
        raise ValueError(f"rva {rva:#x} in no section")

    e = offset(export_rva)
    count, _, names_rva = struct.unpack_from("<III", data, e + 24)
    names = []
    for i in range(count):
        name_rva = struct.unpack_from("<I", data, offset(names_rva) + 4 * i)[0]
        start = offset(name_rva)
        names.append(data[start:data.index(b"\0", start)].decode())
    return names


def elf_exports(path):
    out = subprocess.run(["nm", "-D", "--defined-only", path], capture_output=True, text=True, check=True).stdout
    return [l.split()[-1] for l in out.splitlines() if l.split() and l.split()[-1] not in LINKER_SYMBOLS]


def main():
    path = sys.argv[1]
    with open(path, "rb") as f:
        data = f.read()
    names = elf_exports(path) if data[:4] == b"\x7fELF" else pe_exports(data)
    print(f"{path}: {len(data)} bytes, exports {names}")
    return 0 if sorted(names) == WANT else 1


if __name__ == "__main__":
    sys.exit(main())
