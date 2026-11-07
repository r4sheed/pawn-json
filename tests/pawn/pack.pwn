// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

// JSON_Unpack reading a vehicle row in one call, and JSON_Pack building a tree from a
// format.
#include "test"
#include <json>

main()
{
}

forward OnGameModeInit();
public OnGameModeInit()
{
    new before = JSON_CountNodes();
    Pack_UnpackARow();
    Pack_BuildATree();
    Pack_AnOptionalColumn();
    T_Is(JSON_CountNodes(), before);
    T_Done();
    return 1;
}

Pack_UnpackARow()
{
    new Node:row;
    T_Is(JSON_Parse("{\"model\": 549, \"x\": -1655.78, \"y\": 1315.25, \"z\": 8.7917, \"angle\": 315.0, \"color1\": -1, \"color2\": -1, \"paintjob\": 3, \"respawn\": 0, \"source\": \"both\", \"note\": \"sfe_stream3.ipl, random\"}", row), JSON_OK);
    new model, Float:x, Float:y, Float:z, Float:angle, color1, color2, paintjob, respawn, source[8], note[32];
    new code = JSON_Unpack(row, "{model:i, x:f, y:f, z:f, angle:f, color1:i, color2:i, paintjob:i, respawn:i, source:s[8], note:s[32]}",
        model, x, y, z, angle, color1, color2, paintjob, respawn, source, note);
    T_Is(code, JSON_OK);
    T_Is(model, 549);
    T_True(x == -1655.78 && y == 1315.25 && z == 8.7917 && angle == 315.0);
    T_True(color1 == -1 && color2 == -1 && paintjob == 3 && respawn == 0);
    T_True(T_Same(source, "both"));
    T_True(T_Same(note, "sfe_stream3.ipl, random"));
    new short[4];
    T_Is(JSON_Unpack(row, "{source:s[4]}", short), JSON_ERR_BUFFER);
    T_True(T_Same(short, "bot"));
    T_Is(JSON_Unpack(row, "{model:x}", model), JSON_ERR_FORMAT);
}

Pack_BuildATree()
{
    new name[] = "Zsofi", age = 30, Float:px = 1.5, Float:py = -2.0, Float:pz = 3.25;
    new Node:extra;
    JSON_Parse("{\"k\":true}", extra);
    new Node:tree = JSON_Pack("{name:s, age:i, pos:[f, f, f], on:b, none:n, extra:o}", name, age, px, py, pz, true, extra);
    T_True(_:tree > 0);
    T_Is(JSON_TypeOf(extra), JSON_MISSING);
    new text[128];
    T_Is(JSON_Stringify(tree, text), JSON_OK);
    T_True(T_Same(text, "{\"name\":\"Zsofi\",\"age\":30,\"pos\":[1.5,-2.0,3.25],\"on\":true,\"none\":null,\"extra\":{\"k\":true}}"));
    new back[8], a, Float:b;
    T_Is(JSON_Unpack(tree, "{name:s[8], age:i, pos:[f, f?]}", back, a, b, b), JSON_OK);
    T_True(T_Same(back, "Zsofi") && a == 30);
    T_Is(_:JSON_Pack("{a:i?}", a), 0);
}

Pack_AnOptionalColumn()
{
    new Node:row;
    JSON_Parse("{\"model\":411}", row);
    new model, note[16] = "none";
    T_Is(JSON_Unpack(row, "{model:i, note:s[16]?}", model, note), JSON_OK);
    T_Is(model, 411);
    T_True(T_Same(note, "none"));
    T_Is(JSON_Unpack(row, "{model:i, note:s[16]}", model, note), JSON_ERR_KEY);
}
