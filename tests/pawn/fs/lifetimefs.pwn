// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

// lifetime.pwn loads and unloads this filterscript. It keeps ten nodes, and unloading
// it has to free all of them.
#include <json>

forward OnFilterScriptInit();
public OnFilterScriptInit()
{
    for (new i = 0; i != 10; ++i)
    {
        new Node:n;
        JSON_Parse("[]", n);
        JSON_Keep(n);
    }
    return 1;
}
