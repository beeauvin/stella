// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! An `unsafe impl`, which is unsafe code.

/// A marker that holds nothing.
#[derive(Debug)]
pub struct Marker;

// SAFETY: the marker holds nothing, so sending it to another thread shares nothing.
unsafe impl Send for Marker {}
