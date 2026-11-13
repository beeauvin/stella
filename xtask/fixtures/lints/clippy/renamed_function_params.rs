// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A trait method implemented with another name for its parameter.

/// Something that can be scaled.
pub trait Scale {
    /// Returns this, `factor` times over.
    fn scaled(&self, factor: u32) -> u32;
}

/// A count.
#[derive(Debug)]
pub struct Count {
    count: u32,
}

impl Scale for Count {
    fn scaled(&self, times: u32) -> u32 {
        self.count.wrapping_mul(times)
    }
}
