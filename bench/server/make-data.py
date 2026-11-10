# This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
# copy of the MPL was not distributed with this file, You can obtain one at
# http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

"""Writes the files speed.pwn and quirks.pwn read: flat language files of 500 to 4000
keys, vehicle lists of 500 to 2000 rows, and a string with accented letters. The random seed is
fixed, so every run writes the same bytes.

    python make-data.py <server>/scriptfiles
"""

import json, os, random, sys
out = sys.argv[1]
os.makedirs(out, exist_ok=True)
rnd = random.Random(1)
words = "the vehicle is locked you do not have enough money for this car owner already has one saved successfully something went wrong".split()
for n in (500, 1000, 2000, 4000):
    d = {}
    for i in range(n):
        d["MSG_%05d" % i] = " ".join(rnd.choice(words) for _ in range(rnd.randint(4, 12))) + " (%d)"
    with open(os.path.join(out, "lang-%d.json" % n), "w", encoding="utf-8") as f:
        json.dump(d, f, ensure_ascii=False, indent=4)
for n in (500, 1000, 2000):
    rows = []
    for i in range(n):
        rows.append({"model": rnd.randint(400, 611), "x": round(rnd.uniform(-3000, 3000), 4),
                     "y": round(rnd.uniform(-3000, 3000), 4), "z": round(rnd.uniform(0, 100), 4),
                     "angle": round(rnd.uniform(0, 360), 4), "color1": rnd.randint(0, 255),
                     "color2": rnd.randint(0, 255), "plate": "SA %04d" % rnd.randint(0, 9999),
                     "owner": "Player_%d" % rnd.randint(1, 5000), "locked": rnd.random() < 0.5})
    with open(os.path.join(out, "vehicles-%d.json" % n), "w", encoding="utf-8") as f:
        json.dump({"vehicles": rows}, f, ensure_ascii=False, indent=4)
with open(os.path.join(out, "accent.json"), "w", encoding="utf-8") as f:
    f.write('{"t":"Café Señor, open from 9 to 5. Crème brûlée is $4."}')
