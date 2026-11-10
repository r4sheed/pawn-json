// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

// pawn-json 1.x's own tests, from test.pwn in Southclaws/pawn-json (MIT licence,
// copyright (c) 2018 Barnaby "Southclaws" Keene), moved onto this harness: each Test:
// is a function here and each ASSERT a T_ check. Where 1.x's answer was a bug this
// plugin fixes, the check says what this plugin answers instead, and why.

#include "test"
#include <json>

native strcmp(const string1[], const string2[], bool:ignorecase = false, length = cellmax);

main()
{
}

forward OnGameModeInit();
public OnGameModeInit()
{
    Upstream_JsonParse();
    Upstream_JsonNodeType();
    Upstream_JsonObjectEmpty();
    Upstream_JsonObjectInt();
    Upstream_JsonObjectInts();
    Upstream_JsonObjectFloat();
    Upstream_JsonObjectFloats();
    Upstream_JsonObjectBool();
    Upstream_JsonObjectBools();
    Upstream_JsonObjectString();
    Upstream_JsonObjectStrings();
    Upstream_JsonStringArray();
    Upstream_JsonIntArray();
    Upstream_JsonFloatArray();
    Upstream_JsonObjectArray();
    Upstream_JsonObjectComplex();
    Upstream_JsonAppendObject();
    Upstream_JsonAppendArray();
    Upstream_JsonSetObject();
    Upstream_JsonSetArray();
    Upstream_JsonSetInt();
    Upstream_JsonSetFloat();
    Upstream_JsonSetBool();
    Upstream_JsonSetString();
    Upstream_JsonGetInt();
    Upstream_JsonGetFloat();
    Upstream_JsonGetBool();
    Upstream_JsonGetString();
    Upstream_JsonGetArray();
    Upstream_JsonGetIntInvalid();
    Upstream_JsonGetFloatInvalid();
    Upstream_JsonGetBoolInvalid();
    Upstream_JsonGetStringInvalid();
    Upstream_JsonGetArrayInvalid();
    Upstream_JsonArrayLength();
    Upstream_JsonArrayObject();
    Upstream_JsonGetNodeInt();
    Upstream_JsonGetNodeFloat();
    Upstream_JsonGetNodeBool();
    Upstream_JsonGetNodeString();
    Upstream_JsonScopeGC();
    Upstream_JsonToggleGC();
    Upstream_JsonArrayAppend();
    T_Done();
    return 1;
}

static Upstream_JsonParse()
{
    new Node:node;
    new ret;

    new input[] = "{\"list\":[{\"a_listobj_float\":66.5999984741211,\"a_listobj_number\":76,\"a_listobj_string\":\"another value\",\"one\":\"value one\"},{\"a_listobj_float\":66.5999984741211,\"a_listobj_number\":76,\"a_listobj_string\":\"another value\",\"two\":\"value two\"},{\"a_listobj_float\":66.5999984741211,\"a_listobj_number\":76,\"a_listobj_string\":\"another value\",\"three\":\"value three\"}],\"object\":{\"a_float\":66.5999984741211,\"a_number\":76,\"a_string\":\"a value\",\"nested_object\":{\"a_deeper_float\":66.5999984741211,\"a_deeper_number\":76,\"a_deeper_string\":\"another value\"}}}";

    ret = JSON_Parse(input, node);
    T_Is(ret, 0);

    new output[1024];
    T_Is(JSON_Stringify(node, output), 0);
    T_True(!strcmp(input, output));
}

static Upstream_JsonNodeType()
{
    new Node:number = JSON_Int(3); // JSON_NODE_NUMBER
    T_True(JSON_NodeType(number) ==  JSON_NODE_NUMBER);

    new Node:boolean = JSON_Bool(true); // JSON_NODE_BOOLEAN
    T_True(JSON_NodeType(boolean) ==  JSON_NODE_BOOLEAN);

    new Node:string = JSON_String("hi"); // JSON_NODE_STRING
    T_True(JSON_NodeType(string) ==  JSON_NODE_STRING);

    new Node:object = JSON_Object("k", JSON_Int(1)); // JSON_NODE_OBJECT
    T_True(JSON_NodeType(object) ==  JSON_NODE_OBJECT);

    new Node:array = JSON_Array(JSON_Int(1), JSON_Int(2)); // JSON_NODE_ARRAY
    T_True(JSON_NodeType(array) ==  JSON_NODE_ARRAY);

    // 1.x called a node that doesn't exist JSON_NODE_NULL, the same as a JSON null. Here
    // it is JSON_NODE_MISSING, so the two can be told apart.
    new Node:null = Node:-1;
    T_True(JSON_NodeType(null) ==  JSON_NODE_MISSING);
}

static Upstream_JsonObjectEmpty()
{
    new Node:node = JSON_Object();

    new buf[128];
    new ret = JSON_Stringify(node, buf);
    T_True(ret == 0);
    T_True(!strcmp(buf, "{}"));
}

static Upstream_JsonObjectInt()
{
    new Node:node = JSON_Object(
        "key", JSON_Int(1)
    );

    new buf[128];
    new ret = JSON_Stringify(node, buf);
    T_True(ret == 0);
    T_True(!strcmp(buf, "{\"key\":1}"));
    print(buf);
}

static Upstream_JsonObjectInts()
{
    new Node:node = JSON_Object(
        "key1", JSON_Int(1),
        "key2", JSON_Int(2),
        "key3", JSON_Int(3)
    );

    new buf[128];
    new ret = JSON_Stringify(node, buf);
    T_True(ret == 0);
    T_True(!strcmp(buf, "{\"key1\":1,\"key2\":2,\"key3\":3}"));
    print(buf);
}

static Upstream_JsonObjectFloat()
{
    new Node:node = JSON_Object(
        "key", JSON_Float(1.5)
    );

    new buf[128];
    new ret = JSON_Stringify(node, buf);
    T_True(ret == 0);
    T_True(!strcmp(buf, "{\"key\":1.5}"));
    print(buf);
}

static Upstream_JsonObjectFloats()
{
    new Node:node = JSON_Object(
        "key1", JSON_Float(1.5),
        "key2", JSON_Float(2.5),
        "key3", JSON_Float(3.5)
    );

    new buf[128];
    new ret = JSON_Stringify(node, buf);
    T_True(ret == 0);
    T_True(!strcmp(buf, "{\"key1\":1.5,\"key2\":2.5,\"key3\":3.5}"));
    print(buf);
}

static Upstream_JsonObjectBool()
{
    new Node:node = JSON_Object(
        "key", JSON_Bool(true)
    );

    new buf[128];
    new ret = JSON_Stringify(node, buf);
    T_True(ret == 0);
    T_True(!strcmp(buf, "{\"key\":true}"));
    print(buf);
}

static Upstream_JsonObjectBools()
{
    new Node:node = JSON_Object(
        "key1", JSON_Bool(false),
        "key2", JSON_Bool(true),
        "key3", JSON_Bool(false)
    );

    new buf[128];
    new ret = JSON_Stringify(node, buf);
    T_True(ret == 0);
    T_True(!strcmp(buf, "{\"key1\":false,\"key2\":true,\"key3\":false}"));
    print(buf);
}

static Upstream_JsonObjectString()
{
    new Node:node = JSON_Object(
        "key", JSON_String("value")
    );

    new buf[128];
    new ret = JSON_Stringify(node, buf);
    T_True(ret == 0);
    T_True(!strcmp(buf, "{\"key\":\"value\"}"));
    print(buf);
}

static Upstream_JsonObjectStrings()
{
    new Node:node = JSON_Object(
        "key1", JSON_String("value1"),
        "key2", JSON_String("value2"),
        "key3", JSON_String("value3")
    );

    new buf[128];
    new ret = JSON_Stringify(node, buf);
    T_True(ret == 0);
    T_True(!strcmp(buf, "{\"key1\":\"value1\",\"key2\":\"value2\",\"key3\":\"value3\"}"));
    print(buf);
}

static Upstream_JsonStringArray()
{
    new Node:node = JSON_Array(
        JSON_String("one"),
        JSON_String("two"),
        JSON_String("three")
    );

    new buf[128];
    new ret = JSON_Stringify(node, buf);
    T_True(ret == 0);
    T_True(!strcmp(buf, "[\"one\",\"two\",\"three\"]"));
    print(buf);
}

static Upstream_JsonIntArray()
{
    new Node:node = JSON_Array(
        JSON_Int(1),
        JSON_Int(2),
        JSON_Int(3)
    );

    new buf[128];
    new ret = JSON_Stringify(node, buf);
    T_True(ret == 0);
    T_True(!strcmp(buf, "[1,2,3]"));
    print(buf);
}

static Upstream_JsonFloatArray()
{
    new Node:node = JSON_Array(
        JSON_Float(1.5),
        JSON_Float(2.5),
        JSON_Float(3.5)
    );

    new buf[128];
    new ret = JSON_Stringify(node, buf);
    T_True(ret == 0);
    T_True(!strcmp(buf, "[1.5,2.5,3.5]"));
    print(buf);
}

static Upstream_JsonObjectArray()
{
    new Node:node = JSON_Array(
        JSON_Object(
            "one", JSON_String("value one")
        ),
        JSON_Object(
            "two", JSON_String("value two")
        ),
        JSON_Object(
            "three", JSON_String("value three")
        )
    );

    new buf[128];
    new ret = JSON_Stringify(node, buf);
    T_True(ret == 0);
    T_True(!strcmp(buf, "[{\"one\":\"value one\"},{\"two\":\"value two\"},{\"three\":\"value three\"}]"));
    print(buf);
}

/*
JSON_ObjectComplex generates this rather complex JSON object:
{
  "object": {
    "a_float": 66.599998474121094,
    "a_number": 76,
    "a_string": "a value",
    "nested_object": {
      "a_deeper_float": 66.599998474121094,
      "a_deeper_number": 76,
      "a_deeper_string": "another value"
    }
  },
  "list": [
    {
      "a_listobj_float": 66.599998474121094,
      "a_listobj_number": 76,
      "a_listobj_string": "another value",
      "one": "value one"
    },
    {
      "a_listobj_float": 66.599998474121094,
      "a_listobj_number": 76,
      "a_listobj_string": "another value",
      "two": "value two"
    },
    {
      "a_listobj_float": 66.599998474121094,
      "a_listobj_number": 76,
      "a_listobj_string": "another value",
      "three": "value three"
    }
  ]
}
*/
static Upstream_JsonObjectComplex()
{
    new Node:node = JSON_Object(
        "object", JSON_Object(
            "a_string", JSON_String("a value"),
            "a_number", JSON_Int(76),
            "a_float", JSON_Float(66.6),
            "nested_object", JSON_Object(
                "a_deeper_string", JSON_String("another value"),
                "a_deeper_number", JSON_Int(76),
                "a_deeper_float", JSON_Float(66.6)
            )
        ),
        "list", JSON_Array(
            JSON_Object(
                "one", JSON_String("value one"),
                "a_listobj_string", JSON_String("another value"),
                "a_listobj_number", JSON_Int(76),
                "a_listobj_float", JSON_Float(66.6)
            ),
            JSON_Object(
                "two", JSON_String("value two"),
                "a_listobj_string", JSON_String("another value"),
                "a_listobj_number", JSON_Int(76),
                "a_listobj_float", JSON_Float(66.6)
            ),
            JSON_Object(
                "three", JSON_String("value three"),
                "a_listobj_string", JSON_String("another value"),
                "a_listobj_number", JSON_Int(76),
                "a_listobj_float", JSON_Float(66.6)
            )
        )
    );

    new buf[1024];
    new ret = JSON_Stringify(node, buf);
    T_True(ret == 0);
    // Two things differ from 1.x here. 1.x sorted an object's keys, where they are kept in
    // the order they were written. And 1.x wrote JSON_Float(66.6) as 66.5999984741211,
    // the float widened to a double, where it is written as the shortest text that reads
    // back as the same float.
    T_True(!strcmp(buf, "{\"object\":{\"a_string\":\"a value\",\"a_number\":76,\"a_float\":66.6,\"nested_object\":{\"a_deeper_string\":\"another value\",\"a_deeper_number\":76,\"a_deeper_float\":66.6}},\"list\":[{\"one\":\"value one\",\"a_listobj_string\":\"another value\",\"a_listobj_number\":76,\"a_listobj_float\":66.6},{\"two\":\"value two\",\"a_listobj_string\":\"another value\",\"a_listobj_number\":76,\"a_listobj_float\":66.6},{\"three\":\"value three\",\"a_listobj_string\":\"another value\",\"a_listobj_number\":76,\"a_listobj_float\":66.6}]}"));
    print(buf);
}

static Upstream_JsonAppendObject()
{
    new Node:a = JSON_Object(
        "key1", JSON_String("value1"),
        "key2", JSON_String("value2")
    );
    new Node:b = JSON_Object(
        "key3", JSON_String("value3")
    );

    new Node:c = JSON_Append(a, b);

    new buf[128];
    new ret = JSON_Stringify(c, buf);
    T_Is(ret, 0);
    T_True(T_Same(buf, "{\"key1\":\"value1\",\"key2\":\"value2\",\"key3\":\"value3\"}"));
    print(buf);
}

static Upstream_JsonAppendArray()
{
    new Node:a = JSON_Array(
        JSON_Int(1),
        JSON_Int(2)
    );
    new Node:b = JSON_Array(
        JSON_Int(3)
    );

    new Node:c = JSON_Append(a, b);

    new buf[128];
    new ret = JSON_Stringify(c, buf);
    T_Is(ret, 0);
    T_True(T_Same(buf, "[1,2,3]"));
    print(buf);
}

static Upstream_JsonSetObject()
{
    new Node:node = JSON_Object();
    new ret = JSON_SetObject(node, "key", JSON_Object("key", JSON_String("value")));
    T_True(ret == 0);

    new buf[128];
    ret = JSON_Stringify(node, buf);
    T_True(ret == 0);
    T_True(!strcmp(buf, "{\"key\":{\"key\":\"value\"}}"));
    print(buf);
}

static Upstream_JsonSetArray()
{
    new Node:node = JSON_Object();
    new ret = JSON_SetArray(node, "key", JSON_Array(JSON_Int(1), JSON_Int(2)));
    T_True(ret == 0);

    new buf[128];
    ret = JSON_Stringify(node, buf);
    T_True(ret == 0);
    T_True(!strcmp(buf, "{\"key\":[1,2]}"));
}

static Upstream_JsonSetInt()
{
    new Node:node = JSON_Object();
    new ret = JSON_SetInt(node, "key", 5);
    T_True(ret == 0);

    new buf[128];
    ret = JSON_Stringify(node, buf);
    T_True(ret == 0);
    T_True(!strcmp(buf, "{\"key\":5}"));
    print(buf);
}

static Upstream_JsonSetFloat()
{
    new Node:node = JSON_Object();
    new ret = JSON_SetFloat(node, "key", 5.5);
    T_True(ret == 0);

    new buf[128];
    ret = JSON_Stringify(node, buf);
    T_True(ret == 0);
    T_True(!strcmp(buf, "{\"key\":5.5}"));
    print(buf);
}

static Upstream_JsonSetBool()
{
    new Node:node = JSON_Object();
    new ret = JSON_SetBool(node, "key", true);
    T_True(ret == 0);

    new buf[128];
    ret = JSON_Stringify(node, buf);
    T_True(ret == 0);
    T_True(!strcmp(buf, "{\"key\":true}"));
    print(buf);
}

static Upstream_JsonSetString()
{
    new Node:node = JSON_Object();
    new ret = JSON_SetString(node, "key", "value");
    T_True(ret == 0);

    new buf[128];
    ret = JSON_Stringify(node, buf);
    T_True(ret == 0);
    T_True(!strcmp(buf, "{\"key\":\"value\"}"));
    print(buf);
}

static Upstream_JsonGetInt()
{
    new Node:node = JSON_Object(
        "key1", JSON_Int(1),
        "key2", JSON_Int(2),
        "key3", JSON_Int(3)
    );

    new got;
    new ret;
    
    ret = JSON_GetInt(node, "key1", got);
    T_Is(ret, 0);
    T_Is(got, 1);

    ret = JSON_GetInt(node, "key2", got);
    T_Is(ret, 0);
    T_Is(got, 2);

    ret = JSON_GetInt(node, "key3", got);
    T_Is(ret, 0);
    T_Is(got, 3);

    ret = JSON_GetInt(node, "key4", got);
    T_Is(ret, 2);
}

static Upstream_JsonGetFloat()
{
    new Node:node = JSON_Object(
        "key1", JSON_Float(1.5),
        "key2", JSON_Float(2.5),
        "key3", JSON_Float(3.5)
    );

    new Float:got;
    new ret;
    
    ret = JSON_GetFloat(node, "key1", got);
    T_True(ret == 0);
    T_True(got == 1.5);

    ret = JSON_GetFloat(node, "key2", got);
    T_True(ret == 0);
    T_True(got == 2.5);

    ret = JSON_GetFloat(node, "key3", got);
    T_True(ret == 0);
    T_True(got == 3.5);

    ret = JSON_GetFloat(node, "key4", got);
    T_True(ret == 2);
}

static Upstream_JsonGetBool()
{
    new Node:node = JSON_Object(
        "key1", JSON_Bool(false),
        "key2", JSON_Bool(true),
        "key3", JSON_Bool(false)
    );

    new bool:got;
    new ret;
    
    ret = JSON_GetBool(node, "key1", got);
    T_True(ret == 0);
    T_True(got == false);

    ret = JSON_GetBool(node, "key2", got);
    T_True(ret == 0);
    T_True(got == true);

    ret = JSON_GetBool(node, "key3", got);
    T_True(ret == 0);
    T_True(got == false);

    ret = JSON_GetBool(node, "key4", got);
    T_True(ret == 2);
}

static Upstream_JsonGetString()
{
    new Node:node = JSON_Object(
        "key1", JSON_String("value1"),
        "key2", JSON_String("value2"),
        "key3", JSON_String("value3")
    );

    new got[128];
    new ret;
    
    ret = JSON_GetString(node, "key1", got);
    T_True(ret == 0);
    T_True(!strcmp(got, "value1"));

    ret = JSON_GetString(node, "key2", got);
    T_True(ret == 0);
    T_True(!strcmp(got, "value2"));

    ret = JSON_GetString(node, "key3", got);
    T_True(ret == 0);
    T_True(!strcmp(got, "value3"));

    ret = JSON_GetString(node, "key4", got);
    T_True(ret == 2);
}

static Upstream_JsonGetArray()
{
    new Node:node = JSON_Object(
        "key1", JSON_Array(
            JSON_String("one"),
            JSON_String("two"),
            JSON_String("three")
        )
    );

    new Node:arrayNode;
    new ret;

    ret = JSON_GetArray(node, "key1", arrayNode);
    printf("JSON_GetArray:%d arrayNode: %d", ret, _:arrayNode);
    T_Is(ret, 0);

    new Node:output;
    new gotString[32];

    ret = JSON_ArrayObject(arrayNode, 0, output);
    T_Is(ret, 0);
    ret = JSON_GetNodeString(output, gotString);
    T_Is(ret, 0);
    T_True(T_Same(gotString, "one"));

    ret = JSON_ArrayObject(arrayNode, 1, output);
    T_Is(ret, 0);
    ret = JSON_GetNodeString(output, gotString);
    T_Is(ret, 0);
    T_True(T_Same(gotString, "two"));

    ret = JSON_ArrayObject(arrayNode, 2, output);
    T_Is(ret, 0);
    ret = JSON_GetNodeString(output, gotString);
    T_Is(ret, 0);
    T_True(T_Same(gotString, "three"));
}

static Upstream_JsonGetIntInvalid()
{
    new Node:node = JSON_Object("k", JSON_String("v"));
    new gotInt;
    new ret = JSON_GetInt(node, "key4", gotInt);
    T_True(ret == 2);
}

static Upstream_JsonGetFloatInvalid()
{
    new Node:node = JSON_Object("k", JSON_String("v"));
    new Float:gotFloat;
    new ret = JSON_GetFloat(node, "key4", gotFloat);
    T_True(ret == 2);
}

static Upstream_JsonGetBoolInvalid()
{
    new Node:node = JSON_Object("k", JSON_String("v"));
    new bool:gotBool;
    new ret = JSON_GetBool(node, "key4", gotBool);
    T_True(ret == 2);
}

static Upstream_JsonGetStringInvalid()
{
    new Node:node = JSON_Object("k", JSON_String("v"));
    new gotString[1];
    new ret = JSON_GetString(node, "key4", gotString);
    T_True(ret == 2);
}

static Upstream_JsonGetArrayInvalid()
{
    new Node:node = JSON_Object("k", JSON_String("v"));
    new Node:gotNode;
    new ret = JSON_GetArray(node, "key4", gotNode);
    T_True(ret == 2);
}

static Upstream_JsonArrayLength()
{
    new Node:node = JSON_Array(
        JSON_String("one"),
        JSON_String("two"),
        JSON_String("three")
    );

    new length;
    new ret;
    ret = JSON_ArrayLength(node, length);
    printf("ret %d length %d", ret, length);
    T_True(ret == 0);
    T_True(length == 3);
}

static Upstream_JsonArrayObject()
{
    new Node:node = JSON_Array(
        JSON_String("one"),
        JSON_String("two"),
        JSON_String("three")
    );

    new Node:output;
    new ret;
    ret = JSON_ArrayObject(node, 1, output);
    T_True(ret == 0);

    new got[32];
    ret = JSON_GetNodeString(output, got);
    T_True(ret == 0);
    T_True(!strcmp(got, "two"));
}

static Upstream_JsonGetNodeInt()
{
    new Node:node = JSON_Object(
        "key", JSON_Int(1)
    );

    new Node:output;
    new ret;
    ret = JSON_GetObject(node, "key", output);
    T_Is(ret, 0);

    new got;
    ret = JSON_GetNodeInt(output, got);
    T_Is(ret, 0);
    T_Is(got, 1);
}

static Upstream_JsonGetNodeFloat()
{
    new Node:node = JSON_Object(
        "key", JSON_Float(1.34)
    );

    new Node:output;
    new ret;
    ret = JSON_GetObject(node, "key", output);
    T_True(ret == 0);

    new Float:got;
    ret = JSON_GetNodeFloat(output, got);
    T_True(ret == 0);
    T_True(got == 1.34);
}

static Upstream_JsonGetNodeBool()
{
    new Node:node = JSON_Object(
        "key", JSON_Bool(true)
    );

    new Node:output;
    new ret;
    ret = JSON_GetObject(node, "key", output);
    T_True(ret == 0);

    new bool:got;
    ret = JSON_GetNodeBool(output, got);
    T_True(ret == 0);
    T_True(got == true);
}

static Upstream_JsonGetNodeString()
{
    new Node:node = JSON_Object(
        "key", JSON_String("value")
    );

    new Node:output;
    new ret;
    ret = JSON_GetObject(node, "key", output);
    T_True(ret == 0);

    new got[32];
    ret = JSON_GetNodeString(output, got);
    T_True(ret == 0);
    T_True(!strcmp(got, "value"));
}

static Upstream_JsonScopeGC()
{
    // 1.x freed a node when a function it was passed to returned, so the caller's node
    // was gone. Here it stays alive until the scope that made it ends, and the cleanup
    // that follows frees it.
    new Node:node = JSON_Object();
    scopeNodeGC(node);
    T_True(JSON_Cleanup(node) == 0);
}

static Upstream_JsonToggleGC()
{
    new Node:node = JSON_Object(
        "key", JSON_String("value")
    );
    JSON_ToggleGC(node, false);
    scopeNodeGC(node);
    new value[6];
    JSON_GetString(node, "key", value);
    T_True(T_Same(value, "value"));
    T_Is(JSON_Cleanup(node), 0);
    T_Is(JSON_Cleanup(node), 1);
}

static Upstream_JsonArrayAppend()
{
    new Node:arr = JSON_Array(
        JSON_Int(1),
        JSON_Int(2)
    );

    new Node:a = JSON_Object(
        "key1", arr
    );

    new ret = JSON_ArrayAppend(a, "key1", JSON_Int(3));
    T_Is(ret, 0);

    new buf[128];
    ret = JSON_Stringify(a, buf);
    T_Is(ret, 0);
    T_True(!strcmp(buf, "{\"key1\":[1,2,3]}"));
    print(buf);
}

scopeNodeGC(Node:node) {
    printf("scoped %d", _:node);
}
