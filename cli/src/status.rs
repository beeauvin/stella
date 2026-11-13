// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A status is how a run of the command ends.

/// A status is how a run of the command ends, and what the process exits with.
///
/// They're in order of how bad they are, so of two statuses the worse is the greater. Linting
/// every program ends with the worst status any of them gets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    /// Every program the command checked passed, or the program was built.
    Passed,
    /// A program was refused.
    Refused,
    /// Something outside the program went wrong.
    Failed,
}

impl Status {
    /// Returns what the process exits with: 0 when every program passed or the program was
    /// built, 1 when one was refused, and 2 when something outside the program went wrong.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Passed => 0,
            Self::Refused => 1,
            Self::Failed => 2,
        }
    }
}
