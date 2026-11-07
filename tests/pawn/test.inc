// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

// A tiny test harness, so the test scripts need no other include. A script runs its
// checks in OnGameModeInit and finishes with T_Done(), which prints the totals that
// tools/server-test.py looks for and then shuts the server down.

#pragma rational Float

native print(const string[]);
native printf(const format[], {Float, _}:...);
native SendRconCommand(const command[]);
native CallLocalFunction(const function[], const format[], {Float, _}:...);
native format(output[], len = sizeof (output), const format[], {Float, _}:...);

static T@pass, T@fail;

#define T_Is(%0,%1) T@Is((%0), (%1), __line)
#define T_True(%0) T@Is(!!(%0), 1, __line)

stock T@Is(got, want, line)
{
    if (got == want)
    {
        T@pass++;
    }
    else
    {
        T@fail++;
        printf("FAIL line %d: %d, want %d", line, got, want);
    }
}

// True if both strings are the same, character for character.
stock bool:T_Same(const a[], const b[])
{
    for (new i = 0; ; ++i)
    {
        if (a[i] != b[i])
        {
            return false;
        }
        if (a[i] == 0)
        {
            return true;
        }
    }
}

stock T_Done()
{
    printf("PASS %d FAIL %d", T@pass, T@fail);
    SendRconCommand("exit");
}
