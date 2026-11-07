// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

/// The codes every read and write returns, with 0 for success. A function that makes a
/// node returns the node instead, or 0 if it could not, and never one of these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Error {
    /// The handle names no node, or a node that has since been freed.
    Node = 1,
    /// The node is not the object or array the call needs.
    Container = 2,
    /// The key is not there, or the index is out of range.
    Key = 3,
    /// The value is of a different type than the one asked for.
    Type = 4,
    /// The buffer was too short, so the string in it was cut to fit.
    Buffer = 5,
    /// The integer does not fit in 32 bits, so the variable was left as it was.
    Range = 6,
    Syntax = 7,
    File = 8,
    Path = 9,
    Format = 10,
    /// An argument's address points outside the script's data.
    Address = 11,
    /// The call hit a bug in the plugin and was stopped; the server log says where.
    Internal = 12,
}

pub const OK: i32 = 0;

pub fn code(r: Result<(), Error>) -> i32 {
    match r {
        Ok(()) => OK,
        Err(e) => e as i32,
    }
}

/// What `JSON_TypeOf` returns. `Missing` means a key that is not there or a node that is
/// gone, and it has a value of its own: pawn-json 1.x reported a missing key as a number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Type {
    Missing = 0,
    Null = 1,
    Bool = 2,
    Int = 3,
    Float = 4,
    String = 5,
    Array = 6,
    Object = 7,
}
