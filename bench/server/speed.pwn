// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

// Times what a gamemode does with JSON, on a real server. It only calls pawn-json 1.x's
// functions, which this plugin keeps under the same names, so the same script compiles
// against either include and runs on either plugin. compare.py builds and runs it on
// both; make-data.py writes the files it reads.
//
// Each workload is one call of a function below, timed from the outside, so the time
// includes everything the script pays: reading the file, parsing, every native call,
// converting strings, and freeing the nodes when the function returns. The server's
// clock counts whole milliseconds, so a workload is run again and again until MIN_MS
// have passed (at least once), and the time of one run is the total divided by the
// runs. Each workload's result is summed into a check, which has to be the same on both
// plugins: they did the same work and read the same values.
//
// Output, one line per workload: RESULT <name> runs=<n> ms=<total> check=<sum>

native print(const string[]);
native printf(const format[], {Float, _}:...);
native SendRconCommand(const command[]);
native GetTickCount();
native strlen(const string[]);
native floatround(Float:value, method = 0); // 0 rounds to the nearest

#include <json>

#if !defined MIN_MS
    #define MIN_MS 1000
#endif

main() {}

// Every key of a flat language file, read the way a text loader does.
Lang(const file[])
{
    new Node:root;
    if (JSON_ParseFile(file, root))
    {
        printf("cannot parse %s", file);
        return 0;
    }
    new key[32], text[256], sum;
    for (new i = 0; !JSON_Keys(root, i, key); ++i)
    {
        JSON_GetString(root, key, text);
        sum += strlen(text) + 1;
    }
    return sum;
}

// Every column of every row of a vehicle list.
Rows(const file[])
{
    new Node:root, Node:list, Node:row, len;
    if (JSON_ParseFile(file, root))
    {
        printf("cannot parse %s", file);
        return 0;
    }
    JSON_GetArray(root, "vehicles", list);
    JSON_ArrayLength(list, len);
    new model, Float:x, Float:y, Float:z, Float:a, c1, c2, plate[16], owner[32], bool:locked, sum;
    for (new i = 0; i < len; ++i)
    {
        JSON_ArrayObject(list, i, row);
        JSON_GetInt(row, "model", model);
        JSON_GetFloat(row, "x", x);
        JSON_GetFloat(row, "y", y);
        JSON_GetFloat(row, "z", z);
        JSON_GetFloat(row, "angle", a);
        JSON_GetInt(row, "color1", c1);
        JSON_GetInt(row, "color2", c2);
        JSON_GetString(row, "plate", plate);
        JSON_GetString(row, "owner", owner);
        JSON_GetBool(row, "locked", locked);
        sum += model + c1 + c2 + strlen(plate) + strlen(owner) + _:locked
            + floatround(x) + floatround(y) + floatround(z) + floatround(a);
    }
    return sum;
}

// One number read again and again from a small document that stays open.
new Node:gAccount;

Hot(reads)
{
    new money, sum;
    for (new i = 0; i < reads; ++i)
    {
        JSON_GetInt(gAccount, "money", money);
        sum += money;
    }
    return sum;
}

// One string read again and again from a big document that stays open, the way a text
// lookup works.
new Node:gTexts;

HotText(reads)
{
    new text[256], sum;
    for (new i = 0; i < reads; ++i)
    {
        JSON_GetString(gTexts, "MSG_01000", text);
        sum += strlen(text);
    }
    return sum;
}

Work(w)
{
    switch (w)
    {
        case 0: return Lang("scriptfiles/lang-500.json");
        case 1: return Lang("scriptfiles/lang-1000.json");
        case 2: return Lang("scriptfiles/lang-2000.json");
        case 3: return Lang("scriptfiles/lang-4000.json");
        case 4: return Rows("scriptfiles/vehicles-500.json");
        case 5: return Rows("scriptfiles/vehicles-1000.json");
        case 6: return Rows("scriptfiles/vehicles-2000.json");
        case 7: return HotText(1000);
        case 8: return Hot(100000);
    }
    return 0;
}

static const gNames[][] = {
    "lang-500", "lang-1000", "lang-2000", "lang-4000",
    "vehicles-500", "vehicles-1000", "vehicles-2000",
    "hot-string-1000", "hot-int-100000"
};

forward OnGameModeInit();
public OnGameModeInit()
{
    // The open documents for the two hot workloads, made once and kept, so that only the
    // reads are timed.
    JSON_Parse("{\"name\":\"Player_1\",\"money\":2500,\"score\":17,\"skin\":101,\"x\":1958.33,\"y\":1343.12,\"z\":15.36,\"admin\":0,\"vip\":false,\"email\":\"someone@example.com\",\"inventory\":[1,2,3,4,5,6,7,8]}", gAccount);
    JSON_ParseFile("scriptfiles/lang-2000.json", gTexts);
    JSON_ToggleGC(gAccount, false);
    JSON_ToggleGC(gTexts, false);

    for (new w = 0; w < sizeof (gNames); ++w)
    {
        new runs, check, start = GetTickCount(), elapsed;
        do
        {
            check = Work(w);
            ++runs;
            elapsed = GetTickCount() - start;
        }
        while (elapsed < MIN_MS);
        printf("RESULT %s runs=%d ms=%d check=%d", gNames[w], runs, elapsed, check);
    }
    print("RESULT done");
    SendRconCommand("exit");
    return 1;
}
