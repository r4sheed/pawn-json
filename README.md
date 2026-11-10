# pawn-json

[![sampctl](https://img.shields.io/badge/sampctl-pawn--json-2f2f2f.svg?style=for-the-badge)](https://github.com/r4sheed/pawn-json)

> [!WARNING]
> **AI-generated code.** This project was designed by r4sheed in collaboration with
> Claude Fable 5.1. The implementation was written by Claude Opus 5.5, then audited,
> corrected, optimized and tested by Claude Fable 5.2. Review the source before deploying
> it to a production server.

JSON for open.mp scripts: parse files, read values, build documents and save them. It is
a drop-in successor to [Southclaws/pawn-json](https://github.com/Southclaws/pawn-json):
every 1.x function is still there under its old name, so existing scripts compile as
they are, but loading a few thousand keys takes milliseconds instead of seconds, and a
long list of 1.x's quiet mistakes are fixed. See [Compared to pawn-json 1.x](#compared-to-pawn-json-1x)
for the measurements.

- Reads are lookups, not copies. A 4000-key file loads in 1.5 ms, where 1.x takes 5 s.
- Children are views: write through one and the parent changes.
- Nodes free themselves when the function that made them returns, and a node you pass to
  a helper survives it.
- One call reads a whole row: `JSON_Unpack(row, "{model:i, pos:[f, f, f], plate:s[16]}", ...)`.
- Strings follow the server's code page, so `Café Señor` arrives as `Café Señor`, not as
  `Caf&#233; Se&#241;or`.
- Saves go through a temporary file, so a crash never leaves half a file behind.
- Every call returns `JSON_OK` or a `JSON_ERR_*` code that says what went wrong.

It is an open.mp component (a small C++ shell on the open.mp SDK around a Rust core), so it
needs an open.mp server, and there is no `legacy_plugins` entry to add.

## Installation

Simply install to your project:

```bash
sampctl package install r4sheed/pawn-json
```

Include in your code and begin using the library:

```pawn
#include <json>
```

## Usage

Every function is documented in [json.inc](include/json.inc). The examples below are the
everyday jobs: each one was compiled against open.mp's includes and run on a server.

### Loading a list at startup

`scriptfiles/vehicles.json`:

```json
{
    "vehicles": [
        { "model": 411, "pos": [1958.33, 1343.12, 15.36], "angle": 90.0, "colors": [1, 1], "plate": "SA 0001" },
        { "model": 560, "pos": [2040.5, 1330.0, 10.5], "angle": 180.0, "colors": [3, 0] }
    ]
}
```

```pawn
LoadVehicles()
{
    new Node:root;
    if (JSON_ParseFile("scriptfiles/vehicles.json", root) != JSON_OK)
    {
        print("vehicles.json is missing or broken, the server log says why");
        return 0;
    }

    new Node:list, Node:row, count;
    JSON_GetNode(root, "vehicles", list);
    while (JSON_Iter(list, row))
    {
        new model, Float:x, Float:y, Float:z, Float:angle, color1, color2, plate[16];
        if (JSON_Unpack(row, "{model:i, pos:[f, f, f], angle:f, colors:[i, i], plate:s[16]?}",
                model, x, y, z, angle, color1, color2, plate) != JSON_OK)
        {
            continue; // a broken row is skipped, the rest still load
        }
        new vehicleid = CreateVehicle(model, x, y, z, angle, color1, color2, -1);
        if (plate[0])
        {
            SetVehicleNumberPlate(vehicleid, plate);
        }
        count++;
    }
    return count;
}
```

`JSON_Unpack` reads a whole row in one call. The format mirrors the JSON: `i` int, `f`
Float, `b` bool, `s[size]` string, `o` node, `[...]` and `{...}` nest, and a `?` marks
something that may be missing. `root`, `list` and `row` are freed when `LoadVehicles`
returns; there is nothing to clean up by hand.

### Saving what admins build in game

Teleport points an admin adds with a command, kept in memory and saved on every change.

```pawn
#define TELEPORTS "scriptfiles/teleports.json"

new Node:gTeleports;

LoadTeleports()
{
    new result = JSON_ParseFile(TELEPORTS, gTeleports);
    if (result == JSON_ERR_FILE)
    {
        gTeleports = JSON_Array(); // no teleports yet
    }
    else if (result != JSON_OK)
    {
        print("teleports.json is broken, so it won't be overwritten; the server log says why");
        return;
    }
    JSON_Keep(gTeleports); // a global outlives every function, so keep it
}

AddTeleport(playerid, const name[])
{
    new Float:x, Float:y, Float:z, Float:angle;
    GetPlayerPos(playerid, x, y, z);
    GetPlayerFacingAngle(playerid, angle);
    new interior = GetPlayerInterior(playerid), world = GetPlayerVirtualWorld(playerid);

    JSON_AppendNode(gTeleports, JSON_Pack("{name:s, pos:[f, f, f], angle:f, interior:i, world:i}",
        name, x, y, z, angle, interior, world));
    return JSON_SaveFile(TELEPORTS, gTeleports, true);
}

bool:Teleport(playerid, const name[])
{
    new Node:teleport;
    while (JSON_Iter(gTeleports, teleport))
    {
        new found[32], Float:x, Float:y, Float:z, Float:angle, interior, world;
        if (JSON_Unpack(teleport, "{name:s[32], pos:[f, f, f], angle:f, interior:i, world:i}",
                found, x, y, z, angle, interior, world) != JSON_OK
            || strcmp(found, name, true) != 0)
        {
            continue;
        }
        SetPlayerInterior(playerid, interior);
        SetPlayerVirtualWorld(playerid, world);
        SetPlayerPos(playerid, x, y, z);
        SetPlayerFacingAngle(playerid, angle);
        return true;
    }
    return false;
}
```

`JSON_Pack` is `JSON_Unpack` the other way round, and `JSON_AppendNode` uses up the fresh
node it is given, so nothing leaks. `JSON_SaveFile` writes a temporary file and swaps it
in, so a crash mid-save leaves the old file whole; the wait for the disk happens on a
background thread, so a save doesn't stall the server.

### Texts kept in memory

A document you keep open costs well under a microsecond per read, so there is no need to
copy a language file into Pawn arrays.

```pawn
new Node:gTexts;

LoadTexts()
{
    if (JSON_ParseFile("scriptfiles/lang/en.json", gTexts) == JSON_OK)
    {
        JSON_Keep(gTexts); // a global outlives every function, so keep it
    }
}

SendText(playerid, const key[])
{
    new text[144];
    if (JSON_GetString(gTexts, key, text) != JSON_OK)
    {
        format(text, sizeof (text), "(missing text: %s)", key);
    }
    SendClientMessage(playerid, -1, text);
}
```

### Settings with defaults

A read that fails leaves the variable as it was, so the defaults are just the initial
values. A path reads deep into the tree in one call.

```pawn
new gMaxPing = 500, gSpawnMoney = 1000, gWelcome[128] = "Welcome!";

LoadConfig()
{
    new Node:config;
    if (JSON_ParseFile("scriptfiles/server.json", config) != JSON_OK)
    {
        return;
    }
    JSON_PathGetInt(config, "/limits/max_ping", gMaxPing);
    JSON_PathGetInt(config, "/spawn/money", gSpawnMoney);
    JSON_PathGetString(config, "/messages/welcome", gWelcome);
}
```

### Walking an object

Keys come in the order they are in the file.

```pawn
// scriptfiles/shop.json: { "prices": { "Desert Eagle": 1500, "M4": 4500, "Sniper Rifle": 8000 } }
LoadShop()
{
    new Node:shop;
    if (JSON_ParseFile("scriptfiles/shop.json", shop) != JSON_OK)
    {
        return;
    }
    new Node:prices, Node:price, weapon[32], cost;
    JSON_GetNode(shop, "prices", prices);
    while (JSON_IterEntry(prices, weapon, price))
    {
        if (JSON_ValueInt(price, cost) == JSON_OK)
        {
            printf("%s costs $%d", weapon, cost);
        }
    }
}
```

`JSON_IterKey(object, key)` walks just the keys, and `JSON_Iter(container, element)` the
values of an array or an object.

### One getter for every type

`JSON_Get` picks the read from the variable's tag at compile time, so it is still one
native call:

```pawn
new Float:health, bool:locked, Node:pos, owner[MAX_PLAYER_NAME], model;
JSON_Get(row, "health", health);
JSON_Get(row, "locked", locked);
JSON_Get(row, "pos", pos);      // a view into the row
JSON_Get(row, "owner", owner);
JSON_Get(row, "model", model);
```

### Lifetime

A `Node:` variable frees its node when it goes out of scope, if the node was made in that
function or in one it called. A node you pass down to a helper (or to a hook through
`CallLocalFunction`) is left alone. To hand a node in a variable back to the caller,
`return JSON_Up(node);`. To keep one past every scope - in a global, or for a timer that
runs later - call `JSON_Keep(node)`, and `JSON_Release(node)` when you're done.
`JSON_CountNodes()` says how many handles are alive, which is handy for spotting leaks.

### Code page

Strings are converted between the script's code page and the file's UTF-8. The default
comes from the server's `config.json`, and is `windows-1252` if unset:

```json
{
    "json": { "codepage": "windows-1250" }
}
```

A script can pick its own with `JSON_SetCodepage("windows-1250")`. A character the code
page can't hold comes out as `?`.

### Errors

Everything returns `JSON_OK` (0) or one of these, and the server log names the call and
the reason:

| Code | Meaning |
| --- | --- |
| `JSON_ERR_NODE` | the node was never made, or has been freed |
| `JSON_ERR_CONTAINER` | not an object or array where one was needed |
| `JSON_ERR_KEY` | no such key, or an index past the end |
| `JSON_ERR_TYPE` | the value is another type |
| `JSON_ERR_BUFFER` | the string didn't fit: you got as much as fits |
| `JSON_ERR_RANGE` | a whole number doesn't fit in 32 bits |
| `JSON_ERR_SYNTAX` | the text isn't valid JSON |
| `JSON_ERR_FILE` | the file can't be read or written |
| `JSON_ERR_PATH` | a malformed path |
| `JSON_ERR_FORMAT` | a malformed `JSON_Pack` / `JSON_Unpack` format |
| `JSON_ERR_INTERNAL` | a bug in the plugin; the call was stopped, the server carries on |

Functions that create a node return it, or 0.

## Compared to pawn-json 1.x

pawn-json 1.4.1 is the latest release of
[Southclaws/pawn-json](https://github.com/Southclaws/pawn-json) (November 2022), and the
one most servers run.

### Speed

Both plugins ran the same Pawn script, [bench/server/speed.pwn](bench/server/speed.pwn),
on an open.mp 1.6.7 server: compiled once against 1.4.1's `json.inc` with its `json.dll`
loaded as a legacy plugin, and once against this `json.inc` with the component. The
script uses only 1.x functions, so it is exactly the code an existing gamemode runs. The
times are measured inside Pawn, so they include everything a script pays: reading the
file, the native calls, the conversion of strings. i7-8700K, Windows 11, the mean of 3
runs for 1.4.1 and of 200 runs for this plugin, measured in the same sitting.

| What the script does | pawn-json 1.4.1 | this plugin | |
| --- | ---: | ---: | ---: |
| Load a language file, read every key: 500 keys, 36 KB | 89 ms | 0.26 ms | 340x |
| ... 1000 keys, 71 KB | 399 ms | 0.45 ms | 900x |
| ... 2000 keys, 143 KB | 1 373 ms | 0.73 ms | 1 890x |
| ... 4000 keys, 288 KB | 5 060 ms | 1.5 ms | 3 370x |
| Load a vehicle list, read every column: 500 rows, 160 KB | 271 ms | 1.0 ms | 270x |
| ... 1000 rows, 320 KB | 1 190 ms | 1.9 ms | 630x |
| ... 2000 rows, 640 KB | 4 999 ms | 3.9 ms | 1 300x |
| 1000 reads of one string from an open 2000-key document | 507 ms | 0.12 ms | 4 400x |
| 100 000 reads of one int from an open document | 188 ms | 7.7 ms | 24x |

Every time 1.4.1 reads a value by key, it first copies the whole object it reads from,
then the value; `JSON_ArrayObject` copies the whole array to hand out one element; and
`JSON_Keys` builds a list of every key on every call
([plugin.rs](https://github.com/Southclaws/pawn-json/blob/1.4.1/src/plugin.rs#L540-L560)).
So reading every key of a file costs the file's size times its number of keys: twice the
keys, four times the time, which is the 1.4.1 column above. Here a read looks the value
up where it is and copies nothing, so the time grows with the file and no faster. The
last row is mostly the cost of a native call itself, which neither plugin can avoid.

To compare on your own files, `bench/` has `jsonbench`, which reads every `.json` and
`.jsonl` file under a folder through this plugin's core and through a model of 1.x's
natives, and checks that both hand back the same values.

### Behaviour

[bench/server/quirks.pwn](bench/server/quirks.pwn) runs these cases on both plugins,
again one script compiled against both includes:

| The script does | pawn-json 1.4.1 | this plugin |
| --- | --- | --- |
| `JSON_GetType` of a key that isn't there | `JSON_NODE_NUMBER` | `JSON_NODE_MISSING` |
| `JSON_GetString` of `"hello"` into `buf[4]` | returns 0 (success), writes nothing | `"hel"` and `JSON_ERR_BUFFER` |
| `JSON_SetFloat(o, "x", 2165.9163)`, then stringify | `{"x":2165.916259765625}` | `{"x":2165.9163}` |
| `JSON_GetInt` of `5000000000` | returns 0 (success), value `705032704` | `JSON_ERR_RANGE`, value untouched |
| Change a child got with `JSON_GetObject` | the parent doesn't change: the child is a copy | the parent changes |
| `JSON_GetNodeInt` twice on the same element | the second fails: reading deleted it | both work |
| Read `"Café Señor, open from 9 to 5. Crème brûlée is $4."` from a file | `Caf&#233; Se&#241;or, open from 9 to 5. Cr&#232;me br&#251;l&#233;e is $4.` | the same text, letter for letter |
| Pass a node to a helper function, then read it | fails: the helper's return freed it | works |

And the ones a short script can't show:

- **Saves.** 1.4.1 writes the file in place, so a crash mid-save leaves it cut short.
  Here the text goes to a temporary file that replaces the old one only when it is whole.
- **Stale handles.** 1.4.1's handles are plain counters in a hash map. Here every handle
  carries a generation, so a handle to a freed node answers `JSON_ERR_NODE` and can never
  reach a node made later in its place.
- **Code page.** 1.4.1 always converts as Cyrillic (Windows-1251), whatever the server
  uses. Here it is the server's choice, or the script's.
- **Plugin bugs.** Every native here catches a panic in its own code: that one call
  returns `JSON_ERR_INTERNAL`, the log says where, and the server carries on.
- **Errors.** 1.4.1 uses 1, 2 and 3 for different things in different functions, and a
  failed builder returns a number that looks like a node. Here one set of codes means the
  same thing everywhere, and a failed builder returns 0.

### How it is tested

- `cargo test --workspace` runs over a hundred tests of the core and of the natives,
  through a fake AMX, with no server: parsing and writing, RFC 6901's own path examples,
  the pack and unpack grammar, code pages, and a save that fails halfway leaving the old
  file whole. Others count allocations: reading by key, by path or a typed value
  allocates nothing, and reading a child copies nothing.
- A model test feeds the natives random scripts: nested functions, `Node:` variables
  reused and overwritten, loops left with `break`, nodes kept and released by hand. After
  every step it checks that nothing panics, only documented codes come back, freed
  handles stay dead and every loop ends; at the end, no node may be left alive.
  `cargo test` runs 256 such scripts, CI 20 000 on each platform.
- `tools/server-test.py` runs the Pawn scripts in `tests/pawn/` on a real open.mp server,
  674 checks in ten scripts: lifetime across helpers, hooks and recursion, a filterscript unloaded
  with its nodes, every 1.x function, the mistakes scripts actually make, saves queued
  right before shutdown landing on disk byte for byte, and code pages checked in the saved
  file's bytes.
- CI runs all of it, with `clippy -D warnings`, on Windows and Linux for the 32-bit
  targets the server uses.

## Upgrading from pawn-json 1.x

1. Replace `Southclaws/pawn-json` with `r4sheed/pawn-json` in `pawn.json`'s dependencies.
2. Remove `"json"` from `legacy_plugins` in `config.json`, and the old `json.dll` /
   `json.so` from `plugins/`.
3. Recompile. 1.x code compiles unchanged.

What behaves differently is what the table above shows: missing keys, short buffers and
numbers past 32 bits now answer with an error code, children are views, and reading a
node no longer deletes it. Text that 1.x turned into `&#225;` entities now arrives as
real letters, so code that decoded those entities can go. Set `json.codepage` to the
code page your players use.

## Building

```bash
cargo build --release --target i686-pc-windows-msvc -p pawn-json      # pawn_json.dll
cargo build --release --target i686-unknown-linux-gnu -p pawn-json    # libpawn_json.so
```

The open.mp SDK is a git submodule, so clone with `--recursive` or run
`git submodule update --init --recursive`. The Linux build needs `gcc-multilib` and
`g++-multilib`. On Linux the library uses the 32-bit `libstdc++`, which an open.mp server
already has for its own components.

## Testing

```bash
cargo test --workspace
python tools/server-test.py --server <open.mp folder> --pawncc <pawncc folder>
```

The first needs no server. The second copies `omp-server.exe` and pawncc into
`test-server/` and `tools/pawncc/` on its first run, then runs each script in
`tests/pawn/` on 127.0.0.1:7797 and prints `PASS n FAIL m` for each.

To repeat the comparison with 1.4.1, put `bench/server/speed.pwn` or `quirks.pwn` into a
server's `gamemodes/`, generate its data with `python bench/server/make-data.py
<server>/scriptfiles`, and compile it once against each `json.inc`.

## Licence

Mozilla Public License 2.0, see `LICENSE`. The original code is copyright (c) rasheed.
The AMX headers in `lib/amx/` belong to ITB CompuPhase and come under their zlib licence.
