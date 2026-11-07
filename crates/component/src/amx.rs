// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

//! A small AMX layer of our own. A native's arguments are read straight through the `AMX`
//! struct, and every address is checked against the script's data segment before it is
//! touched.

use std::ffi::{c_long, c_void};

pub type Cell = i32;

/// Mirrors `tagAMX` from `amx.h` field for field.
#[repr(C)]
pub struct Amx {
    pub base: *mut u8,
    pub data: *mut u8,
    pub callback: *mut c_void,
    pub debug: *mut c_void,
    pub cip: Cell,
    pub frm: Cell,
    pub hea: Cell,
    pub hlw: Cell,
    pub stk: Cell,
    pub stp: Cell,
    pub flags: i32,
    pub usertags: [c_long; 4],
    pub userdata: [*mut c_void; 4],
    pub error: i32,
    pub paramcount: i32,
    pub pri: Cell,
    pub alt: Cell,
    pub reset_stk: Cell,
    pub reset_hea: Cell,
    pub sysreq_d: Cell,
}

/// Where `AMX_HEADER`'s `dat` field sits in the packed header. It holds the data
/// segment's offset from `base`.
const HEADER_DAT: usize = 16;

/// A native's arguments. The range `[0, top)` of the data segment covers the script's
/// data, stack and heap, and nothing outside it is ever read or written.
pub struct Args {
    amx: *mut Amx,
    data: *mut u8,
    top: usize,
    params: *const Cell,
    count: usize,
}

impl Args {
    /// # Safety
    /// `amx` and `params` must be the pointers the abstract machine passed to a native.
    pub unsafe fn new(amx: *mut Amx, params: *const Cell) -> Args {
        unsafe {
            let a = &*amx;
            let data = if a.data.is_null() {
                let dat = std::ptr::read_unaligned(a.base.add(HEADER_DAT) as *const i32);
                a.base.add(dat as usize)
            } else {
                a.data
            };
            Args {
                amx,
                data,
                top: a.stp.max(0) as usize,
                params,
                count: (*params).max(0) as usize / size_of::<Cell>(),
            }
        }
    }

    pub fn count(&self) -> usize {
        self.count
    }

    pub fn amx(&self) -> *mut Amx {
        self.amx
    }

    /// The frame of the Pawn function that called the native.
    pub fn frame(&self) -> Cell {
        unsafe { (*self.amx).frm }
    }

    /// The value of the `i`th argument, or 0 if the call didn't pass it.
    pub fn cell(&self, i: usize) -> Cell {
        if i < self.count {
            unsafe { *self.params.add(1 + i) }
        } else {
            0
        }
    }

    /// Where the `i`th argument points, as long as it was passed, is cell-aligned, and the
    /// `cells` cells from there all lie inside the script's memory.
    fn at(&self, i: usize, cells: usize) -> Option<*mut Cell> {
        if i >= self.count {
            return None;
        }
        let addr = usize::try_from(self.cell(i)).ok()?;
        if addr % size_of::<Cell>() != 0 {
            return None;
        }
        let end = addr.checked_add(cells.checked_mul(size_of::<Cell>())?)?;
        (end <= self.top).then(|| unsafe { self.data.add(addr) as *mut Cell })
    }

    /// The string the `i`th argument points at, packed or unpacked. The slice includes
    /// the cell holding the terminator, or runs to the end of the data segment if there
    /// is none.
    pub fn string(&self, i: usize) -> Option<&[Cell]> {
        let start = self.at(i, 1)?;
        let room = (self.top - self.cell(i) as usize) / size_of::<Cell>();
        let all = unsafe { std::slice::from_raw_parts(start, room) };
        let packed = all.first().is_some_and(|&c| c as u32 > 0x00FF_FFFF);
        let end = if packed {
            all.iter()
                .position(|&c| (c as u32).to_be_bytes().contains(&0))
        } else {
            all.iter().position(|&c| c as u8 == 0)
        };
        Some(&all[..end.map_or(all.len(), |e| e + 1)])
    }

    /// Reads the cell a reference argument points at.
    pub fn read(&self, i: usize) -> Option<Cell> {
        self.at(i, 1).map(|p| unsafe { p.read() })
    }

    /// The `len`-cell buffer an output argument points at. A zero-length buffer is
    /// refused, since it would have no room for the terminator.
    pub fn buffer(&self, i: usize, len: usize) -> Option<(*mut Cell, usize)> {
        if len == 0 {
            return None;
        }
        self.at(i, len).map(|p| (p, len))
    }

    /// The `len` cells an array argument points at.
    pub fn cells(&self, i: usize, len: usize) -> Option<&[Cell]> {
        let start = self.at(i, len)?;
        Some(unsafe { std::slice::from_raw_parts(start, len) })
    }

    /// Reads the cell at a data address. The natives use it to read the saved frame at
    /// `[frm]`.
    pub fn data_cell(&self, addr: Cell) -> Option<Cell> {
        let addr = usize::try_from(addr).ok()?;
        (addr % size_of::<Cell>() == 0 && addr + size_of::<Cell>() <= self.top)
            .then(|| unsafe { (self.data.add(addr) as *const Cell).read() })
    }

    /// Writes the cell a reference argument points at. Returns false for a bad address.
    pub fn write(&self, i: usize, value: Cell) -> bool {
        match self.at(i, 1) {
            Some(p) => {
                unsafe { p.write(value) };
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
pub mod fake {
    use super::*;

    /// A stand-in script for tests: a data segment and an AMX that points at it.
    pub struct Script {
        pub data: Vec<Cell>,
        pub amx: Box<Amx>,
        heap: usize,
    }

    impl Script {
        pub fn new(cells: usize) -> Script {
            let mut data = vec![0; cells];
            let amx = Box::new(Amx {
                base: std::ptr::null_mut(),
                data: data.as_mut_ptr() as *mut u8,
                callback: std::ptr::null_mut(),
                debug: std::ptr::null_mut(),
                cip: 0,
                frm: 0,
                hea: 0,
                hlw: 0,
                stk: 0,
                stp: (cells * size_of::<Cell>()) as Cell,
                flags: 0,
                usertags: [0; 4],
                userdata: [std::ptr::null_mut(); 4],
                error: 0,
                paramcount: 0,
                pri: 0,
                alt: 0,
                reset_stk: 0,
                reset_hea: 0,
                sysreq_d: 0,
            });
            Script { data, amx, heap: 1 }
        }

        /// Copies cells into the data segment and returns their address.
        pub fn put(&mut self, cells: &[Cell]) -> Cell {
            let at = self.heap;
            self.data[at..at + cells.len()].copy_from_slice(cells);
            self.heap += cells.len();
            (at * size_of::<Cell>()) as Cell
        }

        /// Frees everything `put` placed from cell `at` on, for the next call's arguments.
        pub fn rewind(&mut self, at: usize) {
            self.heap = at;
        }

        pub fn string(&mut self, s: &[u8]) -> Cell {
            let cells: Vec<Cell> = s.iter().map(|&b| b as Cell).chain([0]).collect();
            self.put(&cells)
        }

        pub fn cell_at(&self, addr: Cell) -> Cell {
            self.data[addr as usize / size_of::<Cell>()]
        }

        /// Calls a native the way the abstract machine would.
        pub fn call(
            &mut self,
            native: extern "C" fn(*mut Amx, *const Cell) -> Cell,
            args: &[Cell],
        ) -> Cell {
            let params: Vec<Cell> = [size_of_val(args) as Cell]
                .into_iter()
                .chain(args.iter().copied())
                .collect();
            native(&mut *self.amx, params.as_ptr())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::Script;
    use super::*;
    use pawn_json_core::text::{Codepage, with_text};

    fn args(script: &mut Script, args: &[Cell]) -> (Args, Vec<Cell>) {
        let params: Vec<Cell> = [size_of_val(args) as Cell]
            .into_iter()
            .chain(args.iter().copied())
            .collect();
        let a = unsafe { Args::new(&mut *script.amx, params.as_ptr()) };
        (a, params)
    }

    #[test]
    fn the_amx_struct_is_laid_out_as_amx_h() {
        if size_of::<usize>() == 4 {
            assert_eq!(std::mem::offset_of!(Amx, frm), 20);
            assert_eq!(std::mem::offset_of!(Amx, stp), 36);
            assert_eq!(std::mem::offset_of!(Amx, error), 76);
        }
    }

    #[test]
    fn an_address_past_the_data_answers_none() {
        let mut s = Script::new(16);
        let key = s.string(b"abc");
        let (a, _p) = args(&mut s, &[key, 64, 60, -4, 2, 1 << 30]);
        assert!(a.string(0).is_some());
        assert!(a.string(1).is_none(), "the top itself");
        assert!(a.write(2, 7), "the last cell");
        assert!(!a.write(1, 7) && !a.write(3, 7) && !a.write(4, 7) && !a.write(5, 7));
        assert!(!a.write(6, 7), "an argument the call did not pass");
    }

    #[test]
    fn a_string_with_no_end_stops_at_the_data_s_end() {
        let mut s = Script::new(8);
        let at = s.put(&[b'x' as Cell; 7]);
        let (a, _p) = args(&mut s, &[at]);
        assert_eq!(a.string(0).unwrap().len(), 7);
    }

    #[test]
    fn a_key_read_allocates_nothing() {
        let mut s = Script::new(32);
        let key = s.string(b"vehicle_model");
        let params = [4, key];
        let cp = Codepage::DEFAULT;
        let n = crate::tests::allocations(|| {
            let a = unsafe { Args::new(&mut *s.amx, params.as_ptr()) };
            assert_eq!(with_text(a.string(0).unwrap(), cp, |k| k.len()), 13);
        });
        assert_eq!(n, 0);
    }

    #[test]
    fn a_packed_and_an_unpacked_string_read_the_same() {
        let mut s = Script::new(32);
        let unpacked = s.string(b"model_\xF5");
        let packed = s.put(&[
            i32::from_be_bytes(*b"mode"),
            i32::from_be_bytes([b'l', b'_', 0xF5, 0]),
        ]);
        let (a, _p) = args(&mut s, &[unpacked, packed]);
        let cp = Codepage::from_label("windows-1250").unwrap();
        let x = with_text(a.string(0).unwrap(), cp, |k| k.to_owned());
        let y = with_text(a.string(1).unwrap(), cp, |k| k.to_owned());
        assert_eq!(x, "model_\u{151}");
        assert_eq!(x, y);
    }
}
