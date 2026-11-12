// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A method of a type and a method of a trait it implements, with one name.

/// Something with a label.
pub trait Labelled {
    /// Returns the label.
    fn label(&self) -> &'static str;
}

/// A tag.
#[derive(Debug)]
pub struct Tag {
    label: &'static str,
}

impl Tag {
    /// Returns the label.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        self.label
    }
}

impl Labelled for Tag {
    fn label(&self) -> &'static str {
        self.label
    }
}
