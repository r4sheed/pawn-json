// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

//! Loading and saving files. A relative path starts from the server's working folder, as
//! it did in pawn-json 1.4.1. A save writes a temporary file next to the target and then
//! renames it over the old one, so a crash or a full disk halfway through leaves the old
//! file whole.

use crate::dom::{Doc, NodeId, ParseError};
use crate::write;
use std::fs::File;
use std::io::{self, Write};

#[derive(Debug)]
pub enum LoadError {
    Io(io::Error),
    Syntax(ParseError),
}

pub fn load(path: &str) -> Result<Doc, LoadError> {
    let bytes = std::fs::read(path).map_err(LoadError::Io)?;
    let text = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
    Doc::parse_slice(text).map_err(LoadError::Syntax)
}

pub fn save(doc: &Doc, node: NodeId, path: &str, pretty: bool) -> io::Result<()> {
    let text = write::to_string(doc, node, pretty);
    save_with(path, text.as_bytes(), |f, bytes| f.write_all(bytes))
}

/// The body of `save`, with the writing of the bytes into the temporary file left to
/// `write`, so a test can make it fail halfway through.
pub fn save_with(
    path: &str,
    bytes: &[u8],
    write: impl FnOnce(&mut File, &[u8]) -> io::Result<()>,
) -> io::Result<()> {
    let tmp = format!("{path}.tmp");
    let result = File::create(&tmp)
        .and_then(|mut f| write(&mut f, bytes).and_then(|()| f.sync_all()))
        .and_then(|()| std::fs::rename(&tmp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::Val;

    fn scratch(name: &str) -> String {
        let dir = std::env::temp_dir().join("pawn-json-core-tests");
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name).to_string_lossy().into_owned()
    }

    #[test]
    fn a_save_that_fails_midway_leaves_the_old_file_whole() {
        let path = scratch("midway.json");
        std::fs::write(&path, b"{\"old\":true}").unwrap();
        let r = save_with(&path, b"{\"new\":1234567890}", |f, bytes| {
            f.write_all(&bytes[..bytes.len() / 2])?;
            Err(io::Error::other("the disk is full"))
        });
        assert!(r.is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"{\"old\":true}");
        assert!(!std::path::Path::new(&format!("{path}.tmp")).exists());
    }

    #[test]
    fn a_save_replaces_the_old_file() {
        let path = scratch("replace.json");
        std::fs::write(
            &path,
            b"{\"old\":true,\"long\":\"a longer old file than the new one\"}",
        )
        .unwrap();
        let doc = Doc::parse("{\"n\":\"\u{151}\"}").unwrap();
        save(&doc, doc.root(), &path, false).unwrap();
        assert_eq!(
            std::fs::read(&path).unwrap(),
            "{\"n\":\"\u{151}\"}".as_bytes()
        );
        let back = load(&path).unwrap();
        let Val::Obj(m) = back.val(back.root()) else {
            panic!()
        };
        assert_eq!(back.val(m["n"]), &Val::Str("\u{151}".into()));
    }

    #[test]
    fn a_file_with_a_bom_loads_and_a_missing_one_answers_io() {
        let path = scratch("bom.json");
        std::fs::write(&path, b"\xEF\xBB\xBF[1]").unwrap();
        assert!(load(&path).is_ok());
        assert!(matches!(
            load(&scratch("missing.json")),
            Err(LoadError::Io(_))
        ));
        std::fs::write(&path, b"[1,").unwrap();
        assert!(matches!(load(&path), Err(LoadError::Syntax(_))));
    }
}
