// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

// The loop functions (JSON_Iter, JSON_IterKey, JSON_IterEntry) and JSON_Get.
#include "test"
#include <json>

Iter_WalksAnArray()
{
    new Node:list;
    JSON_Parse("[10,20,30]", list);
    new before = JSON_CountNodes(), sum, steps, v;
    new Node:row;
    while (JSON_Iter(list, row))
    {
        JSON_GetNodeInt(row, v);
        sum = sum * 100 + v;
        steps++;
    }
    T_Is(sum, 102030);
    T_Is(steps, 3);
    T_Is(_:row, 0);
    T_Is(JSON_CountNodes(), before);

    // Using the same variable again starts a new walk from the top.
    steps = 0;
    while (JSON_Iter(list, row))
    {
        steps++;
    }
    T_Is(steps, 3);

    // Breaking out leaves the loop half done. Releasing the element doesn't end it, so
    // that a loop releasing its element on every pass can't start over: the next loop
    // carries on after the break.
    while (JSON_Iter(list, row))
    {
        JSON_GetNodeInt(row, v);
        if (v == 20)
        {
            break;
        }
    }
    T_Is(JSON_Release(row), JSON_OK);
    T_Is(JSON_CountNodes(), before);
    steps = 0;
    while (JSON_Iter(list, row))
    {
        JSON_GetNodeInt(row, v);
        steps++;
    }
    T_Is(steps, 1);
    T_Is(v, 30);

    // Setting the variable to 0 after a break is what starts the next loop afresh.
    while (JSON_Iter(list, row))
    {
        break;
    }
    row = Node:0;
    steps = 0;
    while (JSON_Iter(list, row))
    {
        steps++;
    }
    T_Is(steps, 3);
    T_Is(JSON_CountNodes(), before);

    new Node:empty = JSON_Array(), Node:none;
    T_True(!JSON_Iter(empty, none));
}

Iter_KeepsAKeptElement()
{
    new Node:list;
    JSON_Parse("[1,2,3]", list);
    new Node:row, Node:kept, v, steps;
    while (JSON_Iter(list, row))
    {
        JSON_GetNodeInt(row, v);
        if (v == 1)
        {
            JSON_Keep(row);
            kept = row;
        }
        steps++;
    }
    T_Is(steps, 3);
    T_Is(JSON_GetNodeInt(kept, v), JSON_OK);
    T_Is(v, 1);
    T_Is(JSON_Release(kept), JSON_OK);
}

Iter_Nests()
{
    new Node:grid;
    JSON_Parse("[[1,2],[3]]", grid);
    new sum, v;
    new Node:row, Node:cell;
    while (JSON_Iter(grid, row))
    {
        while (JSON_Iter(row, cell))
        {
            JSON_GetNodeInt(cell, v);
            sum += v;
        }
    }
    T_Is(sum, 6);
}

Iter_WalksKeysInFileOrder()
{
    new Node:o;
    JSON_Parse("{\"zeta\":1,\"alpha\":2,\"mid\":3}", o);
    new order[3], i;
    new key[16];
    while (JSON_IterKey(o, key))
    {
        order[i++] = key[0];
    }
    T_Is(i, 3);
    T_True(order[0] == 'z' && order[1] == 'a' && order[2] == 'm');
    T_Is(key[0], 0);

    new before = JSON_CountNodes(), sum, v;
    new Node:value;
    while (JSON_IterEntry(o, key, value))
    {
        JSON_GetNodeInt(value, v);
        sum += v * (key[0] == 'z' ? 100 : key[0] == 'a' ? 10 : 1);
    }
    T_Is(sum, 123);
    T_Is(JSON_CountNodes(), before);
}

Iter_GetPicksTheReadByTag()
{
    new Node:o;
    JSON_Parse("{\"f\":2.5,\"b\":true,\"i\":411,\"s\":\"Infernus\",\"n\":{\"k\":7}}", o);
    new Float:f, bool:b, i, s[16], Node:n, k;
    T_Is(JSON_Get(o, "f", f), JSON_OK);
    T_True(f == 2.5);
    T_Is(JSON_Get(o, "b", b), JSON_OK);
    T_True(b);
    T_Is(JSON_Get(o, "i", i), JSON_OK);
    T_Is(i, 411);
    T_Is(JSON_Get(o, "s", s), JSON_OK);
    T_True(T_Same(s, "Infernus"));
    T_Is(JSON_Get(o, "n", n), JSON_OK);
    T_Is(JSON_Get(n, "k", k), JSON_OK);
    T_Is(k, 7);
    T_Is(JSON_Get(o, "s", i), JSON_ERR_TYPE);
    new short[4];
    T_Is(JSON_Get(o, "s", short), JSON_ERR_BUFFER);
    T_True(T_Same(short, "Inf"));
}

main()
{
}

forward OnGameModeInit();
public OnGameModeInit()
{
    new before = JSON_CountNodes();
    Iter_WalksAnArray();
    Iter_KeepsAKeptElement();
    Iter_Nests();
    Iter_WalksKeysInFileOrder();
    Iter_GetPicksTheReadByTag();
    T_Is(JSON_CountNodes(), before);
    T_Done();
    return 1;
}
