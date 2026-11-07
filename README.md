# pawn-json

JSON for open.mp scripts. It's an open.mp component: a small C++ shell on the open.mp
SDK with a Rust core behind it, built into a single library.

It started as a rewrite of pawn-json 1.x, whose reads cloned the whole document every
time - a few thousand keys could take seconds to load. This version reads values where
they are, so the same file loads in a few milliseconds.

- Reading a child gives you a view, not a copy: write through it and the parent changes.
  Reading never frees anything, and a handle to a freed node returns `JSON_ERR_NODE`
  instead of quietly pointing at something else.
- Nodes clean up after themselves. A node you pass to a helper stays alive after the
  helper returns; one the helper creates is freed with it.
- Strings follow the script's code page, so accented text round-trips. A character the
  code page can't hold comes out as `?`, never as `&#225;`.
- One call reads a whole row: `JSON_Unpack(row, "{model:i, x:f, name:s[32]}", model, x, name)`.
- One include, `json.inc`, and it still has every pawn-json 1.x function, so old scripts
  compile as they are.

## Install

With sampctl, add the package to your `pawn.json` dependencies; it puts the library in
`components/` and the include where the compiler finds it. By hand: copy `pawn-json.dll`
(or `pawn-json.so` on Linux) into the server's `components/` folder and `json.inc` onto
your include path. It needs open.mp; there's no `legacy_plugins` entry to add.

The default code page comes from `config.json`, and is `windows-1252` if you don't set it:

```json
{
    "json": { "codepage": "windows-1250" }
}
```

A script can pick its own with `JSON_SetCodepage("windows-1250")`.

## Use

```pawn
#include <json>

LoadVehicles()
{
    new Node:root;
    if (JSON_ParseFile("scriptfiles/vehicles.json", root) != JSON_OK)
    {
        return;
    }
    new Node:list;
    JSON_GetNode(root, "vehicles", list);

    new Node:row;
    while (JSON_Iter(list, row))
    {
        new model, Float:x, Float:y, Float:z, name[32];
        if (JSON_Unpack(row, "{model:i, x:f, y:f, z:f, name:s[32]?}", model, x, y, z, name) == JSON_OK)
        {
            // ...
        }
    }
}
```

`root` and `list` are freed when `LoadVehicles` returns, and the loop frees `row` itself
once it runs out. To hand a node back to your caller from a `Node:` variable, use
`return JSON_Up(node);`. To keep one past every scope - in a global, or for a timer or an
inline function that runs later - call `JSON_Keep(node)`, and `JSON_Release(node)` when
you're done with it.

Everything returns `JSON_OK` or a `JSON_ERR_*` code, and functions that create a node
return it (or 0). `json.inc` documents every function: reads by key, by path
(`"/vehicles/3/model"`) and by index, writes, files (saving goes through a temporary
file, so a crash can't leave a half-written one), loops, `JSON_Get`, which picks the
right read from your variable's tag, and `JSON_Pack` / `JSON_Unpack`. The pawn-json 1.x
functions - `JSON_GetObject`, `JSON_ArrayIterate`, `JSON_Object("key", JSON_Int(1))` and
the rest - are at the end of the file. They return the same codes as everything else;
0 still means success.

## Build

```
cargo build --release --target i686-pc-windows-msvc -p pawn-json      # pawn_json.dll
cargo build --release --target i686-unknown-linux-gnu -p pawn-json    # libpawn_json.so
```

The open.mp SDK is a git submodule, so clone with `--recursive` or run
`git submodule update --init --recursive`. The Linux build needs `gcc-multilib` and
`g++-multilib`. On Linux the library uses the 32-bit `libstdc++`, which an open.mp server
already has for its own components.

## Test

- `cargo test --workspace` tests the core and the AMX layer; no server needed.
- `python tools/server-test.py` runs the Pawn tests in `tests/pawn/` on a throwaway
  open.mp server on 127.0.0.1:7797. The first time, point it at an open.mp install and a
  pawncc to copy from - see the top of the script.
- `bench/` compares read speed against a model of pawn-json 1.x over any folder of
  `.json` and `.jsonl` files: `jsonbench.exe <folder>`.

## Licence

Mozilla Public License 2.0, see `LICENSE`. The original code is copyright (c) rasheed.
The AMX headers in `lib/amx/` belong to ITB CompuPhase and come under their zlib licence.
