// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The path lint is the first check, and it runs before the file is even read.
//!
//! The checks run in this order. A path that starts with `/`, as given, is absolute. Then one
//! leading `./` is dropped, and the rest is split on `/` into parts. No part may be `..`, no part
//! may be empty, as `//` or a trailing `/` makes, and no part may be `.`. Then every directory's
//! name must be lowercase kebab: lowercase letters and digits joined by single hyphens. Last, the
//! file's name must end in `.stella`, have something before it, and be kebab before it.

use crate::problem::{Kind, Problem};

/// A source path passed the path lint. It's relative, it stays inside the directory the compiler
/// runs in, and every name in it is lowercase kebab. Only `lint_path` makes one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourcePath<'path> {
    normalized: &'path str,
    name: &'path str,
}

impl<'path> SourcePath<'path> {
    /// Returns the path without its leading `./`, if it had one, such as `programs/hello.stella`.
    pub fn normalized(&self) -> &'path str {
        self.normalized
    }

    /// Returns the file's name without `.stella`, such as `hello`.
    pub fn name(&self) -> &'path str {
        self.name
    }
}

/// Checks the path a program was named by, before anything reads it.
pub fn lint_path(path: &str) -> Result<SourcePath<'_>, Problem> {
    if path.starts_with('/') {
        return Err(Problem::whole_file(Kind::PathAbsolute));
    }
    let normalized = path.strip_prefix("./").unwrap_or(path);
    // Each check looks at every part before the next check runs.
    if normalized.split('/').any(|part| part == "..") {
        return Err(Problem::whole_file(Kind::PathParent));
    }
    if normalized.split('/').any(str::is_empty) {
        return Err(Problem::whole_file(Kind::PathEmptyPart));
    }
    if normalized.split('/').any(|part| part == ".") {
        return Err(Problem::whole_file(Kind::PathDotPart));
    }
    // The last part is the file, and every part before it is a directory.
    let mut parts = normalized.rsplit('/');
    let file = parts.next().expect("a split has at least one part");
    if !parts.all(is_kebab) {
        return Err(Problem::whole_file(Kind::DirectoryNotKebab));
    }
    let Some(name) = file.strip_suffix(".stella") else {
        return Err(Problem::whole_file(Kind::NameNotStella));
    };
    if name.is_empty() {
        return Err(Problem::whole_file(Kind::NameEmpty));
    }
    if !is_kebab(name) {
        return Err(Problem::whole_file(Kind::NameNotKebab));
    }
    // Every part passed, so the path is kebab names joined by `/`: relative, and ASCII.
    assert!(!normalized.starts_with('/'));
    assert!(normalized.is_ascii());
    Ok(SourcePath { normalized, name })
}

/// Says whether `name` is lowercase letters and digits joined by single hyphens, like
/// `hello-world`.
fn is_kebab(name: &str) -> bool {
    // A word is needed at the start, and again after every hyphen.
    let mut word_needed = true;
    for byte in name.bytes() {
        match byte {
            b'a'..=b'z' | b'0'..=b'9' => word_needed = false,
            b'-' if !word_needed => word_needed = true,
            _ => return false,
        }
    }
    let kebab = !word_needed;
    if kebab {
        assert!(!name.starts_with('-'));
        assert!(!name.ends_with('-'));
    }
    kebab
}
