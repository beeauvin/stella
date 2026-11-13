// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A pointer formatted as its address, which is different on every run.

/// Returns where `text` is, as text.
#[must_use]
pub fn place(text: &str) -> String {
    format!("{text:p}")
}
