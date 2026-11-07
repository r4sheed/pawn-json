// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

// Calls every native at least once, through the include, on a real server.
#include "test"
#include <json>

Natives_Make()
{
    new Node:o = JSON_Object(), Node:a = JSON_Array();
    T_True(_:o > 0 && _:a > 0);
    T_Is(JSON_TypeOf(o), JSON_OBJECT);
    T_Is(JSON_TypeOf(a), JSON_ARRAY);
    T_Is(JSON_SetInt(o, "model", 411), JSON_OK);
    T_Is(JSON_SetFloat(o, "x", 2165.9163), JSON_OK);
    T_Is(JSON_SetBool(o, "on", true), JSON_OK);
    T_Is(JSON_SetString(o, "name", "Infernus"), JSON_OK);
    T_Is(JSON_SetNull(o, "none"), JSON_OK);
    T_Is(JSON_AppendInt(a, 1), JSON_OK);
    T_Is(JSON_AppendFloat(a, 2.5), JSON_OK);
    T_Is(JSON_AppendBool(a, false), JSON_OK);
    T_Is(JSON_AppendString(a, "s"), JSON_OK);
    T_Is(JSON_AppendNull(a), JSON_OK);
    T_Is(JSON_AppendNode(a, JSON_Copy(o)), JSON_OK);
    T_Is(JSON_SetNode(o, "list", a), JSON_OK);
    T_Is(JSON_TypeOf(a), JSON_MISSING);
    new text[256];
    T_Is(JSON_Stringify(o, text), JSON_OK);
    new Node:back;
    T_Is(JSON_Parse(text, back), JSON_OK);
    new v;
    T_Is(JSON_GetInt(back, "model", v), JSON_OK);
    T_Is(v, 411);
    T_Is(JSON_Stringify(o, text, 8), JSON_ERR_BUFFER);
    T_Is(text[7], 0);
}

Natives_Read()
{
    new Node:o;
    T_Is(JSON_Parse("{\"model\":411,\"x\":2165.9163,\"on\":true,\"name\":\"Infernus\",\"pos\":[1,2.5,\"three\"]}", o), JSON_OK);
    new v, Float:f, bool:b, s[16];
    T_Is(JSON_GetFloat(o, "x", f), JSON_OK);
    T_True(f == 2165.9163);
    T_Is(JSON_GetBool(o, "on", b), JSON_OK);
    T_True(b);
    T_Is(JSON_GetString(o, "name", s), JSON_OK);
    T_True(s[0] == 'I' && s[7] == 's' && s[8] == 0);
    T_Is(JSON_GetString(o, "name", s, 4), JSON_ERR_BUFFER);
    T_True(s[0] == 'I' && s[3] == 0);
    T_Is(JSON_TypeOf(o, "nope"), JSON_MISSING);
    T_Is(JSON_TypeOf(o, "x"), JSON_FLOAT);
    T_Is(JSON_Count(o, v), JSON_OK);
    T_Is(v, 5);
    T_Is(JSON_KeyAt(o, 4, s), JSON_OK);
    T_True(s[0] == 'p' && s[3] == 0);
    new Node:pos;
    T_Is(JSON_GetNode(o, "pos", pos), JSON_OK);
    new Node:el, cursor, sum;
    while (JSON_Next(pos, cursor, el) == JSON_OK)
    {
        if (JSON_ValueInt(el, v) == JSON_OK)
        {
            sum += v;
        }
        else if (JSON_ValueFloat(el, f) == JSON_OK)
        {
            sum += (f == 2.5) ? 20 : 0;
        }
        else if (JSON_ValueString(el, s) == JSON_OK)
        {
            sum += 100;
        }
    }
    T_Is(sum, 121);
    T_Is(cursor, 3);
    cursor = 0;
    new keys;
    while (JSON_NextKey(o, cursor, s) == JSON_OK)
    {
        keys++;
    }
    T_Is(keys, 5);
    cursor = 0;
    new Node:val, ints;
    while (JSON_NextEntry(o, cursor, val, s) == JSON_OK)
    {
        if (JSON_TypeOf(val) == JSON_INT)
        {
            ints++;
        }
    }
    T_Is(ints, 1);
    T_Is(JSON_ValueBool(o, b), JSON_ERR_TYPE);
}

Natives_Write()
{
    new Node:a;
    T_Is(JSON_Parse("[10,20,30,40,50,60]", a), JSON_OK);
    T_Is(JSON_ArraySetInt(a, 0, 1), JSON_OK);
    T_Is(JSON_ArraySetFloat(a, 1, 2.0), JSON_OK);
    T_Is(JSON_ArraySetBool(a, 2, true), JSON_OK);
    T_Is(JSON_ArraySetString(a, 3, "x"), JSON_OK);
    T_Is(JSON_ArraySetNull(a, 4), JSON_OK);
    T_Is(JSON_ArraySetNode(a, 5, JSON_Array()), JSON_OK);
    T_Is(JSON_ArraySetInt(a, 6, 0), JSON_ERR_KEY);
    T_Is(JSON_ArrayRemoveAt(a, 0), JSON_OK);
    new text[64];
    T_Is(JSON_Stringify(a, text), JSON_OK);
    new Node:want;
    JSON_Parse("[2.0,true,\"x\",null,[]]", want);
    new other[64];
    JSON_Stringify(want, other);
    T_True(T_Same(text, other));
    new Node:o;
    JSON_Parse("{\"a\":1,\"b\":2}", o);
    T_Is(JSON_Remove(o, "a"), JSON_OK);
    T_Is(JSON_Remove(o, "a"), JSON_ERR_KEY);
    new Node:c = JSON_Copy(o);
    T_Is(JSON_SetInt(c, "b", 3), JSON_OK);
    new v;
    JSON_GetInt(o, "b", v);
    T_Is(v, 2);
}

Natives_Files()
{
    new Node:o;
    JSON_Parse("{\"saved\":true}", o);
    T_Is(JSON_SaveFile("scriptfiles/natives.json", o, true), JSON_OK);
    new Node:back;
    T_Is(JSON_ParseFile("scriptfiles/natives.json", back), JSON_OK);
    new bool:b;
    T_Is(JSON_GetBool(back, "saved", b), JSON_OK);
    T_True(b);
    new Node:none = Node:-1;
    T_Is(JSON_ParseFile("scriptfiles/none.json", none), JSON_ERR_FILE);
    T_Is(_:none, 0);
}

Natives_Lifetime()
{
    new Node:o = JSON_Object();
    T_Is(JSON_Keep(o), JSON_OK);
    T_Is(JSON_Release(o), JSON_OK);
    T_Is(JSON_Release(o), JSON_ERR_NODE);
    T_True(JSON_SetCodepage("windows-1250"));
    T_True(!JSON_SetCodepage("utf-8"));
}

main()
{
}

forward OnGameModeInit();
public OnGameModeInit()
{
    new before = JSON_CountNodes();
    Natives_Make();
    Natives_Read();
    Natives_Write();
    Natives_Files();
    Natives_Lifetime();
    T_Is(JSON_CountNodes(), before);
    T_Done();
    return 1;
}
