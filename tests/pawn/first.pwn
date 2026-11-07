// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

#include "test"
#include <json>

main()
{
}

forward OnGameModeInit();
public OnGameModeInit()
{
    new Node:root, value;
    T_Is(JSON_Parse("{\"model\":411,\"big\":4294967296}", root), JSON_OK);
    T_True(_:root > 0);
    T_Is(JSON_GetInt(root, "model", value), JSON_OK);
    T_Is(value, 411);
    value = 7;
    T_Is(JSON_GetInt(root, "big", value), JSON_ERR_RANGE);
    T_Is(value, 7);
    T_Is(JSON_GetInt(root, "none", value), JSON_ERR_KEY);
    T_Is(JSON_GetInt(Node:12345, "model", value), JSON_ERR_NODE);
    T_Is(JSON_Parse("{", root), JSON_ERR_SYNTAX);
    T_Is(_:root, 0);
    T_Done();
    return 1;
}
