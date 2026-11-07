// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

// The mistakes scripts tend to make. None of them may crash the server, hang it, or
// quietly reach a node other than the one meant: each gets an error code back, and the
// server is still alive at the end to print the totals.

#include "test"
#include <json>

main()
{
}

new Node:gForgotten, Node:gKept;

// A loop guard: a walk that runs this many steps has not stopped by itself.
const RUNAWAY = 100;

static StoreWithoutKeep()
{
    new Node:n = JSON_Object();
    JSON_SetInt(n, "a", 1);
    gForgotten = n;
}

static StoreWithKeep()
{
    new Node:n = JSON_Object();
    JSON_SetInt(n, "a", 1);
    JSON_Keep(n);
    gKept = n;
}

static Node:ReturnWithoutUp()
{
    new Node:n = JSON_Object();
    JSON_SetInt(n, "a", 1);
    return n;
}

static Node:ReturnWithUp()
{
    new Node:n = JSON_Object();
    JSON_SetInt(n, "a", 1);
    return JSON_Up(n);
}

static ParseInALoop()
{
    new Node:n;
    for (new i = 0; i < 1000; ++i)
    {
        JSON_Parse("{\"a\":[1,2,3]}", n);
    }
}

static ReadIntoOneVariable()
{
    new Node:root, Node:v;
    JSON_Parse("{\"list\":[{\"pos\":[1]},{\"pos\":[2]}],\"a\":{\"b\":{}}}", root);
    for (new i = 0; i < 1000; ++i)
    {
        JSON_GetNode(root, "a", v);
        JSON_PathGetNode(root, "/a/b", v);
        JSON_PathGetNode(root, "/list", v);
        JSON_ArrayGetNode(v, i % 2, v);
    }
}

static ParseFileIntoOneVariable()
{
    new Node:n;
    JSON_Parse("{\"a\":1}", n);
    JSON_SaveFile("scriptfiles/mistakes-load.json", n);
    for (new i = 0; i < 200; ++i)
    {
        JSON_ParseFile("scriptfiles/mistakes-load.json", n);
    }
}

static MakeLots()
{
    for (new i = 0; i < 100000; ++i)
    {
        new Node:n = JSON_Object();
        JSON_SetInt(n, "i", i);
    }
}

// Handles that are not, or no longer, nodes.
static Handles()
{
    new Node:none, Node:row, value;
    // A variable that was never given a node.
    T_Is(JSON_GetInt(none, "a", value), JSON_ERR_NODE);
    T_Is(JSON_SetInt(none, "a", 1), JSON_ERR_NODE);
    T_Is(JSON_AppendInt(none, 1), JSON_ERR_NODE);
    T_Is(JSON_Count(none, value), JSON_ERR_NODE);
    T_Is(JSON_TypeOf(none), JSON_MISSING);
    T_True(!JSON_Iter(none, row));
    T_Is(JSON_Release(none), JSON_ERR_NODE);
    // Numbers that were never handles at all.
    T_Is(JSON_GetInt(Node:(-1), "a", value), JSON_ERR_NODE);
    T_Is(JSON_GetInt(Node:cellmax, "a", value), JSON_ERR_NODE);
    T_Is(JSON_GetInt(Node:cellmin, "a", value), JSON_ERR_NODE);
    T_Is(JSON_GetInt(Node:123456, "a", value), JSON_ERR_NODE);

    // Using a node after releasing it, and releasing it twice.
    new Node:a, Node:b;
    JSON_Parse("{\"a\":1}", a);
    T_Is(JSON_Release(a), JSON_OK);
    T_Is(JSON_GetInt(a, "a", value), JSON_ERR_NODE);
    T_Is(JSON_Release(a), JSON_ERR_NODE);

    // The next node made gets a different handle, and the old one still can't reach it.
    JSON_Parse("{\"a\":2}", b);
    T_True(a != b);
    T_Is(JSON_GetInt(a, "a", value), JSON_ERR_NODE);
    T_Is(JSON_GetInt(b, "a", value), JSON_OK);
    T_Is(value, 2);
}

// Nodes kept past the function that made them, or not.
static Lifetimes()
{
    new value;
    // A local node put in a global without JSON_Keep is gone once the function returns.
    StoreWithoutKeep();
    T_Is(JSON_GetInt(gForgotten, "a", value), JSON_ERR_NODE);
    StoreWithKeep();
    T_Is(JSON_GetInt(gKept, "a", value), JSON_OK);
    T_Is(JSON_Release(gKept), JSON_OK);

    // Returning a node without JSON_Up hands back a dead one.
    new Node:lost = ReturnWithoutUp();
    T_Is(JSON_GetInt(lost, "a", value), JSON_ERR_NODE);
    new Node:kept = ReturnWithUp();
    T_Is(JSON_GetInt(kept, "a", value), JSON_OK);

    // Parsing into one variable over and over, or making a hundred thousand nodes in a
    // loop: they all go when the function returns.
    new before = JSON_CountNodes();
    ParseInALoop();
    T_Is(JSON_CountNodes(), before);
    MakeLots();
    T_Is(JSON_CountNodes(), before);
    // Reading children, or a file, into one variable over and over.
    ReadIntoOneVariable();
    T_Is(JSON_CountNodes(), before);
    ParseFileIntoOneVariable();
    T_Is(JSON_CountNodes(), before);
}

// A fresh node set into another is used up, so changing it afterwards does nothing.
static UsedUp()
{
    new Node:obj = JSON_Object(), Node:list = JSON_Array(), Node:inside, value, text[32];
    T_Is(JSON_SetNode(obj, "list", list), JSON_OK);
    T_Is(JSON_AppendInt(list, 1), JSON_ERR_NODE);
    T_Is(JSON_GetNode(obj, "list", inside), JSON_OK);
    T_Is(JSON_Count(inside, value), JSON_OK);
    T_Is(value, 0);
    // The way that works: take the child back out and change that.
    T_Is(JSON_AppendInt(inside, 1), JSON_OK);
    JSON_Stringify(obj, text);
    T_True(T_Same(text, "{\"list\":[1]}"));
}

// A node set into itself, or into one of its own children, is copied first, so no loop
// can form and writing it out still ends.
static IntoItself()
{
    new Node:obj, Node:arr, Node:root, Node:child, text[64];
    JSON_Parse("{\"a\":1}", obj);
    T_Is(JSON_SetNode(obj, "me", obj), JSON_OK);
    T_Is(JSON_Stringify(obj, text), JSON_OK);
    T_True(T_Same(text, "{\"a\":1,\"me\":{\"a\":1}}"));

    JSON_Parse("[1]", arr);
    T_Is(JSON_AppendNode(arr, arr), JSON_OK);
    JSON_Stringify(arr, text);
    T_True(T_Same(text, "[1,[1]]"));
    T_Is(JSON_ArraySetNode(arr, 0, arr), JSON_OK);
    JSON_Stringify(arr, text);
    T_True(T_Same(text, "[[1,[1]],[1]]"));

    JSON_Parse("{\"c\":{}}", root);
    JSON_GetNode(root, "c", child);
    T_Is(JSON_SetNode(child, "up", root), JSON_OK);
    JSON_Stringify(root, text);
    T_True(T_Same(text, "{\"c\":{\"up\":{\"c\":{}}}}"));
}

// A view whose node was removed or replaced must not read whatever took its place.
static StaleViews()
{
    new Node:root, Node:a, value;
    JSON_Parse("{\"a\":{\"x\":1},\"b\":2}", root);
    JSON_GetNode(root, "a", a);
    T_Is(JSON_Remove(root, "a"), JSON_OK);
    T_Is(JSON_GetInt(a, "x", value), JSON_ERR_NODE);
    JSON_Parse("{\"a\":{\"x\":1}}", root);
    JSON_GetNode(root, "a", a);
    JSON_SetInt(root, "a", 5);
    T_Is(JSON_GetInt(a, "x", value), JSON_ERR_NODE);
    T_Is(JSON_ValueInt(a, value), JSON_ERR_NODE);

    // Releasing the root while a child view is still held: the child stays readable.
    new Node:r, Node:c;
    JSON_Parse("{\"c\":{\"x\":7}}", r);
    JSON_GetNode(r, "c", c);
    T_Is(JSON_Release(r), JSON_OK);
    T_Is(JSON_GetInt(c, "x", value), JSON_OK);
    T_Is(value, 7);
}

// Buffers that are too small, sizes that lie, and keys that are odd.
static Buffers()
{
    new Node:row, small[4], big[16], value;
    JSON_Parse("{\"name\":\"Infernus\",\"n\":1}", row);
    T_Is(JSON_GetString(row, "name", small), JSON_ERR_BUFFER);
    T_True(T_Same(small, "Inf"));
    T_Is(JSON_Stringify(row, small), JSON_ERR_BUFFER);
    T_Is(small[3], 0);

    // A size smaller than the array: nothing past it is touched.
    for (new i = 0; i < sizeof (big); ++i)
    {
        big[i] = 'Z';
    }
    T_Is(JSON_GetString(row, "name", big, 4), JSON_ERR_BUFFER);
    T_Is(big[3], 0);
    T_Is(big[4], 'Z');
    T_Is(big[15], 'Z');
    // A size far beyond the script's memory, none at all, or below zero.
    T_Is(JSON_GetString(row, "name", big, 100000000), JSON_ERR_ADDRESS);
    T_Is(JSON_GetString(row, "name", big, 0), JSON_ERR_ADDRESS);
    T_Is(JSON_GetString(row, "name", big, -5), JSON_ERR_ADDRESS);
    T_Is(JSON_Stringify(row, big, 100000000), JSON_ERR_ADDRESS);
    T_Is(big[4], 'Z');
    // SetString reads up to the end of the string, whatever length it is told.
    T_Is(JSON_SetString(row, "s", "abc", 100000000), JSON_OK);
    T_Is(JSON_GetString(row, "s", big), JSON_OK);
    T_True(T_Same(big, "abc"));

    // A packed string where a key is expected.
    T_Is(JSON_GetInt(row, !"n", value), JSON_OK);
    T_Is(value, 1);
    // An empty key, and a key full of quotes and backslashes.
    T_Is(JSON_SetInt(row, "", 3), JSON_OK);
    T_Is(JSON_GetInt(row, "", value), JSON_OK);
    T_Is(value, 3);
    new Node:o = JSON_Object(), Node:back, text[48], s[8];
    JSON_SetString(o, "a\"b\\c", "x\ny");
    T_Is(JSON_Stringify(o, text), JSON_OK);
    T_True(T_Same(text, "{\"a\\\"b\\\\c\":\"x\\ny\"}"));
    T_Is(JSON_Parse(text, back), JSON_OK);
    T_Is(JSON_GetString(back, "a\"b\\c", s), JSON_OK);
    T_True(T_Same(s, "x\ny"));

    // An unknown code page is refused, and reading goes on as before.
    T_True(!JSON_SetCodepage("klingon"));
    T_True(!JSON_SetCodepage(""));
    T_Is(JSON_GetString(row, "name", big), JSON_OK);
    T_True(T_Same(big, "Infernus"));
}

// Text that isn't JSON, or is JSON at its edges.
static BadText()
{
    new Node:n = Node:7, value, Float:f;
    T_Is(JSON_Parse("", n), JSON_ERR_SYNTAX);
    T_Is(_:n, 0);
    T_Is(JSON_Parse("   ", n), JSON_ERR_SYNTAX);
    T_Is(JSON_Parse("{", n), JSON_ERR_SYNTAX);
    T_Is(JSON_Parse("[1,]", n), JSON_ERR_SYNTAX);
    T_Is(JSON_Parse("{\"a\":1,}", n), JSON_ERR_SYNTAX);
    T_Is(JSON_Parse("{'a':1}", n), JSON_ERR_SYNTAX);
    T_Is(JSON_Parse("{a:1}", n), JSON_ERR_SYNTAX);
    T_Is(JSON_Parse("// a note\n{}", n), JSON_ERR_SYNTAX);
    T_Is(JSON_Parse("NaN", n), JSON_ERR_SYNTAX);
    T_Is(JSON_Parse("[1] x", n), JSON_ERR_SYNTAX);
    T_Is(JSON_Parse("[1 2]", n), JSON_ERR_SYNTAX);
    T_Is(JSON_Parse("\"no end", n), JSON_ERR_SYNTAX);
    T_Is(JSON_Parse("tru", n), JSON_ERR_SYNTAX);
    T_Is(JSON_Parse("1e400", n), JSON_ERR_SYNTAX);
    T_Is(_:n, 0);

    // Nested deeper than the parser allows fails cleanly; a reasonable depth is fine.
    new deep[402];
    for (new i = 0; i < 200; ++i)
    {
        deep[i] = '[';
        deep[200 + i] = ']';
    }
    T_Is(JSON_Parse(deep, n), JSON_ERR_SYNTAX);
    for (new i = 0; i < 100; ++i)
    {
        deep[i] = '[';
        deep[100 + i] = ']';
    }
    deep[200] = 0;
    T_Is(JSON_Parse(deep, n), JSON_OK);

    // Numbers a Pawn cell can't hold, and a number read as the wrong kind.
    JSON_Parse("[123456789012345678901234567890, 4294967296, 1.0, 5]", n);
    T_Is(JSON_ArrayGetInt(n, 0, value), JSON_ERR_TYPE);
    T_Is(JSON_ArrayGetFloat(n, 0, f), JSON_OK);
    T_Is(JSON_ArrayGetInt(n, 1, value), JSON_ERR_RANGE);
    T_Is(JSON_ArrayGetInt(n, 2, value), JSON_ERR_TYPE);
    T_Is(JSON_ArrayGetFloat(n, 3, f), JSON_OK);
    T_True(f == 5.0);

    // A key given twice: the last one counts.
    JSON_Parse("{\"a\":1,\"a\":2}", n);
    T_Is(JSON_GetInt(n, "a", value), JSON_OK);
    T_Is(value, 2);
}

// Reading and writing a node as a kind it isn't.
static WrongKinds()
{
    new Node:arr, Node:obj, Node:num, value, k[8];
    JSON_Parse("[1,2]", arr);
    JSON_Parse("{\"n\":5,\"s\":\"x\"}", obj);
    T_Is(JSON_SetInt(arr, "a", 1), JSON_ERR_CONTAINER);
    T_Is(JSON_GetInt(arr, "a", value), JSON_ERR_CONTAINER);
    T_Is(JSON_AppendInt(obj, 1), JSON_ERR_CONTAINER);
    T_Is(JSON_ArrayGetInt(obj, 0, value), JSON_ERR_CONTAINER);
    T_Is(JSON_ArraySetInt(arr, 2, 9), JSON_ERR_KEY);
    T_Is(JSON_ArraySetInt(arr, -1, 9), JSON_ERR_KEY);
    T_Is(JSON_ArrayRemoveAt(arr, 5), JSON_ERR_KEY);
    T_Is(JSON_ArrayGetInt(arr, cellmax, value), JSON_ERR_KEY);
    T_Is(JSON_ArrayGetInt(arr, cellmin, value), JSON_ERR_KEY);
    T_Is(JSON_Remove(obj, "missing"), JSON_ERR_KEY);
    T_Is(JSON_GetInt(obj, "s", value), JSON_ERR_TYPE);
    T_Is(JSON_GetNode(obj, "n", num), JSON_OK);
    T_Is(JSON_SetInt(num, "x", 1), JSON_ERR_CONTAINER);
    T_Is(JSON_AppendInt(num, 1), JSON_ERR_CONTAINER);
    T_Is(JSON_KeyAt(arr, 0, k), JSON_ERR_CONTAINER);
    T_Is(JSON_KeyAt(obj, 99, k), JSON_ERR_KEY);
    T_Is(JSON_KeyAt(obj, -1, k), JSON_ERR_KEY);
    // The array and the object are as they were.
    T_Is(JSON_Count(arr, value), JSON_OK);
    T_Is(value, 2);
    T_Is(JSON_Count(obj, value), JSON_OK);
    T_Is(value, 2);
}

// Paths written the way people guess they work.
static Paths()
{
    new Node:root, value;
    JSON_Parse("{\"list\":[10,20],\"name\":\"x\",\"a/b\":1,\"m~n\":2}", root);
    T_Is(JSON_PathGetInt(root, "list/0", value), JSON_ERR_PATH);
    T_Is(JSON_PathGetInt(root, "/list/01", value), JSON_ERR_PATH);
    T_Is(JSON_PathGetInt(root, "/list/-1", value), JSON_ERR_PATH);
    T_Is(JSON_PathGetInt(root, "/list/2", value), JSON_ERR_KEY);
    T_Is(JSON_PathGetInt(root, "/list/99999999999", value), JSON_ERR_KEY);
    T_Is(JSON_PathGetInt(root, "/name/x", value), JSON_ERR_CONTAINER);
    T_Is(JSON_PathGetInt(root, "/a/b", value), JSON_ERR_KEY);
    T_Is(JSON_PathGetInt(root, "/a~1b", value), JSON_OK);
    T_Is(value, 1);
    T_Is(JSON_PathGetInt(root, "/m~0n", value), JSON_OK);
    T_Is(value, 2);
    T_Is(JSON_PathGetInt(root, "/m~2n", value), JSON_ERR_PATH);
    T_Is(JSON_PathGetInt(root, "", value), JSON_ERR_TYPE);
}

// Loops that change what they walk, stop early, or share a variable.
static Loops()
{
    new Node:list, Node:row, Node:other, Node:root, Node:inner, steps, value;

    // Adding to the array being walked must not go on forever.
    JSON_Parse("[1,2,3]", list);
    steps = 0;
    while (JSON_Iter(list, row))
    {
        JSON_AppendInt(list, 0);
        if (++steps > RUNAWAY)
        {
            break;
        }
    }
    T_Is(steps, 3);

    // The same for an object that gains a key every step.
    new key[8];
    JSON_Parse("{\"a\":1}", other);
    steps = 0;
    while (JSON_Iter(other, row))
    {
        format(key, sizeof (key), "k%d", steps);
        JSON_SetInt(other, key, steps);
        if (++steps > RUNAWAY)
        {
            break;
        }
    }
    T_Is(steps, 1);

    // Removing from the array being walked.
    JSON_Parse("[1,2,3,4,5]", list);
    steps = 0;
    while (JSON_Iter(list, row))
    {
        JSON_ArrayRemoveAt(list, 0);
        if (++steps > RUNAWAY)
        {
            break;
        }
    }
    T_True(steps <= 5);

    // Replacing the array being walked ends the walk.
    JSON_Parse("{\"list\":[1,2,3]}", root);
    JSON_GetNode(root, "list", list);
    steps = 0;
    while (JSON_Iter(list, row))
    {
        JSON_SetInt(root, "list", 0);
        if (++steps > RUNAWAY)
        {
            break;
        }
    }
    T_Is(steps, 1);

    // Leaving a loop early, then walking another array with the same variable: the new
    // walk starts at its own beginning. Setting the variable to 0 after the break makes
    // the next loop a fresh one, which leaves the variable at 0 when it ends.
    JSON_Parse("[1,2,3]", list);
    JSON_Parse("[7,8]", other);
    T_True(JSON_Iter(list, row));
    T_True(JSON_Iter(other, row));
    T_Is(JSON_ValueInt(row, value), JSON_OK);
    T_Is(value, 7);
    row = Node:0;
    steps = 0;
    while (JSON_Iter(other, row))
    {
        if (++steps > RUNAWAY)
        {
            break;
        }
    }
    T_Is(steps, 2);
    T_Is(_:row, 0);

    // Copying the element into a variable of the loop's own block: the copy is freed at
    // the end of each pass, and the loop must not start over because of it.
    JSON_Parse("[1,2,3]", list);
    steps = 0;
    while (JSON_Iter(list, row))
    {
        new Node:item = row;
        T_Is(JSON_ValueInt(item, value), JSON_OK);
        if (++steps > RUNAWAY)
        {
            break;
        }
    }
    T_Is(steps, 3);

    // Releasing the element inside the loop, as 1.x scripts did with JSON_Cleanup.
    steps = 0;
    while (JSON_Iter(list, row))
    {
        JSON_Release(row);
        if (++steps > RUNAWAY)
        {
            break;
        }
    }
    T_Is(steps, 3);
    steps = 0;
    while (JSON_Iter(list, row))
    {
        JSON_Cleanup(row);
        if (++steps > RUNAWAY)
        {
            break;
        }
    }
    T_Is(steps, 3);

    // An inner loop that reuses the outer loop's variable. Both loops run in full, and
    // after the inner one the variable holds the outer element again.
    JSON_Parse("[{\"n\":10,\"items\":[1,2]},{\"n\":20,\"items\":[3]}]", list);
    new outer = 0, sum = 0, Node:items, Node:cur;
    while (JSON_Iter(list, cur))
    {
        JSON_GetNode(cur, "items", items);
        while (JSON_Iter(items, cur))
        {
            JSON_ValueInt(cur, value);
            sum += value;
            if (++steps > RUNAWAY)
            {
                break;
            }
        }
        T_Is(JSON_GetInt(cur, "n", value), JSON_OK);
        sum += value;
        if (++outer > RUNAWAY)
        {
            break;
        }
    }
    T_Is(outer, 2);
    T_Is(sum, 36);
    T_Is(_:cur, 0);

    // The same with a copy of the outer element as the inner loop's container.
    JSON_Parse("[[1,2],[3,4]]", list);
    steps = 0;
    while (JSON_Iter(list, row))
    {
        inner = row;
        while (JSON_Iter(inner, row))
        {
            if (++steps > RUNAWAY)
            {
                break;
            }
        }
        if (++steps > RUNAWAY)
        {
            break;
        }
    }
    T_Is(steps, 6);

    // A loop over a container walked into its own variable doesn't start.
    JSON_Parse("[1,2]", list);
    T_True(!JSON_Iter(list, list));
    T_Is(JSON_Count(list, value), JSON_OK);
    T_Is(value, 2);

    // Walking something that isn't a container, or isn't there.
    JSON_Parse("{\"n\":1}", other);
    JSON_GetNode(other, "n", inner);
    T_True(!JSON_Iter(inner, row));
    T_True(!JSON_Iter(Node:123456, row));

    // An inner loop over the very container the outer loop walks, on the same variable,
    // can't be told from the outer loop's next step, and would go round for ever. The
    // plugin stops it once the variable has started over on that container too often in
    // one tick, and logs it. The variable is one of its own, since it has no steps left
    // on that container for the rest of the tick.
    new Node:runaway, cycles = 0;
    JSON_Parse("[1,2,3]", list);
    while (JSON_Iter(list, runaway))
    {
        while (JSON_Iter(list, runaway))
        {
        }
        if (++cycles > 50000000)
        {
            break;
        }
    }
    T_True(cycles < 50000000);
}

// Files that aren't there, aren't files, or are written over and over.
static Files()
{
    new Node:n, Node:back, Node:gone, value;
    JSON_Parse("{\"a\":1}", n);
    T_Is(JSON_ParseFile("scriptfiles", back), JSON_ERR_FILE);
    T_Is(JSON_ParseFile("", back), JSON_ERR_FILE);
    // tools/server-test.py leaves an empty file here.
    T_Is(JSON_ParseFile("scriptfiles/empty.json", back), JSON_ERR_SYNTAX);
    T_Is(_:back, 0);
    // No file name at all.
    T_Is(JSON_SaveFile("", n), JSON_ERR_FILE);
    T_Is(JSON_SaveFile("scriptfiles/", n), JSON_ERR_FILE);
    T_Is(JSON_Flush(), JSON_OK);

    // Saving a released node fails, and the file keeps what it had.
    T_Is(JSON_SaveFile("scriptfiles/mistakes.json", n), JSON_OK);
    JSON_Parse("[0]", gone);
    JSON_Release(gone);
    T_Is(JSON_SaveFile("scriptfiles/mistakes.json", gone), JSON_ERR_NODE);
    T_Is(JSON_ParseFile("scriptfiles/mistakes.json", back), JSON_OK);
    T_Is(JSON_GetInt(back, "a", value), JSON_OK);
    T_Is(value, 1);

    // Saving one file five hundred times in a row.
    for (new i = 0; i < 500; ++i)
    {
        JSON_SetInt(n, "a", i);
        JSON_SaveFile("scriptfiles/mistakes.json", n);
    }
    T_Is(JSON_ParseFile("scriptfiles/mistakes.json", back), JSON_OK);
    T_Is(JSON_GetInt(back, "a", value), JSON_OK);
    T_Is(value, 499);

    // Saving a document over the file it was read from.
    JSON_SetInt(back, "b", 2);
    T_Is(JSON_SaveFile("scriptfiles/mistakes.json", back), JSON_OK);
    T_Is(JSON_ParseFile("scriptfiles/mistakes.json", n), JSON_OK);
    T_Is(JSON_GetInt(n, "b", value), JSON_OK);
    T_Is(value, 2);
}

// Formats that don't match their variables.
static Formats()
{
    new Node:row, model, Float:x, name[8];
    JSON_Parse("{\"model\":411,\"x\":1.5,\"name\":\"Infernus\"}", row);
    T_Is(JSON_Unpack(row, "{model:i, x:f}", model), JSON_ERR_FORMAT);
    T_Is(JSON_Unpack(row, "{model:i, name:s}", model, name), JSON_ERR_FORMAT);
    T_Is(JSON_Unpack(row, "{model:i", model), JSON_ERR_FORMAT);
    T_Is(JSON_Unpack(row, "{model:i, name:s[8]}", model, name), JSON_ERR_BUFFER);
    T_Is(JSON_Unpack(row, "{name:s[100000000]}", name), JSON_ERR_ADDRESS);
    T_Is(JSON_Unpack(row, "{missing:i}", model), JSON_ERR_KEY);
    T_Is(JSON_Unpack(row, "{name:i}", model), JSON_ERR_TYPE);
    T_Is(JSON_Unpack(row, "{x:f}", x), JSON_OK);
    T_True(x == 1.5);
    T_Is(_:JSON_Pack("{a:i, b:i}", 1), 0);
    T_Is(_:JSON_Pack("{a:s}"), 0);
    T_Is(_:JSON_Pack("{a:i"), 0);
}

// pawn-json 1.x's names, used the way old scripts get them wrong.
static OldNames()
{
    new Node:root, Node:out, Node:list, value;
    JSON_Parse("{\"a\":{\"x\":1},\"list\":[1,2]}", root);
    T_Is(JSON_GetObject(root, "a", out), JSON_OK);
    T_Is(JSON_Cleanup(out), JSON_OK);
    T_Is(JSON_Cleanup(out), JSON_ERR_NODE);
    T_Is(JSON_PathGetInt(root, "/a/x", value), JSON_OK);
    T_Is(JSON_ArrayObject(root, 0, out), JSON_ERR_CONTAINER);
    T_Is(JSON_GetArray(root, "list", list), JSON_OK);
    T_Is(JSON_ArrayObject(list, -1, out), JSON_ERR_KEY);
    T_Is(JSON_ArrayObject(list, 2, out), JSON_ERR_KEY);
    T_Is(JSON_GetArray(root, "a", out), JSON_ERR_TYPE);
    T_Is(JSON_ToggleGC(Node:123456, false), JSON_ERR_NODE);
    T_Is(JSON_Cleanup(Node:123456), JSON_ERR_NODE);

    // A node handed to the old builder belongs to it now.
    new Node:five = JSON_Int(5);
    new Node:obj = JSON_Object("k", five);
    T_True(obj != Node:0);
    T_Is(JSON_ValueInt(five, value), JSON_ERR_NODE);
    // The builder given a key with no node.
    T_Is(_:JSON_Object("k"), 0);
}

forward OnGameModeInit();
public OnGameModeInit()
{
    Handles();
    Lifetimes();
    UsedUp();
    IntoItself();
    StaleViews();
    Buffers();
    BadText();
    WrongKinds();
    Paths();
    Loops();
    Files();
    Formats();
    OldNames();
    T_Done();
    return 1;
}
