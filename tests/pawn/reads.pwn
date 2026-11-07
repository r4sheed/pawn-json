// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

// Reading by path, by array index, and JSON_HasKey.
#include "test"
#include <json>

main()
{
}

forward OnGameModeInit();
public OnGameModeInit()
{
    new Node:r;
    T_Is(JSON_Parse("{\"vehicles\":[{\"model\":400},{\"model\":401},{\"model\":402},{\"model\":411,\"x\":2165.9163,\"on\":true,\"name\":\"Infernus\"}],\"a/b\":1}", r), JSON_OK);

    new v, Float:f, bool:b, s[16];
    T_Is(JSON_PathGetInt(r, "/vehicles/3/model", v), JSON_OK);
    T_Is(v, 411);
    T_Is(JSON_PathGetFloat(r, "/vehicles/3/x", f), JSON_OK);
    T_True(f == 2165.9163);
    T_Is(JSON_PathGetBool(r, "/vehicles/3/on", b), JSON_OK);
    T_True(b);
    T_Is(JSON_PathGetString(r, "/vehicles/3/name", s), JSON_OK);
    T_True(T_Same(s, "Infernus"));
    T_Is(JSON_PathGetInt(r, "/a~1b", v), JSON_OK);
    T_Is(v, 1);
    T_Is(JSON_PathGetInt(r, "/vehicles/9/model", v), JSON_ERR_KEY);
    T_Is(JSON_PathGetInt(r, "vehicles", v), JSON_ERR_PATH);

    new Node:list;
    T_Is(JSON_PathGetNode(r, "/vehicles", list), JSON_OK);
    new count = JSON_CountNodes();
    new Node:third;
    T_Is(JSON_ArrayGetNode(list, 2, third), JSON_OK);
    T_Is(JSON_CountNodes(), count + 1);
    T_Is(JSON_GetInt(third, "model", v), JSON_OK);
    T_Is(v, 402);

    new Node:nums;
    T_Is(JSON_Parse("[7,2.5,false,\"s\"]", nums), JSON_OK);
    count = JSON_CountNodes();
    T_Is(JSON_ArrayGetInt(nums, 0, v), JSON_OK);
    T_Is(v, 7);
    T_Is(JSON_ArrayGetFloat(nums, 1, f), JSON_OK);
    T_True(f == 2.5);
    T_Is(JSON_ArrayGetBool(nums, 2, b), JSON_OK);
    T_True(!b);
    T_Is(JSON_ArrayGetString(nums, 3, s), JSON_OK);
    T_True(T_Same(s, "s"));
    T_Is(JSON_ArrayGetInt(nums, 4, v), JSON_ERR_KEY);
    T_Is(JSON_CountNodes(), count);

    T_True(JSON_HasKey(r, "vehicles"));
    T_True(!JSON_HasKey(r, "nope"));
    T_True(!JSON_HasKey(list, "0"));

    T_Done();
    return 1;
}
