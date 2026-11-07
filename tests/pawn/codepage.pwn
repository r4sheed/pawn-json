// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

// Checks that strings survive the round trip through the code page. The test server's
// config.json sets windows-1250; the script then switches itself to windows-1252, and the
// file it saves has to be plain UTF-8 (the runner looks at its bytes afterwards).
#include "test"
#include <json>

main()
{
}

forward OnGameModeInit();
public OnGameModeInit()
{
    new s[4], out[8];
    s[0] = 0xF5;
    s[1] = 0xE1;
    s[2] = 'x';
    new Node:o = JSON_Object();
    T_Is(JSON_SetString(o, "n", s), JSON_OK);
    T_Is(JSON_GetString(o, "n", out), JSON_OK);
    T_True(T_Same(out, s));
    T_Is(JSON_SaveFile("scriptfiles/codepage.json", o), JSON_OK);
    new Node:back;
    T_Is(JSON_ParseFile("scriptfiles/codepage.json", back), JSON_OK);
    T_Is(JSON_GetString(back, "n", out), JSON_OK);
    T_True(T_Same(out, s));
    T_True(JSON_SetCodepage("windows-1252"));
    T_Is(JSON_GetString(back, "n", out), JSON_OK);
    T_Is(out[0], '?');
    T_Is(out[1], 0xE1);
    T_Is(out[2], 'x');
    T_Done();
    return 1;
}
