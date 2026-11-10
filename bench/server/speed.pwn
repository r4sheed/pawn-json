// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

// Times what a gamemode does with JSON at boot, on a real server. It only calls pawn-json
// 1.x's functions, which this plugin keeps under the same names, so the same script
// compiles against either include and runs on either plugin. Each line it prints starts
// with RESULT and ends with the milliseconds it took. make-data.py writes the files it
// reads into scriptfiles/. Build with REPS=<n> to run everything n times.

native print(const string[]);
native printf(const format[], {Float, _}:...);
native SendRconCommand(const command[]);
native GetTickCount();
native strlen(const string[]);

#include <json>

#if !defined REPS
    #define REPS 1
#endif

main() {}

// Every key of a flat language file, read the way a text loader does.
Lang(const file[])
{
    new t = GetTickCount();
    new Node:root;
    if (JSON_ParseFile(file, root))
    {
        printf("cannot parse %s", file);
        return;
    }
    new key[32], text[256], n, sum;
    for (new i = 0; !JSON_Keys(root, i, key); ++i)
    {
        JSON_GetString(root, key, text);
        sum += strlen(text);
        ++n;
    }
    printf("RESULT lang %s keys=%d chars=%d ms=%d", file, n, sum, GetTickCount() - t);
}

// Every column of every row of a vehicle list.
Rows(const file[])
{
    new t = GetTickCount();
    new Node:root, Node:list, Node:row, len;
    if (JSON_ParseFile(file, root))
    {
        printf("cannot parse %s", file);
        return;
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
        sum += model + c1 + c2 + strlen(plate) + strlen(owner) + _:locked;
    }
    printf("RESULT rows %s rows=%d sum=%d ms=%d", file, len, sum, GetTickCount() - t);
}

// Reads one number again and again from a small document that stays open.
Hot(reads)
{
    new Node:acc;
    JSON_Parse("{\"name\":\"Player_1\",\"money\":2500,\"score\":17,\"skin\":101,\"x\":1958.33,\"y\":1343.12,\"z\":15.36,\"admin\":0,\"vip\":false,\"email\":\"someone@example.com\",\"inventory\":[1,2,3,4,5,6,7,8]}", acc);
    new t = GetTickCount(), money, sum;
    for (new i = 0; i < reads; ++i)
    {
        JSON_GetInt(acc, "money", money);
        sum += money;
    }
    printf("RESULT hot reads=%d sum=%d ms=%d", reads, sum, GetTickCount() - t);
}

// One string from a big document that stays open, the way a text lookup works.
HotText(const file[], reads)
{
    new Node:root;
    JSON_ParseFile(file, root);
    new t = GetTickCount(), text[256], sum;
    for (new i = 0; i < reads; ++i)
    {
        JSON_GetString(root, "MSG_01000", text);
        sum += strlen(text);
    }
    printf("RESULT hottext reads=%d sum=%d ms=%d", reads, sum, GetTickCount() - t);
}

forward OnGameModeInit();
public OnGameModeInit()
{
    for (new r = 0; r < REPS; ++r)
    {
        Run();
    }
    print("RESULT done");
    SendRconCommand("exit");
    return 1;
}

Run()
{
    Lang("scriptfiles/lang-500.json");
    Lang("scriptfiles/lang-1000.json");
    Lang("scriptfiles/lang-2000.json");
    Lang("scriptfiles/lang-4000.json");
    Rows("scriptfiles/vehicles-500.json");
    Rows("scriptfiles/vehicles-1000.json");
    Rows("scriptfiles/vehicles-2000.json");
    Hot(100000);
    HotText("scriptfiles/lang-2000.json", 1000);
}
