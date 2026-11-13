// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Tests of the repository as a whole.
//!
//! One file keeps the reference's links honest: each link to a test leads to that test, and each
//! link to a page leads to a page. Another holds every crate to what no tool makes it do: be a
//! member of the workspace, inherit its lints, forbid `unsafe`, and carry the license. A third
//! holds together what the repository says in more than one place: the Rust it's pinned to, the
//! command that runs everything, and what Git is told to keep. A fourth holds the rules page to
//! the rules the code cites.
//!
//! They're in `tests/`, so they run with every `cargo test`. This library holds only what they
//! share: where the repository is, and how to read it.

#![forbid(unsafe_code)]

use std::env;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{self, Read as _};
use std::path::{Path, PathBuf};

/// This is the most bytes of a file a test reads. It's many times the longest page, manifest or
/// source file in the repository, and it bounds what one read can hold.
pub const TEXT_BYTES_MAX: u64 = 1024 * 1024;

/// This is the most files and directories one walk looks at. It's far more than the repository
/// holds, and it keeps a walk bounded.
pub const ENTRIES_MAX: u32 = 10_000;

/// A filter says which files a listing or a walk is looking for.
#[derive(Clone, Copy, Debug)]
pub enum Filter {
    /// Files with exactly this name.
    Named(&'static str),
    /// Files with this extension.
    WithExtension(&'static str),
}

impl Filter {
    /// Says whether the file at `path` is one the filter is looking for.
    fn takes(self, path: &Path) -> bool {
        match self {
            Self::Named(name) => path.file_name() == Some(OsStr::new(name)),
            Self::WithExtension(extension) => path.extension() == Some(OsStr::new(extension)),
        }
    }
}

/// Returns the repository's root, which holds this crate's directory.
///
/// # Panics
///
/// Panics if Cargo isn't what's running the test, or if this crate's directory isn't in the
/// root, which would be a bug in where the crate is.
#[must_use]
pub fn root() -> PathBuf {
    let manifest_directory = env::var_os("CARGO_MANIFEST_DIR");
    let manifest_directory =
        manifest_directory.expect("Cargo tells a test which crate's directory it's built from");
    let root =
        Path::new(&manifest_directory).parent().expect("this crate's directory is in the root");
    assert!(root.join("Cargo.lock").is_file(), "the root holds the workspace's lock file");
    root.to_path_buf()
}

/// Returns the text of the file at `path`, which is in the repository.
///
/// # Panics
///
/// Panics if the file can't be read as UTF-8 text, or is longer than `TEXT_BYTES_MAX`. A test
/// that can't read what it's about has nothing to say, and must not pass.
#[must_use]
pub fn text_of(path: &Path) -> String {
    let file = File::open(path);
    assert!(file.is_ok(), "{} should be a file that can be opened: {file:?}", path.display());
    let mut text = String::new();
    // One byte past the most is asked for, so a longer file is told from one that just fits.
    let read = file
        .expect("the file was opened, as just asserted")
        .take(TEXT_BYTES_MAX + 1)
        .read_to_string(&mut text);
    assert!(read.is_ok(), "{} should be UTF-8 text that can be read: {read:?}", path.display());
    let is_bounded = u64::try_from(text.len()).is_ok_and(|length| length <= TEXT_BYTES_MAX);
    assert!(is_bounded, "{} is longer than {TEXT_BYTES_MAX} bytes", path.display());
    text
}

/// Returns the workspace's members, as the root manifest lists them.
///
/// # Panics
///
/// Panics if the root manifest lists none. A scanner that silently found nothing would let
/// every test of the members pass.
#[must_use]
pub fn members() -> Vec<String> {
    let members = members_in(&text_of(&root().join("Cargo.toml")));
    assert!(!members.is_empty(), "found no members in the root manifest");
    members
}

/// Returns the members `manifest`, a workspace's manifest, lists: each string in its `members`
/// array.
#[must_use]
pub fn members_in(manifest: &str) -> Vec<String> {
    let Some((_, after_open)) = manifest.split_once("members = [") else {
        return Vec::new();
    };
    let Some((array, _)) = after_open.split_once(']') else {
        return Vec::new();
    };
    // The array's strings are between its quotes, so they're every second piece.
    array.split('"').skip(1).step_by(2).map(String::from).collect()
}

/// Returns every file directly in `directory` that `filter` takes, sorted by path. A directory
/// that isn't there holds none, so a caller needn't ask first whether it is.
///
/// # Panics
///
/// Panics if the directory is there and can't be read, or holds more than `ENTRIES_MAX` entries.
#[must_use]
pub fn files_in(directory: &Path, filter: Filter) -> Vec<PathBuf> {
    let listing = match fs::read_dir(directory) {
        Ok(listing) => listing,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Vec::new(),
        unreadable @ Err(_) => unreadable.expect("a directory of the repository can be read"),
    };
    let mut found = Vec::new();
    let mut seen_count: u32 = 0;
    for entry in listing {
        let path = entry.expect("an entry of the repository can be read").path();
        seen_count += 1;
        assert!(seen_count <= ENTRIES_MAX, "more than {ENTRIES_MAX} entries in a directory");
        if path.is_file() && filter.takes(&path) {
            found.push(path);
        }
    }
    found.sort();
    found
}

/// Returns every file under `directory` that `filter` takes, sorted by path.
///
/// It doesn't go into `target` or into anything whose name starts with `.`, where nothing
/// authored lives that a test here reads. It walks with a stack rather than recursion.
///
/// # Panics
///
/// Panics if a directory can't be read, holds a name that isn't UTF-8, or the walk passes
/// `ENTRIES_MAX`. A walk that quietly found less would let the tests that lean on it pass.
#[must_use]
pub fn files_under(directory: &Path, filter: Filter) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut directories = vec![directory.to_path_buf()];
    let mut seen_count: u32 = 0;
    while let Some(next) = directories.pop() {
        for entry in fs::read_dir(&next).expect("a directory of the repository can be read") {
            let path = entry.expect("an entry of the repository can be read").path();
            seen_count += 1;
            assert!(seen_count <= ENTRIES_MAX, "more than {ENTRIES_MAX} entries in the repository");
            let name = path.file_name().and_then(OsStr::to_str);
            let name = name.expect("everything in the repository has a name that's UTF-8");
            if name.starts_with('.') || name == "target" {
                continue;
            }
            if path.is_dir() {
                directories.push(path);
            } else if filter.takes(&path) {
                found.push(path);
            } else {
                // Any other file isn't one that was asked for.
            }
        }
    }
    found.sort();
    found
}
