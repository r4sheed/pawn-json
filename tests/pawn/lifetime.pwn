// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

#include "test"
#include <json>

new Node:gSeen;

// Returns the node's "v", or -1 once the node has been freed.
Read(Node:n)
{
    new v = -1;
    JSON_GetInt(n, "v", v);
    return v;
}

Node:Make(v)
{
    new text[16], Node:n;
    format(text, sizeof (text), "{\"v\":%d}", v);
    JSON_Parse(text, n);
    return JSON_Up(n);
}

Helper(Node:n)
{
    return Read(n);
}

forward Lifetime_Hook(Node:n);
public Lifetime_Hook(Node:n)
{
    return Read(n);
}

MakeAndLeave()
{
    new Node:m = Make(3);
    gSeen = m;
}

Node:Middle()
{
    return JSON_Up(Make(4));
}

KeepOne()
{
    new Node:m = Make(5);
    JSON_Keep(m);
    gSeen = m;
}

Recurse(depth)
{
    new Node:m = Make(depth);
    if (depth > 0)
    {
        Recurse(depth - 1);
    }
    return Read(m) == depth;
}

Lifetime_AHelperLeavesItsArgumentAlive()
{
    new Node:n = Make(1);
    T_Is(Helper(n), 1);
    T_Is(Read(n), 1);
}

Lifetime_AHookLeavesItsArgumentAlive()
{
    new Node:n = Make(2);
    T_Is(CallLocalFunction("Lifetime_Hook", "d", _:n), 2);
    T_Is(Read(n), 2);
}

Lifetime_ANodeMadeInAHelperDiesWithIt()
{
    MakeAndLeave();
    new v;
    T_Is(JSON_GetInt(gSeen, "v", v), JSON_ERR_NODE);
}

Lifetime_UpHandsANodeToTheCaller()
{
    new Node:n = Middle();
    T_Is(Read(n), 4);
    gSeen = n;
}

Lifetime_TheHandedNodeDiesWithItsNewScope()
{
    T_Is(Read(gSeen), -1);
}

Lifetime_AKeptNodeOutlivesItsScope()
{
    KeepOne();
    T_Is(Read(gSeen), 5);
    T_Is(JSON_Release(gSeen), JSON_OK);
    T_Is(Read(gSeen), -1);
}

Lifetime_ANodeInAnInnerBlockDiesWithTheBlock()
{
    {
        new Node:m = Make(6);
        gSeen = m;
        T_Is(Read(gSeen), 6);
    }
    T_Is(Read(gSeen), -1);
}

Lifetime_EachRecursionLevelKeepsItsOwn()
{
    new before = JSON_CountNodes();
    T_True(Recurse(8));
    T_Is(JSON_CountNodes(), before);
}

Lifetime_AnUnloadedScriptFreesItsHandles()
{
    new before = JSON_CountNodes();
    SendRconCommand("loadfs lifetimefs");
    T_Is(JSON_CountNodes(), before + 10);
    SendRconCommand("unloadfs lifetimefs");
    T_Is(JSON_CountNodes(), before);
}

main()
{
}

forward OnGameModeInit();
public OnGameModeInit()
{
    new before = JSON_CountNodes();
    Lifetime_AHelperLeavesItsArgumentAlive();
    Lifetime_AHookLeavesItsArgumentAlive();
    Lifetime_ANodeMadeInAHelperDiesWithIt();
    Lifetime_UpHandsANodeToTheCaller();
    Lifetime_TheHandedNodeDiesWithItsNewScope();
    Lifetime_AKeptNodeOutlivesItsScope();
    Lifetime_ANodeInAnInnerBlockDiesWithTheBlock();
    Lifetime_EachRecursionLevelKeepsItsOwn();
    Lifetime_AnUnloadedScriptFreesItsHandles();
    T_Is(JSON_CountNodes(), before);
    T_Done();
    return 1;
}
