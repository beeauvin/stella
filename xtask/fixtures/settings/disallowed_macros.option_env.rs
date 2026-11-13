// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A variable read from the environment while the program is compiled.

/// What the variable held when this was compiled.
pub const HELD: Option<&str> = option_env!("STELLA_LINT_FIXTURE");
