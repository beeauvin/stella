// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A stop is why the command went no further with a program.

use crate::status::Status;

/// A stop is why the command went no further with a program, and the message that says so.
///
/// A program that passes says nothing, so there's no stop for it: linting one returns nothing,
/// and building one returns its report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stop {
    /// The program was refused, with this message.
    Refused(String),
    /// Something outside the program went wrong, with this message.
    Failed(String),
}

impl Stop {
    /// Returns the status the stop ends a run with.
    #[must_use]
    pub const fn status(&self) -> Status {
        match self {
            Self::Refused(_) => Status::Refused,
            Self::Failed(_) => Status::Failed,
        }
    }

    /// Returns the stop's message, every line of which ends in `\n`.
    #[must_use]
    pub fn message(&self) -> &str {
        match self {
            Self::Refused(text) | Self::Failed(text) => text,
        }
    }
}
