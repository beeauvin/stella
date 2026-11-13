// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A block statement with its semicolon outside the block.

/// Adds a one to `items`, then takes it away again.
pub fn touch(items: &mut Vec<u32>) {
    {
        items.push(1);
        items.pop()
    };
}
