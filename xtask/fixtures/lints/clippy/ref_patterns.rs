// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A `ref` in a pattern.

/// Something that may have a name.
#[derive(Debug)]
pub struct Named {
    name: Option<String>,
}

impl Named {
    /// Returns how long the name is, or none when there is no name.
    #[must_use]
    pub const fn length(&self) -> usize {
        let Some(ref name) = self.name else {
            return 0;
        };
        name.len()
    }
}
