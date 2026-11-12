// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Checks of the repository as a whole.
//!
//! One keeps the reference's links honest: each link to a test leads to that test, and each link
//! to a page leads to a page. Another holds every crate to what no tool makes it do: be a member
//! of the workspace, inherit its lints, forbid `unsafe`, and carry the license. The crate has no
//! code of its own. The checks are in `tests/`, so they run with every `cargo test`.

#![forbid(unsafe_code)]
