// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

// expect: warning 213
// expect: Text3D
// JSON_Get with a variable whose tag it can't read should fail to compile cleanly,
// and the warning should name the tag.
#include <json>

main()
{
    new Node:o, Text3D:label;
    JSON_Get(o, "k", label);
}
