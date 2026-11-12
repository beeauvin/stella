// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Why a file couldn't be reached.
//!
//! Finding a path as it's stored and reading a file both ask the filesystem about a path, and it
//! can fail them the same two ways. This is the one place that says which, and in what words.

use std::fmt;
use std::io;

use crate::message::reason;

/// Unreached says why the filesystem couldn't give what's at a path.
#[derive(Debug)]
pub enum Unreached {
    /// Nothing is at the path.
    Missing,
    /// What's at the path couldn't be looked at, opened or read.
    Unreadable(io::Error),
}

impl Unreached {
    /// Returns why, from the `error` the filesystem gave when the path was looked at.
    pub(crate) fn from_looking(error: io::Error) -> Self {
        match error.kind() {
            io::ErrorKind::NotFound => Self::Missing,
            _ => Self::Unreadable(error),
        }
    }
}

/// It displays as the sentence that says what's wrong.
impl fmt::Display for Unreached {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing => f.write_str("this file doesn't exist."),
            Self::Unreadable(error) => write!(f, "this file can't be read: {}.", reason(error)),
        }
    }
}
