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

use std::ffi::OsStr;
use std::fs;
use std::os::unix::ffi::OsStrExt;

/// This is the most entries the walk looks at, counting files and directories alike, skipped ones
/// included, but not the directory the command runs in. It's far more than a program's directory
/// holds, and it keeps the walk bounded.
pub const WALK_ENTRIES_MAX: u32 = 10_000;

/// A find is something the walk found. Its path is relative to the directory the command runs in.
#[derive(Debug)]
pub enum Found {
    /// This is a `.stella` source file.
    Source(Vec<u8>),
    /// This is a directory the walk couldn't read. The directory the command runs in is called
    /// `.`.
    Unreadable {
        /// This is the directory's path.
        directory: Vec<u8>,
        /// This is why it couldn't be read.
        error: std::io::Error,
    },
}

/// The walk looked at `WALK_ENTRIES_MAX` entries, and there were more.
#[derive(Debug)]
pub struct TooManyEntries;

/// Returns everything the walk found under the directory the command runs in, sorted by the bytes
/// of its path.
pub fn walk() -> Result<Vec<Found>, TooManyEntries> {
    let mut found = Vec::new();
    // These are the directories still to read, by their paths relative to the directory the
    // command runs in, which is the empty path.
    let mut directories: Vec<Vec<u8>> = vec![Vec::new()];
    let mut entries_count: u32 = 0;
    while let Some(directory) = directories.pop() {
        let entries = match fs::read_dir(read_dir_path(&directory)) {
            Ok(entries) => entries,
            Err(error) => {
                found.push(unreadable(directory, error));
                continue;
            }
        };
        for entry in entries {
            let (name, file_type) =
                match entry.and_then(|entry| Ok((entry.file_name(), entry.file_type()?))) {
                    Ok(named) => named,
                    Err(error) => {
                        found.push(unreadable(directory.clone(), error));
                        break;
                    }
                };
            entries_count += 1;
            if entries_count > WALK_ENTRIES_MAX {
                return Err(TooManyEntries);
            }
            let name = name.as_bytes();
            if name.starts_with(b".") {
                continue;
            }
            if name == b"target" {
                continue;
            }
            let path = joined(&directory, name);
            // A symlink is neither a directory nor a file here, so it's never followed.
            if file_type.is_dir() {
                directories.push(path);
                assert!(directories.len() <= WALK_ENTRIES_MAX as usize);
            } else if file_type.is_file() {
                if !name.ends_with(b".stella") {
                    continue;
                }
                // A source's path is relative, and no part of it was skipped.
                assert!(!path.starts_with(b"."));
                assert!(!path.starts_with(b"/"));
                found.push(Found::Source(path));
            }
        }
    }
    found.sort_by(|one, other| found_path(one).cmp(found_path(other)));
    // Some filesystems can list a name twice if it's renamed away and back while the walk reads
    // its directory, so a path found twice is kept once.
    found.dedup_by(|one, other| found_path(one) == found_path(other));
    assert!(entries_count <= WALK_ENTRIES_MAX);
    // Every find has a path of its own, in order.
    assert!(found.windows(2).all(|pair| found_path(&pair[0]) < found_path(&pair[1])));
    Ok(found)
}

/// Returns a find for `directory`, which couldn't be read for `error`. The empty path is the
/// directory the command runs in, called `.`.
fn unreadable(directory: Vec<u8>, error: std::io::Error) -> Found {
    let directory = if directory.is_empty() { b".".to_vec() } else { directory };
    assert!(!directory.starts_with(b"/"));
    Found::Unreadable { directory, error }
}

/// Returns the path to read `directory` by, where the empty path is the directory the command runs
/// in.
fn read_dir_path(directory: &[u8]) -> &OsStr {
    if directory.is_empty() { OsStr::new(".") } else { OsStr::from_bytes(directory) }
}

/// Returns the path of `name` in `directory`, where the empty path is the directory the command
/// runs in.
fn joined(directory: &[u8], name: &[u8]) -> Vec<u8> {
    assert!(!name.is_empty());
    if directory.is_empty() { name.to_vec() } else { [directory, b"/", name].concat() }
}

/// Returns the path a find is sorted by.
fn found_path(found: &Found) -> &[u8] {
    match found {
        Found::Source(path) => path,
        Found::Unreadable { directory, .. } => directory,
    }
}
