// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! An `if let` with an `else`, which `option_if_let_else` would turn into `map_or`.

/// Returns a greeting for `name`, or for nobody in particular when there is none.
#[must_use]
pub fn greeting(name: Option<&str>) -> String {
    if let Some(name) = name { format!("hello, {name}") } else { String::from("hello") }
}
