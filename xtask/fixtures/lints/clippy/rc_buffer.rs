// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A shared pointer to a buffer that can grow, which nobody can then grow.

use std::rc::Rc;

/// Text that is shared.
#[derive(Debug)]
pub struct Shared {
    /// The text.
    pub text: Rc<String>,
}
