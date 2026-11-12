// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A file read into the program as bytes while it is compiled.

/// This very file.
pub const SOURCE: &[u8] = include_bytes!("disallowed_macros.include_bytes.rs");
