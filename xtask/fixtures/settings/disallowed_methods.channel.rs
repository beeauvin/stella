// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A channel with no bound on what it holds.

use std::sync::mpsc::{Receiver, Sender, channel};

/// Returns the two ends of a channel.
#[must_use]
pub fn ends() -> (Sender<u8>, Receiver<u8>) {
    channel()
}
