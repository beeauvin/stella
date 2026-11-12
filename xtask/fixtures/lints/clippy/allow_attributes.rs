// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A lint switched off with `#[allow]`, where `#[expect]` would notice if it stopped firing.

/// Returns `count`, unchanged.
#[allow(clippy::as_conversions, reason = "the fixture is about the attribute")]
#[must_use]
pub const fn same(count: u32) -> u32 {
    count
}
