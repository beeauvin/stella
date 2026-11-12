// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! An enum with one variant of 150 bytes beside a small one.

/// A message, short or long.
#[derive(Debug)]
pub enum Message {
    /// A short message.
    Short(u8),
    /// A long message.
    Long([u8; 150]),
}
