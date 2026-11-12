// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A count is how many of something a source holds, or how far into it something is.
//!
//! The checks hold a count as a `u32`, which keeps what they make small, and index memory with a
//! `usize`. These are the two ways between them, so no check converts with a bare `as`.

/// This is the largest source file, in bytes. Anything larger is refused before its characters
/// are read.
pub const SOURCE_BYTES_MAX: u32 = 16_384;

const _: () =
    assert!(u32::BITS <= usize::BITS, "the compiler builds only where a `usize` holds every `u32`");

/// Returns `count` as a `usize`, to index or size memory with.
#[expect(
    clippy::as_conversions,
    reason = "TS-SIZED-TYPES: `try_from` can't be called in a `const fn`, and the assertion above \
              proves a `usize` holds every `u32`"
)]
#[must_use]
pub const fn usize_from_u32(count: u32) -> usize {
    count as usize
}

/// Returns `count`, which counts something within one source, as a `u32`.
///
/// # Panics
///
/// Panics if `count` is more than `SOURCE_BYTES_MAX`. Nothing within a source is: every byte,
/// character, token and line takes at least one of its bytes.
#[must_use]
pub fn u32_from_usize(count: usize) -> u32 {
    assert!(
        count <= usize_from_u32(SOURCE_BYTES_MAX),
        "a count within a source is at most its size, and {count} is more"
    );
    u32::try_from(count).expect("a count within a source fits in a `u32`")
}
