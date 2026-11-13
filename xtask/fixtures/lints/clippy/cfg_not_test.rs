// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Code that is compiled out of the tests.

/// The most of anything.
#[cfg(not(test))]
pub const LIMIT: u32 = 8;
