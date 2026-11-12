// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Checks the path a Stella program is named by, before anything reads the file.
//!
//! The path lint is the first check a program meets. `lint_path` takes the path as it was given
//! and returns a `SourcePath`, which only it can make, or the first problem with the path. It
//! reads the path's characters and nothing else, so it never touches the filesystem. Whether a
//! part of the path is a symlink, and what each name is stored as, is the command's to find.

#![forbid(unsafe_code)]

mod path;
mod problem;

pub use path::{SourcePath, lint_path};
pub use problem::Problem;
