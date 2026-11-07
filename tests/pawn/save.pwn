// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

// Saves finish in the background. These check what a script can still count on: a read
// right after a save sees it, saves to one file land in order, a bad path fails at once,
// a failure later on reaches JSON_Flush and the log, and the server finishes every save
// before it shuts down (tools/server-test.py reads those files after the exit).

#include "test"
#include <json>

main()
{
}

forward OnGameModeInit();
public OnGameModeInit()
{
    new Node:doc, Node:back, value;

    // Read back straight after each save.
    for (new i = 0; i < 20; ++i)
    {
        doc = JSON_Object();
        JSON_SetInt(doc, "n", i);
        T_Is(JSON_SaveFile("scriptfiles/save.json", doc), JSON_OK);
        T_Is(JSON_ParseFile("scriptfiles/save.json", back), JSON_OK);
        T_Is(JSON_GetInt(back, "n", value), JSON_OK);
        T_Is(value, i);
        JSON_Release(back);
        JSON_Release(doc);
    }

    // Fifty saves to one file without a read in between: the last one is what stays.
    doc = JSON_Object();
    for (new i = 0; i < 50; ++i)
    {
        JSON_SetInt(doc, "n", i);
        T_Is(JSON_SaveFile("scriptfiles/burst.json", doc), JSON_OK);
    }
    T_Is(JSON_Flush(), JSON_OK);
    T_Is(JSON_ParseFile("scriptfiles/burst.json", back), JSON_OK);
    T_Is(JSON_GetInt(back, "n", value), JSON_OK);
    T_Is(value, 49);

    // A folder that isn't there fails before anything is queued.
    T_Is(JSON_SaveFile("scriptfiles/no such folder/a.json", doc), JSON_ERR_FILE);
    T_Is(JSON_Flush(), JSON_OK);

    // The test runner makes scriptfiles/save-taken a folder. The temporary file can be
    // written, but it can't be renamed over a folder, so this one fails in the
    // background: JSON_Flush says so once, and the next one has nothing to tell.
    T_Is(JSON_SaveFile("scriptfiles/save-taken", doc), JSON_OK);
    T_Is(JSON_Flush(), JSON_ERR_FILE);
    T_Is(JSON_Flush(), JSON_OK);

    // Thirty saves queued just before the server is told to exit. Every one has to be on
    // the disk afterwards.
    new Node:pad = JSON_Array();
    for (new j = 0; j < 300; ++j)
    {
        JSON_AppendInt(pad, j);
    }
    new path[48];
    for (new k = 0; k < 30; ++k)
    {
        new Node:row = JSON_Object();
        JSON_SetInt(row, "i", k);
        JSON_SetNode(row, "pad", JSON_Copy(pad));
        format(path, sizeof (path), "scriptfiles/exit-%d.json", k);
        T_Is(JSON_SaveFile(path, row), JSON_OK);
    }
    T_Done();
    return 1;
}
