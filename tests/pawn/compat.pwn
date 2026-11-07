// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

// Calls every pawn-json 1.x function at least once. They return the same codes as the
// rest of the library, and the places where 1.x misbehaved are checked for the fix.
#include "test"
#include <json>

LeaveScope(Node:node)
{
    #pragma unused node
}

Node:HandoffRow()
{
    return JSON_Object("name", JSON_String("a"), "account", JSON_Int(7));
}

Compat_ParseAndFiles()
{
    new Node:root, Node:bad;
    T_Is(JSON_Parse("{\"model\":411}", root), 0);
    T_Is(JSON_Parse("{", bad), JSON_ERR_SYNTAX);
    T_Is(JSON_SaveFile("scriptfiles/compat.json", bad), JSON_ERR_NODE); // a failed parse leaves 0 behind
    T_Is(JSON_SaveFile("scriptfiles/compat.json", root, true), JSON_OK);
    new Node:back, Node:none;
    T_Is(JSON_ParseFile("scriptfiles/compat.json", back), 0);
    T_Is(JSON_ParseFile("scriptfiles/none.json", none), JSON_ERR_FILE);
    new Node:small, text[32];
    JSON_Parse("{\"a\":[1,2]}", small);
    T_Is(JSON_Stringify(small, text), 0);
    T_True(T_Same(text, "{\"a\":[1,2]}"));
    T_Is(JSON_Stringify(small, text, 4), JSON_ERR_BUFFER);
}

Compat_Reads()
{
    new Node:root;
    JSON_Parse("{\"model\":411,\"x\":2165.9163,\"on\":true,\"name\":\"Infernus\",\"pos\":[1,2,3],\"big\":4294967296,\"none\":null}", root);
    new v, Float:f, bool:b, s[16];
    T_Is(JSON_GetInt(root, "model", v), 0);
    T_Is(v, 411);
    T_Is(JSON_GetFloat(root, "x", f), 0);
    T_True(f == 2165.9163);
    T_Is(JSON_GetBool(root, "on", b), 0);
    T_True(b);
    T_Is(JSON_GetString(root, "name", s), 0);
    T_True(T_Same(s, "Infernus"));
    T_Is(JSON_GetInt(root, "nope", v), JSON_ERR_KEY);
    T_Is(JSON_GetInt(root, "name", v), JSON_ERR_TYPE);
    T_Is(JSON_GetInt(root, "big", v), JSON_ERR_RANGE); // 1.x silently cut this to 32 bits
    T_Is(JSON_GetString(root, "name", s, 4), JSON_ERR_BUFFER); // 1.x reported success here
    T_Is(JSON_GetInt(Node:9999, "model", v), JSON_ERR_NODE);
    T_Is(_:JSON_GetType(root, "model"), _:JSON_NODE_NUMBER);
    T_Is(_:JSON_GetType(root, "name"), _:JSON_NODE_STRING);
    T_Is(_:JSON_GetType(root, "pos"), _:JSON_NODE_ARRAY);
    T_Is(_:JSON_GetType(root, "none"), _:JSON_NODE_NULL);
    T_Is(_:JSON_GetType(root, "nope"), _:JSON_NODE_MISSING); // 1.x called a missing key a number
    T_Is(_:JSON_NodeType(root), _:JSON_NODE_OBJECT);
    new key[16];
    T_Is(JSON_Keys(root, 0, key), 0);
    T_True(T_Same(key, "model"));
    T_Is(JSON_Keys(root, 99, key), JSON_ERR_KEY);

    new Node:pos, len;
    T_Is(JSON_GetArray(root, "pos", pos), 0);
    T_Is(JSON_GetArray(root, "name", pos), JSON_ERR_TYPE);
    T_Is(JSON_GetArray(root, "nope", pos), JSON_ERR_KEY);
    T_Is(JSON_ArrayLength(pos, len), 0);
    T_Is(len, 3);
    T_Is(JSON_ArrayLength(pos, len), JSON_OK);
    new Node:el;
    T_Is(JSON_ArrayObject(pos, 1, el), 0);
    T_Is(JSON_GetNodeInt(el, v), 0);
    T_Is(v, 2);
    T_Is(JSON_GetNodeInt(el, v), 0); // reading twice works; 1.x deleted the node on the first read
    T_Is(JSON_ArrayObject(pos, 3, el), JSON_ERR_KEY);
    new count = JSON_CountNodes(), index = -1, sum;
    while (!JSON_ArrayIterate(pos, index, el))
    {
        JSON_GetNodeInt(el, v);
        sum += v;
    }
    T_Is(sum, 6);
    T_Is(index, 3);
    T_Is(JSON_CountNodes(), count); // the walk reused one handle the whole way

    new Node:obj;
    T_Is(JSON_GetObject(root, "pos", obj), 0);
    T_Is(JSON_GetObject(root, "nope", obj), JSON_ERR_KEY);
    T_Is(JSON_GetObject(Node:9999, "pos", obj), JSON_ERR_NODE);

    new Node:nums;
    JSON_Parse("[1.5,true,\"s\"]", nums);
    JSON_ArrayObject(nums, 0, el);
    T_Is(JSON_GetNodeFloat(el, f), 0);
    T_True(f == 1.5);
    JSON_ArrayObject(nums, 1, el);
    T_Is(JSON_GetNodeBool(el, b), 0);
    T_True(b);
    JSON_ArrayObject(nums, 2, el);
    T_Is(JSON_GetNodeString(el, s), 0);
    T_True(T_Same(s, "s"));
    T_Is(JSON_GetNodeInt(el, v), JSON_ERR_TYPE);
}

Compat_Builders()
{
    new before = JSON_CountNodes();
    new Node:o = JSON_Object("name", JSON_String("Zsofi"), "age", JSON_Int(30), "on", JSON_Bool(true), "x", JSON_Float(1.5), "list", JSON_Array(JSON_Int(1), JSON_Int(2)));
    T_True(_:o > 0);
    T_Is(JSON_CountNodes(), before + 1); // the builders used up the nodes passed in
    new text[96];
    JSON_Stringify(o, text);
    T_True(T_Same(text, "{\"name\":\"Zsofi\",\"age\":30,\"on\":true,\"x\":1.5,\"list\":[1,2]}"));
    T_Is(_:JSON_Object("odd"), 0); // a failed build is 0, not an error code that looks like a node

    new Node:joined = JSON_Append(JSON_Object("a", JSON_Int(1)), JSON_Object("b", JSON_Int(2)));
    JSON_Stringify(joined, text);
    T_True(T_Same(text, "{\"a\":1,\"b\":2}"));
    new Node:both = JSON_Array(JSON_Int(1)) + JSON_Array(JSON_Int(2));
    JSON_Stringify(both, text);
    T_True(T_Same(text, "[1,2]"));
    T_Is(_:JSON_Append(JSON_Int(1), JSON_Array()), 0);
}

Compat_Writes()
{
    new Node:o = JSON_Object();
    T_Is(JSON_SetInt(o, "i", 1), 0);
    T_Is(JSON_SetFloat(o, "f", 2165.9163), 0);
    T_Is(JSON_SetBool(o, "b", false), 0);
    new s[] = "str";
    T_Is(JSON_SetString(o, "s", s), 0);
    T_Is(JSON_SetObject(o, "o", JSON_Object("k", JSON_Int(1))), 0);
    T_Is(JSON_SetArray(o, "a", JSON_Array(JSON_Int(1), JSON_Int(2), JSON_Int(1))), 0);
    T_Is(JSON_SetObject(o, "bad", JSON_Array()), JSON_OK); // SetObject is just SetNode, so it accepts any node
    T_Is(JSON_Remove(o, "bad"), JSON_OK);
    T_Is(JSON_SetInt(Node:9999, "i", 1), JSON_ERR_NODE);
    T_Is(JSON_Remove(o, "b"), 0);
    T_Is(JSON_Remove(o, "b"), JSON_ERR_KEY);
    T_Is(JSON_ArrayAppend(o, "a", JSON_Int(3)), 0);
    T_Is(JSON_ArrayAppend(o, "i", JSON_Int(3)), JSON_ERR_TYPE);
    T_Is(JSON_ArrayRemove(o, "a", JSON_Int(1)), 0);
    T_Is(JSON_ArrayRemoveIndex(o, "a", 0), 0);
    T_Is(JSON_ArrayRemoveIndex(o, "a", 9), JSON_ERR_KEY);
    new text[96];
    JSON_Stringify(o, text);
    T_True(T_Same(text, "{\"i\":1,\"f\":2165.9163,\"s\":\"str\",\"o\":{\"k\":1},\"a\":[3]}")); // floats are written in their short form
    T_Is(JSON_ArrayClear(o, "a"), 0);
    T_Is(JSON_ArrayClear(o, "i"), JSON_ERR_TYPE);
    JSON_Stringify(o, text);
    T_True(T_Same(text, "{\"i\":1,\"f\":2165.9163,\"s\":\"str\",\"o\":{\"k\":1},\"a\":[]}"));

    new Node:view;
    JSON_GetObject(o, "o", view);
    JSON_SetInt(view, "k", 2);
    new v;
    new Node:again;
    JSON_GetObject(o, "o", again);
    JSON_GetInt(again, "k", v);
    T_Is(v, 2); // the write went through the view; 1.x handed out copies
}

Compat_Lifetime()
{
    new Node:kept = JSON_Object();
    T_Is(JSON_ToggleGC(kept, false), 0);
    LeaveScope(kept);
    T_Is(_:JSON_NodeType(kept), _:JSON_NODE_OBJECT); // it was kept, so the helper's scope left it alone
    T_Is(JSON_ToggleGC(kept, true), 0);
    T_Is(JSON_Cleanup(kept), 0);
    T_Is(JSON_Cleanup(kept), JSON_ERR_NODE);
    new Node:passed = JSON_Object();
    LeaveScope(passed);
    T_Is(_:JSON_NodeType(passed), _:JSON_NODE_OBJECT); // 1.x would have freed this when the helper returned
    new Node:row = HandoffRow();
    T_Is(_:JSON_NodeType(row), _:JSON_NODE_OBJECT); // returned without JSON_Up, the way 1.x code does it
}

main()
{
}

forward OnGameModeInit();
public OnGameModeInit()
{
    new before = JSON_CountNodes();
    Compat_ParseAndFiles();
    Compat_Reads();
    Compat_Builders();
    Compat_Writes();
    Compat_Lifetime();
    T_Is(JSON_CountNodes(), before);
    T_Done();
    return 1;
}
