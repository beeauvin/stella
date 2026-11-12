// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Fields that all end with their unit, which `struct_field_names` would refuse.

/// The longest each stage may take.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// The longest a read may take.
    pub read_ms_max: u32,
    /// The longest a write may take.
    pub write_ms_max: u32,
    /// The longest a flush may take.
    pub flush_ms_max: u32,
}
