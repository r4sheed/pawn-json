// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

// What the README's usage examples cost on this plugin, next to plain Pawn doing the same
// job. Needs scriptfiles/lang-2000.json from make-data.py.
//
// Each workload does its operation `count` times in a loop, and is run again and again
// until MIN_MS have passed, since the server's clock counts whole milliseconds. The time
// of one operation is the total divided by runs * count, loop included; the empty loop is
// measured too, so it can be taken off.
//
// Output, one line per workload: RESULT <name> ops=<n> ms=<total> check=<sum>

#pragma rational Float

native print(const string[]);
native printf(const format[], {Float, _}:...);
native format(output[], len = sizeof (output), const format[], {Float, _}:...);
native SendRconCommand(const command[]);
native GetTickCount();
native strlen(const string[]);
native strcat(dest[], const source[], maxlength = sizeof (dest));
native strcmp(const string1[], const string2[], bool:ignorecase = false, length = cellmax);
native Float:float(value);

#include <json>

#if !defined MIN_MS
    #define MIN_MS 1000
#endif

#define TELEPORTS 1000

main() {}

new Node:gTexts, Node:gSmall;
new gLine[] = "You do not have enough money for this car, come back later (%d)";

// The teleports, three ways: a JSON array as in the README, the same in a Pawn array, and
// a JSON object keyed by name.
new Node:gList[3], Node:gByName;

enum E_TELEPORT
{
    E_NAME[32],
    Float:E_X,
    Float:E_Y,
    Float:E_Z,
    Float:E_ANGLE,
    E_INTERIOR,
    E_WORLD
}
new gArray[TELEPORTS][E_TELEPORT];

Nothing(x)
{
    return x;
}

// The README's search: unpack every row, then compare the name.
Unpacked(Node:list, const name[])
{
    new Node:teleport;
    while (JSON_Iter(list, teleport))
    {
        new found[32], Float:x, Float:y, Float:z, Float:angle, interior, world;
        if (JSON_Unpack(teleport, "{name:s[32], pos:[f, f, f], angle:f, interior:i, world:i}",
                found, x, y, z, angle, interior, world) != JSON_OK
            || strcmp(found, name, true) != 0)
        {
            continue;
        }
        return interior + world + 1;
    }
    return 0;
}

// Read the name first, and unpack only the row that matches.
NameFirst(Node:list, const name[])
{
    new Node:teleport, found[32];
    while (JSON_Iter(list, teleport))
    {
        if (JSON_GetString(teleport, "name", found) != JSON_OK || strcmp(found, name, true) != 0)
        {
            continue;
        }
        new Float:x, Float:y, Float:z, Float:angle, interior, world;
        JSON_Unpack(teleport, "{pos:[f, f, f], angle:f, interior:i, world:i}",
            x, y, z, angle, interior, world);
        return interior + world + 1;
    }
    return 0;
}

// Plain Pawn: the same rows in an enum array.
InArray(count, const name[])
{
    for (new i = 0; i < count; ++i)
    {
        if (strcmp(gArray[i][E_NAME], name, true) == 0)
        {
            return gArray[i][E_INTERIOR] + gArray[i][E_WORLD] + 1;
        }
    }
    return 0;
}

// An object keyed by name: one lookup, then unpack.
ByName(const name[])
{
    new Node:teleport;
    if (JSON_GetNode(gByName, name, teleport) != JSON_OK)
    {
        return 0;
    }
    new Float:x, Float:y, Float:z, Float:angle, interior, world;
    JSON_Unpack(teleport, "{pos:[f, f, f], angle:f, interior:i, world:i}",
        x, y, z, angle, interior, world);
    return interior + world + 1;
}

static const gNames[][] = {
    "empty-loop", "pawn-call", "strlen", "strcat-copy", "format-copy",
    "get-string-2000-keys", "get-string-11-keys",
    "unpack-search-10", "unpack-search-100", "unpack-search-1000",
    "name-first-search-10", "name-first-search-100", "name-first-search-1000",
    "pawn-array-search-10", "pawn-array-search-100", "pawn-array-search-1000",
    "by-name-1000"
};

// How many operations one run of a workload does.
static const gCounts[] = {
    100000, 100000, 100000, 100000, 100000,
    100000, 100000,
    1000, 100, 10,
    1000, 100, 10,
    1000, 100, 10,
    100000
};

Work(w, count)
{
    new sum, text[144];
    switch (w)
    {
        case 0: for (new i = 0; i < count; ++i) sum += i;
        case 1: for (new i = 0; i < count; ++i) sum += Nothing(i);
        case 2: for (new i = 0; i < count; ++i) sum += strlen(gLine);
        case 3: for (new i = 0; i < count; ++i) { text[0] = 0; strcat(text, gLine); sum += text[3]; }
        case 4: for (new i = 0; i < count; ++i) { format(text, sizeof (text), "%s", gLine); sum += text[3]; }
        case 5: for (new i = 0; i < count; ++i) { JSON_GetString(gTexts, "MSG_01000", text); sum += text[3]; }
        case 6: for (new i = 0; i < count; ++i) { JSON_GetString(gSmall, "email", text); sum += text[3]; }
        case 7: for (new i = 0; i < count; ++i) sum += Unpacked(gList[0], "tp0009");
        case 8: for (new i = 0; i < count; ++i) sum += Unpacked(gList[1], "tp0099");
        case 9: for (new i = 0; i < count; ++i) sum += Unpacked(gList[2], "tp0999");
        case 10: for (new i = 0; i < count; ++i) sum += NameFirst(gList[0], "tp0009");
        case 11: for (new i = 0; i < count; ++i) sum += NameFirst(gList[1], "tp0099");
        case 12: for (new i = 0; i < count; ++i) sum += NameFirst(gList[2], "tp0999");
        case 13: for (new i = 0; i < count; ++i) sum += InArray(10, "tp0009");
        case 14: for (new i = 0; i < count; ++i) sum += InArray(100, "tp0099");
        case 15: for (new i = 0; i < count; ++i) sum += InArray(1000, "tp0999");
        case 16: for (new i = 0; i < count; ++i) sum += ByName("tp0999");
    }
    return sum;
}

// The teleport rows: tp0000, tp0001, ... with made-up positions.
MakeTeleports()
{
    new sizes[] = {10, 100, TELEPORTS};
    for (new l = 0; l < sizeof (sizes); ++l)
    {
        gList[l] = JSON_Array();
        JSON_Keep(gList[l]);
    }
    gByName = JSON_Object();
    JSON_Keep(gByName);
    for (new i = 0; i < TELEPORTS; ++i)
    {
        new name[32], Float:x = float(i), Float:y = float(i) * 2.0, Float:z = 10.0, Float:a = 90.0;
        format(name, sizeof (name), "tp%04d", i);
        for (new l = 0; l < sizeof (sizes); ++l)
        {
            if (i < sizes[l])
            {
                JSON_AppendNode(gList[l], JSON_Pack("{name:s, pos:[f, f, f], angle:f, interior:i, world:i}",
                    name, x, y, z, a, 0, 0));
            }
        }
        JSON_SetNode(gByName, name, JSON_Pack("{pos:[f, f, f], angle:f, interior:i, world:i}",
            x, y, z, a, 0, 0));
        strcat(gArray[i][E_NAME], name, 32);
        gArray[i][E_X] = x;
        gArray[i][E_Y] = y;
        gArray[i][E_Z] = z;
        gArray[i][E_ANGLE] = a;
    }
}

forward OnGameModeInit();
public OnGameModeInit()
{
    JSON_ParseFile("scriptfiles/lang-2000.json", gTexts);
    JSON_Keep(gTexts);
    JSON_Parse("{\"name\":\"Player_1\",\"money\":2500,\"score\":17,\"skin\":101,\"x\":1958.33,\"y\":1343.12,\"z\":15.36,\"admin\":0,\"vip\":false,\"email\":\"someone@example.com\",\"inventory\":[1,2,3,4,5,6,7,8]}", gSmall);
    JSON_Keep(gSmall);
    MakeTeleports();

    for (new w = 0; w < sizeof (gNames); ++w)
    {
        new runs, check, start = GetTickCount(), elapsed;
        do
        {
            check = Work(w, gCounts[w]);
            ++runs;
            elapsed = GetTickCount() - start;
        }
        while (elapsed < MIN_MS);
        printf("RESULT %s ops=%d ms=%d check=%d", gNames[w], runs * gCounts[w], elapsed, check);
    }
    print("RESULT done");
    SendRconCommand("exit");
    return 1;
}
