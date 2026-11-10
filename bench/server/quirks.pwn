// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

// Eight things pawn-json 1.x gets wrong, each printed as a Q line. Like speed.pwn, it
// compiles against either include, so the two logs can be read side by side. Q7 reads
// scriptfiles/accent.json, which make-data.py writes.

#pragma rational Float

native print(const string[]);
native printf(const format[], {Float, _}:...);
native SendRconCommand(const command[]);

#include <json>

main() {}

Helper(Node:n)
{
    #pragma unused n
}

forward OnGameModeInit();
public OnGameModeInit()
{
    new Node:root, r, v, buf[4], out[256];

    JSON_Parse("{\"a\":1}", root);
    printf("Q1 type of a missing key: %d (number is 0)", _:JSON_GetType(root, "nope"));

    JSON_Parse("{\"s\":\"hello\"}", root);
    buf = "xyz";
    r = JSON_GetString(root, "s", buf);
    printf("Q2 string into a 4-cell buffer: returns %d, buffer \"%s\"", r, buf);

    root = JSON_Object();
    JSON_SetFloat(root, "x", 2165.9163);
    JSON_Stringify(root, out);
    printf("Q3 SetFloat 2165.9163 writes %s", out);

    JSON_Parse("{\"big\":5000000000}", root);
    v = -1;
    r = JSON_GetInt(root, "big", v);
    printf("Q4 GetInt of 5000000000: returns %d, value %d", r, v);

    new Node:child;
    JSON_Parse("{\"a\":{\"x\":1}}", root);
    JSON_GetObject(root, "a", child);
    JSON_SetInt(child, "x", 2);
    JSON_Stringify(root, out);
    printf("Q5 after setting x=2 on the child, the parent is %s", out);

    new Node:arr, Node:el;
    JSON_Parse("[7]", arr);
    JSON_ArrayObject(arr, 0, el);
    r = JSON_GetNodeInt(el, v);
    new r2 = JSON_GetNodeInt(el, v);
    printf("Q6 reading the same element twice: %d then %d", r, r2);

    JSON_ParseFile("scriptfiles/accent.json", root);
    JSON_GetString(root, "t", out);
    printf("Q7 %s", out);

    JSON_Parse("{\"a\":1}", root);
    Helper(root);
    v = -1;
    r = JSON_GetInt(root, "a", v);
    printf("Q8 after passing the node to a helper: returns %d, value %d", r, v);

    SendRconCommand("exit");
    return 1;
}
