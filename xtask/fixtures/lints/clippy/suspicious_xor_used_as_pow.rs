// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A `^` between two literals, which is not a power.

/// Not sixty-five thousand and something.
pub const BYTES: u32 = 2 ^ 16;
