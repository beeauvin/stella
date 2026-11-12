// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A shared pointer cloned with `.clone()`, which reads like a copy of what it points at.

use std::sync::Arc;

/// Returns another pointer to what `shared` points at.
#[must_use]
pub fn another(shared: &Arc<u32>) -> Arc<u32> {
    shared.clone()
}
