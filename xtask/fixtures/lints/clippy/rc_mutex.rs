// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A lock for many threads behind a pointer for one.

use std::rc::Rc;
use std::sync::Mutex;

/// A count that is shared.
#[derive(Debug)]
pub struct Shared {
    /// The count.
    pub count: Rc<Mutex<u32>>,
}
