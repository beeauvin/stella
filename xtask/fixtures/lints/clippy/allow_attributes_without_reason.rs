// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A lint expected with no reason given.

/// The most of anything.
#[expect(clippy::unseparated_literal_suffix)]
pub const LIMIT: u32 = 8u32;
