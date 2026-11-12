// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Tests of the repository as a whole.
//!
//! One file keeps the reference's links honest: each link to a test leads to that test, and each
//! link to a page leads to a page. Another holds every crate to what no tool makes it do: be a
//! member of the workspace, inherit its lints, forbid `unsafe`, and carry the license. A third
//! holds together what the repository says in more than one place: the Rust it's pinned to, the
//! command that runs everything, and what Git is told to keep.
//!
//! They're in `tests/`, so they run with every `cargo test`. This library holds only what they
//! share: where the repository is, and how to read it.

#![forbid(unsafe_code)]

use std::env;
use std::fs::File;
use std::io::Read as _;
use std::path::{Path, PathBuf};

/// This is the most bytes of a file a test reads. It's many times the longest page, manifest or
/// source file in the repository, and it bounds what one read can hold.
pub const TEXT_BYTES_MAX: u64 = 1024 * 1024;

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
