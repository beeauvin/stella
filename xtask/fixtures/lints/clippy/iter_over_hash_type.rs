// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A loop over a hash map, whose order changes from run to run.

use std::collections::HashMap;

/// Counts by name.
#[derive(Debug)]
pub struct Tally {
    counts: HashMap<String, u32>,
}

impl Tally {
    /// Returns the names, in whatever order the map gives them.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        let mut names = Vec::new();
        for name in self.counts.keys() {
            names.push(name.as_str());
        }
        names
    }
}
