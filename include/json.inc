/**
 * <library name="json" summary="JSON for open.mp scripts: parse, read, build, walk and save JSON documents.">
 *   <license>
 *     This Source Code Form is subject to the terms of the Mozilla Public License,
 *     v. 2.0. If a copy of the MPL was not distributed with this file, You can
 *     obtain one at http://mozilla.org/MPL/2.0/.
 *
 *     The original code is copyright (c) rasheed.
 *   </license>
 *   <summary pawndoc="true">
 *     This library uses the enhanced <em>pawndoc.xsl</em> from
 *     <a href="https://github.com/pawn-lang/pawndoc">pawn-lang/pawndoc</a>.
 *     This XSL has features such as library and markdown support, and will not
 *     render this message when used.
 *   </summary>
 *   <remarks>
 *     Everything that reads or writes returns <c>JSON_OK</c> (0) or one of the
 *     <c>JSON_ERR_*</c> codes below. Functions that create a node return it instead,
 *     or 0 if they couldn't. A handle to a node that has since been freed gives
 *     <c>JSON_ERR_NODE</c> - it can never quietly point at some other node.
 *
 *     Reading a child doesn't copy it: you get a view into the same document, so
 *     writing through it changes the parent too, and nothing is ever freed just
 *     because you read it.
 *
 *     You rarely need to free anything by hand. When a <c>Node:</c> variable goes out
 *     of scope, its node is freed if it was made in that function, or in a function
 *     it called. A node you pass down to a helper (or to a hook through
 *     <c>CallLocalFunction</c>) is left alone, and a node a helper returns is freed
 *     when the caller is done with it (from a <c>Node:</c> variable, return it with
 *     <c>JSON_Up</c>). If you need a node to outlive every scope -
 *     in a global, or for a timer or an inline function that runs later - call
 *     <c>JSON_Keep(node)</c>, and <c>JSON_Release(node)</c> once you're finished.
 *     PawnPlus tasks resume in the frame they waited in, so nodes survive an
 *     <c>await</c>; a YSI inline function has its own frame, so nodes made inside
 *     it are freed when it ends.
 *
 *     Strings use the script's code page: <c>json.codepage</c> in
 *     <c>config.json</c>, <c>windows-1252</c> if that isn't set, or whatever the
 *     script picks with <c>JSON_SetCodepage</c>. A character the code page can't
 *     hold comes out as <c>?</c>.
 *
 *     The pawn-json 1.x functions are at the end of this file, so old scripts keep
 *     compiling. They return the same codes as everything else; 0 still means it
 *     worked.
 *   </remarks>
 * </library>
 */

#if defined _pawn_json_included
    #endinput
#endif
#define _pawn_json_included

// Claim pawn-json 1.x's include guards too, so an old copy of json.inc somewhere else
// on the include path doesn't get pulled in as well.
#if defined _inc_json
    #undef _inc_json
#endif
#define _json_included

#define JSON_VERSION "0.2.0"

const JSON_OK = 0;
const JSON_ERR_NODE = 1;
const JSON_ERR_CONTAINER = 2;
const JSON_ERR_KEY = 3;
const JSON_ERR_TYPE = 4;
const JSON_ERR_BUFFER = 5;
const JSON_ERR_RANGE = 6;
const JSON_ERR_SYNTAX = 7;
const JSON_ERR_FILE = 8;
const JSON_ERR_PATH = 9;
const JSON_ERR_FORMAT = 10;
const JSON_ERR_ADDRESS = 11;
// The call ran into a bug in the plugin and was stopped there, so the server carries
// on. The server log says where; please report it.
const JSON_ERR_INTERNAL = 12;

const JSON_MISSING = 0;
const JSON_NULL = 1;
const JSON_BOOL = 2;
const JSON_INT = 3;
const JSON_FLOAT = 4;
const JSON_STRING = 5;
const JSON_ARRAY = 6;
const JSON_OBJECT = 7;

// Creating documents. Each of these returns the root of a new document, or 0.

// With no arguments, an empty object. With arguments it works like 1.x's builder,
// taking key and node pairs and using up the nodes you pass:
// JSON_Object("name", JSON_String("Zsofi"), "list", JSON_Array(JSON_Int(1)))
native Node:JSON_Object({_, Node}:...);

// An empty array, or 1.x's builder when you pass it nodes.
native Node:JSON_Array(Node:...);

// A deep copy of the node, as a separate document.
native Node:JSON_Copy(Node:node);

// Text and files. Paths are relative to the server's folder, as they were in 1.x.

// If the text isn't valid JSON, `node` is set to 0, the call returns JSON_ERR_SYNTAX and
// the server log says what was wrong.
native JSON_Parse(const text[], &Node:node);
native JSON_ParseFile(const path[], &Node:node);

// Saving writes to a temporary file first and then swaps it in, so if the server dies
// halfway through, the old file is still there in one piece.
//
// Waiting for the disk is most of a save's time, so that part runs in the background:
// JSON_SaveFile returns as soon as the text is in the temporary file, and the swap
// follows a few milliseconds later. A path that can't be written to still answers
// JSON_ERR_FILE right away; a failure later on goes to the server log. JSON_ParseFile
// always waits for pending saves, so it reads what you just saved, and the server waits
// for them before it shuts down. If the server crashes in between, that one save is lost
// and the old file stays as it was.
native JSON_SaveFile(const path[], Node:node, bool:pretty = false);

// Waits until every save so far is on the disk. You only need this before reading a
// saved file some other way than JSON_ParseFile, for example with fopen. Answers
// JSON_ERR_FILE if one of them could not be finished; the server log says which.
native JSON_Flush();

// If the buffer is too small you get as much as fits, and JSON_ERR_BUFFER.
native JSON_Stringify(Node:node, output[], size = sizeof (output), bool:pretty = false);

// Sets the code page this script uses, by name ("windows-1250"). Returns false if the
// name isn't a single-byte code page we know.
native bool:JSON_SetCodepage(const name[]);

// Reading values by key. If a read fails, the variable keeps the value it had.

// A whole number that doesn't fit in 32 bits gives JSON_ERR_RANGE.
native JSON_GetInt(Node:node, const key[], &value);
native JSON_GetFloat(Node:node, const key[], &Float:value);
native JSON_GetBool(Node:node, const key[], &bool:value);
native JSON_GetString(Node:node, const key[], output[], size = sizeof (output));
native JSON_GetNode(Node:node, const key[], &Node:child);

// Reading a child into a variable that already holds one of your nodes replaces it. A
// child from an earlier read is moved to the new one, so reading in a loop uses a single
// handle, and anything else the variable held is released, unless you kept it. The
// same goes for JSON_PathGetNode, JSON_ArrayGetNode, JSON_Parse and JSON_ParseFile. So a
// copy of the handle in another variable stops working: JSON_Keep the node to hold on
// to it.

// The type stored under `key`, or of the node itself if `key` is empty. A missing key
// or a freed node gives JSON_MISSING.
native JSON_TypeOf(Node:node, const key[] = "");

// How many elements an array has, or how many keys an object has.
native JSON_Count(Node:node, &count);

// The key at `index`, counting in the order the keys appear in the file.
native JSON_KeyAt(Node:node, index, output[], size = sizeof (output));

// False for a missing key, and also when the node isn't an object or has been freed.
native bool:JSON_HasKey(Node:node, const key[]);

// Reading a value deep in a tree in one go, with an RFC 6901 path such as
// "/vehicles/3/model". Inside a path, write a key's `/` as `~1` and its `~` as `~0`.
// A malformed path gives JSON_ERR_PATH.

native JSON_PathGetInt(Node:node, const path[], &value);
native JSON_PathGetFloat(Node:node, const path[], &Float:value);
native JSON_PathGetBool(Node:node, const path[], &bool:value);
native JSON_PathGetString(Node:node, const path[], output[], size = sizeof (output));
native JSON_PathGetNode(Node:node, const path[], &Node:child);

// Reading a whole row in one call, sscanf-style. The format mirrors the JSON: `{key:t, ...}`
// for objects and `[t, ...]` for arrays, nested as deep as you like, where t is
// i (int), f (Float), b (bool), s[size] (a string, with the size of your buffer),
// n (null, takes no variable) or o (a node). Put a `?` after anything that may be missing
// and its variables are simply left alone:
//
//     JSON_Unpack(row, "{model:i, x:f, y:f, z:f, name:s[32], note:s[64]?}", model, x, y, z, name, note);
//
// It stops at the first problem and returns its code; the variables after that point
// aren't touched.
native JSON_Unpack(Node:node, const format[], {Float, bool, Node, _}:...);

// The other direction: build a tree from a format. Same letters, except a string is
// just s:
//
//     new Node:row = JSON_Pack("{name:s, age:i, pos:[f, f, f]}", name, age, x, y, z);
//
// A node passed for o is put in the same way JSON_SetNode would.
native Node:JSON_Pack(const format[], {Float, bool, Node, _}:...);

// Reading an array element by index. These don't create a handle for the element.

native JSON_ArrayGetInt(Node:array, index, &value);
native JSON_ArrayGetFloat(Node:array, index, &Float:value);
native JSON_ArrayGetBool(Node:array, index, &bool:value);
native JSON_ArrayGetString(Node:array, index, output[], size = sizeof (output));
native JSON_ArrayGetNode(Node:array, index, &Node:element);

// Reading the value of a node itself, say an element you got from a loop.

native JSON_ValueInt(Node:node, &value);
native JSON_ValueFloat(Node:node, &Float:value);
native JSON_ValueBool(Node:node, &bool:value);
native JSON_ValueString(Node:node, output[], size = sizeof (output));

// Stepping through by index, if you want the index yourself: start `cursor` at 0, and
// you get JSON_ERR_KEY once you're past the end. The same `element` handle is reused
// for every step, unless you kept it. Most of the time JSON_Iter below is simpler.

native JSON_Next(Node:container, &cursor, &Node:element);
native JSON_NextKey(Node:object, &cursor, key[], size = sizeof (key));
native JSON_NextEntry(Node:object, &cursor, &Node:value, key[], size = sizeof (key));

// Changing documents. Overwriting or removing a value frees it, and any handles you
// still have onto it start returning JSON_ERR_NODE.
//
// Setting or appending a node copies its value in. If that node is the only handle to
// its own document (a fresh JSON_Array(), say), it is used up and freed, so
// JSON_SetNode(o, "list", JSON_Array()) doesn't leak - just like 1.x. A view, or a node
// whose document is still in use elsewhere, is left as it was. A node you kept with
// JSON_Keep is used up the same way: to set a node and still have it afterwards, pass
// JSON_Copy(node).

native JSON_SetInt(Node:node, const key[], value);
native JSON_SetFloat(Node:node, const key[], Float:value);
native JSON_SetBool(Node:node, const key[], bool:value);
native JSON_SetString(Node:node, const key[], const value[], len = sizeof (value));
native JSON_SetNull(Node:node, const key[]);
native JSON_SetNode(Node:node, const key[], Node:value);
native JSON_Remove(Node:node, const key[]);

native JSON_AppendInt(Node:array, value);
native JSON_AppendFloat(Node:array, Float:value);
native JSON_AppendBool(Node:array, bool:value);
native JSON_AppendString(Node:array, const value[]);
native JSON_AppendNull(Node:array);
native JSON_AppendNode(Node:array, Node:value);

// An index past the end gives JSON_ERR_KEY.
native JSON_ArraySetInt(Node:array, index, value);
native JSON_ArraySetFloat(Node:array, index, Float:value);
native JSON_ArraySetBool(Node:array, index, bool:value);
native JSON_ArraySetString(Node:array, index, const value[]);
native JSON_ArraySetNull(Node:array, index);
native JSON_ArraySetNode(Node:array, index, Node:value);
native JSON_ArrayRemoveAt(Node:array, index);

// One getter for every type. JSON_Get(node, key, var) looks at the variable you hand it
// - Float:, bool:, Node: (you get a view), a plain cell or a string - and picks the
// right read at compile time, so it is still a single native call. A variable with any
// other tag gets warning 213, naming the tag.

#define JSON@KIND(%0) (_:(tagof(%0) == tagof(Float:)) | _:(tagof(%0) == tagof(bool:)) << 1 | _:(tagof(%0) == tagof(Node:)) << 2 | _:(sizeof (%0) > 1) << 3)
#define JSON_Get(%0,%1,%2) JSON_GetAs(%0, %1, JSON@KIND(%2), sizeof (%2), %2)

native JSON_GetAs(Node:node, const key[], kind, size, {Float, bool, Node, _}:...);

// Looping, with no index to keep track of:
//
//     new Node:row;
//     while (JSON_Iter(list, row))
//     {
//         ...
//     }
//
// The plugin keeps the loop's place for the variable `row`. A loop starts when `row`
// holds anything but the element it last handed out - 0, as a new variable does - and
// when there's nothing left it frees the element and sets `row` back to 0. It goes as
// far as the length the container had when it began, so adding to the container inside
// the loop can't make it endless. Releasing `row`, or a copy of it going out of scope,
// doesn't send the loop back to the start.
//
// An inner loop may reuse the outer loop's variable: when it ends, `row` holds the outer
// element again and the outer loop carries on. That is also why a loop left with
// `break` keeps its place, and stepping it again carries on from there; set `row` to
// Node:0 after the break if the next loop should be a new one.
//
// Works on an array's elements or an object's values. A container can't be walked into
// its own variable: JSON_Iter(list, list) returns false and leaves `list` alone.
//
// Two mistakes look exactly like correct loops and would go round for ever: an inner
// loop over the same container as the outer one on the same variable, and `row = Node:0`
// inside the loop. Both keep starting over on the same container. As a last guard, once
// a variable has started over on a container in one server tick and taken five million
// more steps on it, its loops over it stop and the server log says so. A correct script
// never gets near that.
native bool:JSON_Iter(Node:container, &Node:element);

// An object's keys, in file order. This starts when `key` is empty, as a new array is,
// and empties it again at the end. Keys longer than the buffer are cut short.
native bool:JSON_IterKey(Node:object, key[], size = sizeof (key));

// Keys together with their values; `value` behaves like JSON_Iter's element.
native bool:JSON_IterEntry(Node:object, key[], &Node:value, size = sizeof (key));

// Lifetime.

// Keeps a node alive past the end of its scope, until you JSON_Release it.
native JSON_Keep(Node:node);

// Frees a node right away, whether it was kept or not.
native JSON_Release(Node:node);

// For returning a node that sits in a `Node:` variable: `return JSON_Up(node);` hands it
// to the caller, where a plain `return node;` would have the variable free it on the
// way out. Returning a call directly, like `return JSON_Object();`, needs nothing.
native Node:JSON_Up(Node:node);

// How many handles are alive right now, across all scripts. Handy for spotting leaks.
native JSON_CountNodes();

native JSON@Free(const Node:nodes[], len);

stock operator~(const Node:nodes[], len)
{
    JSON@Free(nodes, len);
}

// pawn-json 1.x's functions, so old scripts compile unchanged. They return the same
// codes as the rest of the library. A few of 1.x's quirks are fixed rather than copied:
// reading a node no longer deletes it, the type of a missing key is JSON_NODE_MISSING
// rather than JSON_NODE_NUMBER, and a child is a view instead of a copy. JSON_GetObject,
// JSON_GetArray, JSON_ArrayObject and JSON_ArrayIterate reuse the handle already in the
// output variable when they can, so a loop that keeps overwriting one variable doesn't
// pile up handles.

enum JSON_NODE {
    JSON_NODE_NUMBER,
    JSON_NODE_BOOLEAN,
    JSON_NODE_STRING,
    JSON_NODE_OBJECT,
    JSON_NODE_ARRAY,
    JSON_NODE_NULL,
    JSON_NODE_MISSING,
}

static stock JSON_NODE:JSON@Node1x(type)
{
    switch (type)
    {
        case JSON_NULL:
            return JSON_NODE_NULL;
        case JSON_BOOL:
            return JSON_NODE_BOOLEAN;
        case JSON_INT, JSON_FLOAT:
            return JSON_NODE_NUMBER;
        case JSON_STRING:
            return JSON_NODE_STRING;
        case JSON_ARRAY:
            return JSON_NODE_ARRAY;
        case JSON_OBJECT:
            return JSON_NODE_OBJECT;
    }
    return JSON_NODE_MISSING;
}

stock JSON_NODE:JSON_NodeType(Node:node)
{
    return JSON@Node1x(JSON_TypeOf(node));
}

stock JSON_NODE:JSON_GetType(Node:node, const key[])
{
    return JSON@Node1x(JSON_TypeOf(node, key));
}

stock Node:JSON_Int(value)
{
    return JSON_Pack("i", value);
}

stock Node:JSON_Bool(bool:value)
{
    return JSON_Pack("b", value);
}

stock Node:JSON_Float(Float:value)
{
    return JSON_Pack("f", value);
}

stock Node:JSON_String(const value[])
{
    return JSON_Pack("s", value);
}

// Merges two objects, or joins two arrays, into a new node. Both inputs are used up.
native Node:JSON_Append(Node:a, Node:b);
native Node:operator+(Node:a, Node:b) = JSON_Append;

native JSON_SetObject(Node:node, const key[], Node:object) = JSON_SetNode;
native JSON_SetArray(Node:node, const key[], Node:array) = JSON_SetNode;
native JSON_GetObject(Node:node, const key[], &Node:output);
native JSON_GetArray(Node:node, const key[], &Node:output);
native JSON_ArrayLength(Node:node, &length) = JSON_Count;
native JSON_ArrayObject(Node:node, index, &Node:output);

// Start `index` at -1; it is bumped before each read, and you get JSON_ERR_KEY at the end.
native JSON_ArrayIterate(Node:node, &index, &Node:output);

// Removes every element of the array under `key` that equals `input`. `input` is used
// up.
native JSON_ArrayRemove(Node:node, const key[], Node:input);

native JSON_Keys(Node:node, index, output[], len = sizeof(output)) = JSON_KeyAt;
native JSON_GetNodeInt(Node:node, &output) = JSON_ValueInt;
native JSON_GetNodeFloat(Node:node, &Float:output) = JSON_ValueFloat;
native JSON_GetNodeBool(Node:node, &bool:output) = JSON_ValueBool;
native JSON_GetNodeString(Node:node, output[], len = sizeof(output)) = JSON_ValueString;

// Turning collection off keeps a node alive the way JSON_Keep does; turning it back on
// undoes that.
native JSON_ToggleGC(Node:node, bool:toggle);

// Frees a node right away. With `auto`, a node whose collection is off is left alone.
native JSON_Cleanup(Node:node, auto = false);

// Appends `input` to the array under `key`. `input` is used up.
stock JSON_ArrayAppend(Node:node, const key[], Node:input)
{
    new Node:list, code = JSON_GetNode(node, key, list);
    if (code == JSON_OK)
    {
        code = JSON_TypeOf(list) == JSON_ARRAY ? JSON_AppendNode(list, input) : JSON_ERR_TYPE;
    }
    JSON_Release(input);
    return code;
}

stock JSON_ArrayRemoveIndex(Node:node, const key[], index)
{
    new Node:list, code = JSON_GetNode(node, key, list);
    if (code == JSON_OK)
    {
        code = JSON_TypeOf(list) == JSON_ARRAY ? JSON_ArrayRemoveAt(list, index) : JSON_ERR_TYPE;
    }
    return code;
}

stock JSON_ArrayClear(Node:node, const key[])
{
    new Node:list, code = JSON_GetNode(node, key, list);
    if (code == JSON_OK)
    {
        code = JSON_TypeOf(list) == JSON_ARRAY ? JSON_SetNode(node, key, JSON_Array()) : JSON_ERR_TYPE;
    }
    return code;
}
