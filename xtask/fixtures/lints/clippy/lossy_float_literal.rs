// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A float literal that the type cannot hold exactly.

/// One more than the largest whole number an `f32` holds exactly.
pub const TOO_FINE: f32 = 16_777_217.0;
