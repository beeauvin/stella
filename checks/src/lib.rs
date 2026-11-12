// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Checks of the repository as a whole.
//!
//! One keeps the reference's links honest: each link to a test leads to that test, and each link
//! to a page leads to a page. Another holds every crate to what no tool makes it do: be a member
//! of the workspace, inherit its lints, forbid `unsafe`, and carry the license. A third holds
//! together what the repository says in more than one place: the Rust it's pinned to, the
//! command that runs every check, and what Git is told to keep.
//!
//! The checks are in `tests/`, so they run with every `cargo test`. This library holds only what
//! they share: where the repository is, and how to read it.

#![forbid(unsafe_code)]

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// Returns the repository's root, which holds this crate's directory.
///
/// # Panics
///
/// Panics if Cargo isn't what's running the check, or if this crate's directory isn't in the
/// root, which would be a bug in where the crate is.
#[must_use]
pub fn root() -> PathBuf {
    let manifest_directory = env::var_os("CARGO_MANIFEST_DIR");
    let manifest_directory =
        manifest_directory.expect("Cargo tells a test which crate's directory it's built from");
    let root = Path::new(&manifest_directory).parent().expect("checks/ is in the root");
    assert!(root.join("Cargo.lock").is_file(), "the root holds the workspace's lock file");
    root.to_path_buf()
}

/// Returns the text of the file at `path`, which is in the repository.
///
/// # Panics
///
/// Panics if the file can't be read as UTF-8 text. A check that can't read what it checks has
/// nothing to say, and must not pass.
#[must_use]
pub fn text_of(path: &Path) -> String {
    let text = fs::read_to_string(path);
    assert!(text.is_ok(), "{} should be UTF-8 text that can be read: {text:?}", path.display());
    text.expect("the file was read, as just asserted")
}

/// Returns the workspace's members, as the root manifest lists them.
///
/// # Panics
///
/// Panics if the root manifest lists none. A scanner that silently found nothing would let
/// every check of the members pass.
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
