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

use crate::problem::Problem;

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
    todo!("lint the path {path:?}")
}
