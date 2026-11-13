// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! An image is the bytes of a file as they're written, from the first to the last.
//!
//! Everything is added at the end, in the order it's in the file, so a number's offset is how
//! many bytes came before it. The one exception is `fill`, which writes over zeros left for it.
//! The room for the whole file is made once, from its layout, and never grows. Most of a Mach-O
//! file is little-endian. Its code signature is big-endian, as the kernel reads it.

use stella_source::usize_from_u32;

use crate::layout::IMAGE_BYTES_MAX;

/// An image is a file's bytes so far.
#[derive(Debug)]
pub(crate) struct Image {
    bytes: Vec<u8>,
    /// The room the bytes were given when the image was made, which they never outgrow.
    capacity_bytes: usize,
}

impl Image {
    /// Returns an image with nothing in it yet, and room for a file of `image_bytes`.
    pub(crate) fn with_room_for(image_bytes: u32) -> Self {
        assert!(image_bytes <= IMAGE_BYTES_MAX, "a file is at most `IMAGE_BYTES_MAX` bytes");
        let bytes = Vec::with_capacity(usize_from_u32(image_bytes));
        let capacity_bytes = bytes.capacity();
        Self { bytes, capacity_bytes }
    }

    /// Returns the bytes so far.
    pub(crate) const fn as_slice(&self) -> &[u8] {
        self.bytes.as_slice()
    }

    /// Returns the image's bytes, now that every one is written.
    pub(crate) fn into_bytes(self) -> Vec<u8> {
        assert_eq!(self.bytes.capacity(), self.capacity_bytes, "the room for a file never grows");
        self.bytes
    }

    /// Returns how many bytes there are so far, which is the file offset of the next one.
    pub(crate) fn offset(&self) -> u32 {
        let within = self.bytes.len() <= usize_from_u32(IMAGE_BYTES_MAX);
        assert!(within, "a file is at most `IMAGE_BYTES_MAX` bytes");
        u32::try_from(self.bytes.len()).expect("at most `IMAGE_BYTES_MAX` fits in a `u32`")
    }

    /// Adds `bytes` as they are.
    pub(crate) fn push_bytes(&mut self, bytes: &[u8]) {
        let room = self.capacity_bytes - self.bytes.len();
        assert!(bytes.len() <= room, "a file's layout gives room for every byte written to it");
        self.bytes.extend_from_slice(bytes);
    }

    /// Adds each of `values`, little-endian.
    pub(crate) fn push_u32s(&mut self, values: &[u32]) {
        for value in values {
            self.push_bytes(&value.to_le_bytes());
        }
    }

    /// Adds each of `values`, little-endian.
    pub(crate) fn push_u64s(&mut self, values: &[u64]) {
        for value in values {
            self.push_bytes(&value.to_le_bytes());
        }
    }

    /// Adds each of `values`, big-endian, as the code signature holds its numbers.
    pub(crate) fn push_u32s_big_endian(&mut self, values: &[u32]) {
        for value in values {
            self.push_bytes(&value.to_be_bytes());
        }
    }

    /// Adds `text` and then zeros, `size_bytes` in all, with at least one zero to end the text.
    pub(crate) fn push_padded_text(&mut self, text: &str, size_bytes: u32) {
        assert!(!text.is_empty(), "a name in a file is at least one byte");
        let size_bytes = usize_from_u32(size_bytes);
        assert!(text.len() < size_bytes, "a name in a file has room for the zero that ends it");
        let end = self.bytes.len() + size_bytes;
        self.push_bytes(text.as_bytes());
        self.pad_to(u32::try_from(end).expect("a file's offsets fit in a `u32`"));
    }

    /// Adds zeros until the next byte's offset is `offset`.
    pub(crate) fn pad_to(&mut self, offset: u32) {
        let offset = usize_from_u32(offset);
        assert!(offset >= self.bytes.len(), "padding only ever adds bytes");
        assert!(offset <= self.capacity_bytes, "a file's layout gives room for its padding");
        self.bytes.resize(offset, 0);
    }

    /// Writes `bytes` over what's at `offset`, which was written as zeros to be filled in.
    pub(crate) fn fill(&mut self, offset: u32, bytes: &[u8]) {
        let start = usize_from_u32(offset);
        let end = start + bytes.len();
        assert!(end <= self.bytes.len(), "what's filled in was written before");
        let place = &mut self.bytes[start..end];
        assert!(place.iter().all(|&byte| byte == 0), "what's filled in was left as zeros");
        place.copy_from_slice(bytes);
    }
}
