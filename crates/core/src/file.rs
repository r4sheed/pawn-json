// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

//! Loading and saving files. A relative path starts from the server's working folder, as
//! it did in pawn-json 1.4.1. A save writes a temporary file next to the target and then
//! renames it over the old one, so a crash or a full disk halfway through leaves the old
//! file whole.
//!
//! Most of a save's time goes on waiting for the disk to confirm the write. `Saver` does
//! that wait on a thread of its own: the text is in the temporary file before `save`
//! returns, and the thread flushes it to the disk and renames it, one save after another
//! in the order they were made.

use crate::dom::{Doc, NodeId, ParseError};
use crate::write;
use std::collections::VecDeque;
use std::fs::File;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;

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

/// Saves `node` and returns once it is on the disk. The natives go through `Saver`.
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
    names_a_file(path)?;
    let tmp = format!("{path}.tmp");
    let file = begin(&tmp, bytes, write)?;
    finish(file, &tmp, path)
}

/// Refuses a path that can't name a file, empty or ending in a separator, before
/// anything is written. Otherwise the temporary file would land beside it and the
/// failure would only show once the rename was tried.
fn names_a_file(path: &str) -> io::Result<()> {
    if path.is_empty() || path.ends_with(['/', '\\']) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the path names a folder, not a file",
        ));
    }
    Ok(())
}

/// Writes `bytes` into a new file at `tmp`, and removes it again if that fails.
fn begin(
    tmp: &str,
    bytes: &[u8],
    write: impl FnOnce(&mut File, &[u8]) -> io::Result<()>,
) -> io::Result<File> {
    let mut file = File::create(tmp)?;
    match write(&mut file, bytes) {
        Ok(()) => Ok(file),
        Err(e) => {
            drop(file);
            let _ = std::fs::remove_file(tmp);
            Err(e)
        }
    }
}

/// Waits for the temporary file to reach the disk, then renames it over `path`. If either
/// fails, the temporary file is removed and `path` keeps its old text.
fn finish(file: File, tmp: &str, path: &str) -> io::Result<()> {
    let synced = file.sync_all();
    // Closed before the rename, since Windows can refuse to move a file that is open.
    drop(file);
    let result = synced.and_then(|()| std::fs::rename(tmp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(tmp);
    }
    result
}

/// Saves that finish on a thread of their own.
///
/// `save` writes the text into `<path>.<n>.tmp` and returns, so a path that can't be
/// written to fails at once. The thread then waits for the disk and renames the file over
/// `path`. Until it does, `path` holds its old text, whole. If the server dies in those
/// few milliseconds, that save is lost but the old file is not. A failure on the thread is
/// kept for `take_failures`. `flush` waits for everything queued so far, and `close`
/// finishes the queue and stops the thread.
///
/// Each save gets a temporary file of its own, so two saves to one path in a row never
/// write into a file the thread is still finishing.
#[derive(Default)]
pub struct Saver {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
    /// Set once the thread has stopped, or failed to start. Saves then finish in place.
    in_place: bool,
    made: u64,
}

#[derive(Default)]
struct Shared {
    queue: Mutex<Queue>,
    /// Wakes the thread when a save is queued or the saver closes.
    work: Condvar,
    /// Wakes `flush` when the last queued save is done.
    idle: Condvar,
    /// Whether `failures` holds anything, so a server tick can look without the lock.
    failed: AtomicBool,
}

#[derive(Default)]
struct Queue {
    jobs: VecDeque<Job>,
    /// Saves queued or still being finished.
    open: usize,
    closed: bool,
    failures: Vec<String>,
}

struct Job {
    file: File,
    tmp: String,
    path: String,
}

impl Saver {
    /// Writes `node` into a temporary file beside `path` and hands the rest to the thread.
    /// An error writing the temporary file (a missing folder, no permission, a full disk)
    /// comes back here and nothing is queued.
    pub fn save(&mut self, doc: &Doc, node: NodeId, path: &str, pretty: bool) -> io::Result<()> {
        names_a_file(path)?;
        let text = write::to_string(doc, node, pretty);
        self.made += 1;
        let tmp = format!("{path}.{}.tmp", self.made);
        let file = begin(&tmp, text.as_bytes(), |f, bytes| f.write_all(bytes))?;
        if !self.start() {
            return finish(file, &tmp, path);
        }
        let mut q = lock(&self.shared.queue);
        q.open += 1;
        q.jobs.push_back(Job {
            file,
            tmp,
            path: path.to_owned(),
        });
        drop(q);
        self.shared.work.notify_one();
        Ok(())
    }

    /// Starts the thread on the first save. False if saves have to finish in place.
    fn start(&mut self) -> bool {
        if self.thread.is_some() {
            return true;
        }
        if self.in_place {
            return false;
        }
        let shared = Arc::clone(&self.shared);
        let spawned = std::thread::Builder::new()
            .name("pawn-json saves".into())
            .stack_size(256 * 1024)
            .spawn(move || finish_queued(&shared));
        match spawned {
            Ok(t) => {
                self.thread = Some(t);
                true
            }
            Err(_) => {
                self.in_place = true;
                false
            }
        }
    }

    /// Waits until every save queued so far is under its own name on the disk, or has
    /// failed. Returns at once when nothing is queued.
    pub fn flush(&self) {
        let mut q = lock(&self.shared.queue);
        while q.open > 0 {
            q = self
                .shared
                .idle
                .wait(q)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// The saves that failed on the thread since the last call, each as `path: error`.
    /// Cheap when there are none, so it can run every server tick.
    pub fn take_failures(&self) -> Vec<String> {
        if !self.shared.failed.swap(false, Ordering::AcqRel) {
            return Vec::new();
        }
        std::mem::take(&mut lock(&self.shared.queue).failures)
    }

    /// Finishes the queue and stops the thread. A later save finishes in place.
    pub fn close(&mut self) {
        lock(&self.shared.queue).closed = true;
        self.shared.work.notify_all();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        self.in_place = true;
    }
}

impl Drop for Saver {
    /// Tells the thread to stop once the queue is empty, without waiting for it; waiting
    /// is what `close` is for. A saver nobody closed is dropped at process exit, and on
    /// Windows the thread has been ended by then: joining it would abort the process.
    fn drop(&mut self) {
        lock(&self.shared.queue).closed = true;
        self.shared.work.notify_all();
    }
}

fn lock(queue: &Mutex<Queue>) -> MutexGuard<'_, Queue> {
    queue.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The saver's thread: finishes queued saves in order until the saver closes and the
/// queue is empty.
fn finish_queued(shared: &Shared) {
    loop {
        let job = {
            let mut q = lock(&shared.queue);
            loop {
                if let Some(job) = q.jobs.pop_front() {
                    break job;
                }
                if q.closed {
                    return;
                }
                q = shared.work.wait(q).unwrap_or_else(PoisonError::into_inner);
            }
        };
        // A bug here must not stop the thread with the save still counted as open, or
        // `flush` would wait for ever; it counts as a failed save instead.
        let Job { file, tmp, path } = job;
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| finish(file, &tmp, &path)))
                .unwrap_or_else(|_| {
                    let _ = std::fs::remove_file(&tmp);
                    Err(io::Error::other("the save failed inside the plugin"))
                });
        let mut q = lock(&shared.queue);
        if let Err(e) = result {
            q.failures.push(format!("{path}: {e}"));
            shared.failed.store(true, Ordering::Release);
        }
        q.open -= 1;
        if q.open == 0 {
            shared.idle.notify_all();
        }
    }
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

    /// A folder of the test's own, emptied first, so it can check that no temporary file
    /// was left behind.
    fn folder(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("pawn-json-core-tests").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn file_in(dir: &std::path::Path, name: &str) -> String {
        dir.join(name).to_string_lossy().into_owned()
    }

    fn temporaries(dir: &std::path::Path) -> Vec<String> {
        std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect()
    }

    fn save_text(saver: &mut Saver, path: &str, text: &str) -> io::Result<()> {
        let doc = Doc::parse(text).unwrap();
        saver.save(&doc, doc.root(), path, false)
    }

    #[test]
    fn a_queued_save_is_on_the_disk_after_flush() {
        let dir = folder("saver-flush");
        let path = file_in(&dir, "a.json");
        let mut saver = Saver::default();
        save_text(&mut saver, &path, r#"{"n":1}"#).unwrap();
        saver.flush();
        assert_eq!(std::fs::read(&path).unwrap(), br#"{"n":1}"#);
        assert!(temporaries(&dir).is_empty());
        assert!(saver.take_failures().is_empty());
    }

    #[test]
    fn saves_to_one_path_land_in_the_order_they_were_made() {
        let dir = folder("saver-order");
        let path = file_in(&dir, "a.json");
        let mut saver = Saver::default();
        for i in 0..200 {
            save_text(&mut saver, &path, &format!(r#"{{"n":{i}}}"#)).unwrap();
        }
        saver.flush();
        assert_eq!(std::fs::read(&path).unwrap(), br#"{"n":199}"#);
        assert!(temporaries(&dir).is_empty());
    }

    #[test]
    fn saves_to_many_paths_all_land() {
        let dir = folder("saver-many");
        let mut saver = Saver::default();
        for i in 0..300 {
            let path = file_in(&dir, &format!("f{}.json", i % 7));
            save_text(&mut saver, &path, &format!("[{i}]")).unwrap();
        }
        saver.flush();
        for k in 0..7 {
            let last = (0..300).filter(|i| i % 7 == k).max().unwrap();
            let got = std::fs::read_to_string(file_in(&dir, &format!("f{k}.json"))).unwrap();
            assert_eq!(got, format!("[{last}]"));
        }
        assert!(temporaries(&dir).is_empty());
    }

    #[test]
    fn a_path_that_cannot_be_written_fails_at_once() {
        let dir = folder("saver-missing");
        let mut saver = Saver::default();
        let path = file_in(&dir.join("no such folder"), "a.json");
        assert!(save_text(&mut saver, &path, "[1]").is_err());
        saver.flush();
        assert!(saver.take_failures().is_empty());
    }

    #[test]
    fn a_path_that_names_a_folder_fails_at_once() {
        let dir = folder("saver-folder");
        let mut saver = Saver::default();
        let slash = format!("{}/", dir.to_string_lossy());
        let backslash = format!("{}\\", dir.to_string_lossy());
        for path in ["", slash.as_str(), backslash.as_str()] {
            assert!(save_text(&mut saver, path, "[1]").is_err(), "{path:?}");
        }
        let doc = Doc::parse("[1]").unwrap();
        assert!(save(&doc, doc.root(), "", false).is_err());
        saver.flush();
        assert!(saver.take_failures().is_empty());
        assert!(temporaries(&dir).is_empty());
    }

    #[test]
    fn a_failure_on_the_thread_is_kept_for_the_log() {
        let dir = folder("saver-fails");
        // A file can't be renamed over a folder, so the thread's half fails.
        let taken = dir.join("taken.json");
        std::fs::create_dir(&taken).unwrap();
        std::fs::write(taken.join("inside"), b"x").unwrap();
        let path = taken.to_string_lossy().into_owned();
        let mut saver = Saver::default();
        save_text(&mut saver, &path, "[1]").unwrap();
        saver.flush();
        let failures = saver.take_failures();
        assert_eq!(failures.len(), 1, "{failures:?}");
        assert!(failures[0].starts_with(&path), "{}", failures[0]);
        assert!(saver.take_failures().is_empty());
        assert!(taken.is_dir());
        assert!(temporaries(&dir).is_empty());
        // The thread goes on with the next save as usual.
        let fine = file_in(&dir, "fine.json");
        save_text(&mut saver, &fine, "[2]").unwrap();
        saver.flush();
        assert_eq!(std::fs::read(&fine).unwrap(), b"[2]");
    }

    #[test]
    fn closing_the_saver_finishes_its_queue() {
        let dir = folder("saver-close");
        let path = file_in(&dir, "a.json");
        let mut saver = Saver::default();
        for i in 0..100 {
            save_text(&mut saver, &path, &format!("[{i}]")).unwrap();
        }
        saver.close();
        assert_eq!(std::fs::read(&path).unwrap(), b"[99]");
        assert!(temporaries(&dir).is_empty());
    }

    #[test]
    fn a_dropped_saver_still_finishes_its_queue() {
        let dir = folder("saver-drop");
        let path = file_in(&dir, "a.json");
        {
            let mut saver = Saver::default();
            for i in 0..100 {
                save_text(&mut saver, &path, &format!("[{i}]")).unwrap();
            }
        }
        // The drop doesn't wait, so give the thread its time.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::fs::read(&path).ok().as_deref() != Some(&b"[99]"[..]) {
            assert!(
                std::time::Instant::now() < deadline,
                "the queue was not finished"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        while !temporaries(&dir).is_empty() {
            assert!(
                std::time::Instant::now() < deadline,
                "a temporary file was left"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    #[test]
    fn after_close_a_save_finishes_before_it_returns() {
        let dir = folder("saver-closed");
        let path = file_in(&dir, "a.json");
        let mut saver = Saver::default();
        save_text(&mut saver, &path, "[1]").unwrap();
        saver.close();
        assert_eq!(std::fs::read(&path).unwrap(), b"[1]");
        save_text(&mut saver, &path, "[2]").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"[2]");
        let taken = dir.join("taken.json");
        std::fs::create_dir(&taken).unwrap();
        std::fs::write(taken.join("inside"), b"x").unwrap();
        assert!(save_text(&mut saver, &taken.to_string_lossy(), "[3]").is_err());
        assert!(temporaries(&dir).is_empty());
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
