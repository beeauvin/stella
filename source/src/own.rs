// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! What a check made for one source is that source's own: its tokens, its nodes, its functions.
//!
//! Two sources can hold values that are equal, part for part, and still mean different things:
//! the first token of `ab {` and of `cd {` are each a name of two bytes at the start, with
//! different text. So a check that's handed back one of its values doesn't ask whether it has
//! one like it. It asks whether it's the very one it holds.

/// Says whether `element` is one of `elements` itself: the very value the slice holds, where
/// it holds it.
///
/// A copy of one isn't, and neither is a value that only equals one, or one that lies across
/// two of them. An element that takes no room has no place of its own to be told by, so this
/// doesn't build for one.
#[must_use]
pub fn is_own<Element>(elements: &[Element], element: &Element) -> bool {
    const {
        assert!(size_of::<Element>() > 0, "an element that takes no room has no place of its own");
    }
    // Nothing is read through the reference. Where it points is compared with where each
    // element starts, so a reference into the middle of one is none of them.
    elements.element_offset(element).is_some()
}
