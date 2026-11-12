// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The walk finds every source file `stella lint` checks when it's given no path.
//!
//! It looks through the directory the command runs in and every directory under it, and finds each
//! regular file whose name ends in `.stella`. It skips anything whose name starts with `.`, such
//! as `.git` and `.build`. It also skips directories named `target`: the compiler is written in
//! Rust, which builds into `target`, and no Stella source goes there. It never goes into what it
//! skips, and it doesn't follow symlinks, so it can't loop.
//!
//! A directory it can't read is found too, so the problem can be reported while every other file
//! is still checked.
//!
//! The walk holds one path for each find and each directory still to read. Each of those was
//! counted as an entry, so there are never more than `WALK_ENTRIES_MAX` of them, and one for the
//! directory the walk starts in. A path is as long as the filesystem lets one be: on macOS,
//! 1,024 bytes for a directory the walk can read, and 255 more for a name in it.

use std::ffi::OsStr;
use std::fmt;
use std::os::unix::ffi::OsStrExt as _;
use std::path::{Path, PathBuf};

use stella_source::usize_from_u32;

use crate::filesystem::{Reason, list};

/// This is the most entries the walk looks at.
///
/// It counts files and directories alike, skipped ones included, but not the directory the walk
/// starts in. It's far more than a program's directory holds, and it keeps the walk bounded.
pub const WALK_ENTRIES_MAX: u32 = 10_000;

const _: () = assert!(
    WALK_ENTRIES_MAX == 10_000,
    "`TooManyEntries`'s sentence names the bound, so a new bound needs a new sentence"
);

/// A find is something the walk found. Its path is relative to the directory the walk started in.
#[derive(Debug)]
pub enum Find {
    /// This is a `.stella` source file, by its path as it's stored.
    Source(Vec<u8>),
    /// This is a directory the walk couldn't read. The directory the walk started in is called
    /// `.`.
    Unreadable {
        /// This is the directory's path.
        directory: Vec<u8>,
        /// This is why it couldn't be read.
        error: Reason,
    },
}

impl Find {
    /// Returns the find's path, which it's sorted by.
    fn path(&self) -> &[u8] {
        match self {
            Self::Source(path) => path,
            Self::Unreadable { directory, .. } => directory,
        }
    }
}

/// The walk looked at `WALK_ENTRIES_MAX` entries, and there were more.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TooManyEntries;

/// It displays as the sentence that says so.
impl fmt::Display for TooManyEntries {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("there are more than 10,000 files and directories under this directory.")
    }
}

/// Returns everything the walk finds under `root`, the directory the command runs in, sorted by
/// the bytes of its path.
///
/// # Errors
///
/// Returns `TooManyEntries` when there are more than `WALK_ENTRIES_MAX` entries to look at.
///
/// # Panics
///
/// Never, whatever the directory holds. The assertions check the walk's own work.
pub fn walk(root: &Path) -> Result<Vec<Find>, TooManyEntries> {
    let mut finds = Vec::new();
    // These are the directories still to read, by their paths relative to the root, which is the
    // empty path.
    let mut directories: Vec<Vec<u8>> = vec![Vec::new()];
    let mut entries_count: u32 = 0;
    while let Some(directory) = directories.pop() {
        let entries = match list(&directory_path(root, &directory)) {
            Ok(entries) => entries,
            Err(error) => {
                finds.push(unreadable(directory, error));
                continue;
            }
        };
        for entry in entries {
            let (name, file_type) =
                match entry.and_then(|entry| Ok((entry.name(), entry.file_type()?))) {
                    Ok(named) => named,
                    Err(error) => {
                        finds.push(unreadable(directory.clone(), error));
                        break;
                    }
                };
            entries_count += 1;
            if entries_count > WALK_ENTRIES_MAX {
                return Err(TooManyEntries);
            }
            let name = name.as_bytes();
            if name.starts_with(b".") || name == b"target" {
                continue;
            }
            let path = Entry { directory: &directory, name }.path();
            if file_type.is_dir() {
                directories.push(path);
                let bounded = directories.len() <= usize_from_u32(WALK_ENTRIES_MAX);
                assert!(bounded, "every directory still to read was counted as an entry");
                continue;
            }
            #[expect(
                clippy::filetype_is_file,
                reason = "TS-STRICT-LINTS: only a regular file is a source. A symlink, a FIFO or \
                          a socket is neither a directory nor a file here, so it's never followed"
            )]
            let is_source = file_type.is_file() && name.ends_with(b".stella");
            if is_source {
                assert!(!path.starts_with(b"."), "every name that starts with `.` was skipped");
                assert!(!path.starts_with(b"/"), "a find's path is relative to the root");
                finds.push(Find::Source(path));
            }
        }
    }
    finds.sort_by(|one, other| one.path().cmp(other.path()));
    // Some filesystems can list a name twice if it's renamed away and back while the walk reads
    // its directory, so a path found twice is kept once.
    finds.dedup_by(|one, other| one.path() == other.path());
    assert!(entries_count <= WALK_ENTRIES_MAX, "the walk stopped at the bound");
    let in_order = finds.is_sorted_by(|one, other| one.path() < other.path());
    assert!(in_order, "every find has a path of its own, and they're sorted");
    assert!(
        finds.len() <= usize_from_u32(WALK_ENTRIES_MAX) + 1,
        "every find was counted as an entry, but for the directory the walk started in"
    );
    Ok(finds)
}

/// Returns a find for `directory`, which couldn't be read for `error`. The empty path is the
/// root, called `.`.
fn unreadable(directory: Vec<u8>, error: Reason) -> Find {
    let directory = if directory.is_empty() { b".".to_vec() } else { directory };
    assert!(!directory.starts_with(b"/"), "a find's path is relative to the root");
    Find::Unreadable { directory, error }
}

/// Returns the path to read `directory` by, which is relative to `root`. The empty path is the
/// root itself.
fn directory_path(root: &Path, directory: &[u8]) -> PathBuf {
    if directory.is_empty() { root.to_path_buf() } else { root.join(OsStr::from_bytes(directory)) }
}

/// An entry is a name in a directory the walk is reading. Each is named, so the two can't be
/// given the wrong way round.
#[derive(Clone, Copy)]
struct Entry<'walk> {
    /// The directory's path, relative to the root, where the empty path is the root.
    directory: &'walk [u8],
    /// The entry's name in the directory.
    name: &'walk [u8],
}

impl Entry<'_> {
    /// Returns the entry's path, relative to the root. The name is whatever the filesystem
    /// listed, so nothing is asserted of it.
    fn path(self) -> Vec<u8> {
        if self.directory.is_empty() {
            self.name.to_vec()
        } else {
            [self.directory, b"/", self.name].concat()
        }
    }
}
