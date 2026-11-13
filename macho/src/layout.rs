// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A layout is where everything goes, in the file and in memory.
//!
//! The file starts with its header and load commands, and the code follows them, starting in the
//! first of the `__TEXT` segment's 16 KiB pages. `__TEXT` can be read and run, never written.
//! Above it in memory is `__DATA`, which holds Stella's stack at its bottom: a push past the
//! stack's lowest byte lands in `__TEXT` and faults, rather than writing over anything. `__DATA`
//! takes no room in the file, since it starts as zeros. Last is `__LINKEDIT`, which holds the
//! code signature.
//!
//! `layout` works out where each segment goes and how big it is, and where the code and the
//! stack's top are in memory. There are no symbols and nothing to fix up when the program is
//! loaded, since every address the code needs is a number worked out here. Where things go
//! inside the load commands and inside the signature is for the modules that write those.

use stella_arm64::{
    CODE_BYTES_MAX, INSTRUCTION_BYTES, PAGE_ADDRESS_PAGE_BYTES, PAGE_ADDRESS_PAGES_MAX,
    STACK_ALIGNMENT_BYTES,
};

use crate::commands::{COMMANDS_BYTES, HEADER_BYTES};
use crate::name::NAME_BYTES_MAX;
use crate::sha256::HASH_BYTES;
use crate::signature::SIGNATURE_HEADERS_BYTES;

/// This is the size of the pages macOS maps on arm64: 16 KiB. Each segment starts on one.
pub const PAGE_BYTES: u32 = 0x4000;

/// This is the size of the pages the signature hashes: 4 KiB.
pub const HASH_PAGE_BYTES: u32 = 0x1000;

/// This is where `__TEXT` starts in memory, just above the 4 GiB that `__PAGEZERO` keeps
/// unmapped.
pub const BASE_ADDRESS: u64 = 0x1_0000_0000;

/// This is the most stack a program can have: 1 GiB, so the code always reaches it.
pub const STACK_BYTES_MAX: u32 = 1 << 30;

/// This is where the code starts in the file, just after the load commands.
pub const CODE_OFFSET: u32 = HEADER_BYTES + COMMANDS_BYTES;

/// This is the most room `__TEXT` takes: the header, the commands and the most code, in whole
/// pages.
pub const TEXT_BYTES_MAX: u32 = (CODE_OFFSET + CODE_BYTES_MAX).next_multiple_of(PAGE_BYTES);

/// This is the most room the signature takes: its headers, the longest name and its 0, and one
/// hash for each 4 KiB of the most `__TEXT`.
pub const SIGNATURE_BYTES_MAX: u32 = SIGNATURE_HEADERS_BYTES
    + NAME_BYTES_MAX
    + 1
    + HASH_BYTES_U32 * (TEXT_BYTES_MAX / HASH_PAGE_BYTES);

/// This is the most bytes a file takes: the most `__TEXT`, then the most signature, padded to
/// `LINKEDIT_ALIGNMENT_BYTES`.
pub const IMAGE_BYTES_MAX: u32 =
    TEXT_BYTES_MAX + SIGNATURE_BYTES_MAX.next_multiple_of(LINKEDIT_ALIGNMENT_BYTES);

/// `__LINKEDIT`'s size in the file is a multiple of this many bytes.
const LINKEDIT_ALIGNMENT_BYTES: u32 = 16;

/// This is how many bytes a hash takes, as a count that's held.
const HASH_BYTES_U32: u32 = 32;

const _: () = assert!(HASH_BYTES == 32, "a hash is 32 bytes, however the count is held");
const _: () = assert!(
    CODE_OFFSET.is_multiple_of(INSTRUCTION_BYTES),
    "the code starts on an instruction, in the file and so in memory"
);
const _: () = assert!(
    PAGE_BYTES.is_multiple_of(HASH_PAGE_BYTES),
    "the pages the signature hashes fit the pages macOS maps exactly"
);
const _: () = assert!(PAGE_BYTES.is_power_of_two(), "a page's size is a power of two");
const _: () = assert!(
    BASE_ADDRESS.trailing_zeros() >= PAGE_BYTES.trailing_zeros(),
    "`__TEXT` starts on a page"
);
const _: () = assert!(
    (TEXT_BYTES_MAX + STACK_BYTES_MAX + PAGE_BYTES) / PAGE_ADDRESS_PAGE_BYTES
        <= PAGE_ADDRESS_PAGES_MAX.unsigned_abs(),
    "the code always reaches the stack's top: the most `__TEXT` and the most stack, with a \
     page to spare, are within `adrp`'s reach"
);

/// The sizes a layout is worked out from. Each is named, so they can't be given the wrong way
/// round.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sizes {
    /// How many bytes the code takes: whole instructions, at least one, at most
    /// `CODE_BYTES_MAX`.
    pub code_bytes: u32,
    /// How many bytes the stack takes: a multiple of `STACK_ALIGNMENT_BYTES`, at most
    /// `STACK_BYTES_MAX`.
    pub stack_bytes: u32,
    /// How many bytes the binary's name takes: at least one, at most `NAME_BYTES_MAX`.
    pub name_bytes: u32,
}

/// A layout is where everything goes, in the file and in memory, for code and a stack of given
/// sizes and a name of a given length.
///
/// Only `layout` makes one, so every number in a layout that's held agrees with the others.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    code_address: u64,
    code_bytes: u32,
    text_bytes: u32,
    data_bytes: u32,
    stack_bytes: u32,
    linkedit_bytes: u32,
    signature_bytes: u32,
}

impl Layout {
    /// Returns where the code is in memory. In the file it's at `CODE_OFFSET`.
    #[must_use]
    pub const fn code_address(&self) -> u64 {
        self.code_address
    }

    /// Returns how many bytes the code takes.
    #[must_use]
    pub const fn code_bytes(&self) -> u32 {
        self.code_bytes
    }

    /// Returns how much room `__TEXT` takes, in the file and in memory: whole pages.
    #[must_use]
    pub const fn text_bytes(&self) -> u32 {
        self.text_bytes
    }

    /// Returns where `__DATA` starts in memory, just after `__TEXT`. The stack is at its bottom.
    #[must_use]
    pub fn data_address(&self) -> u64 {
        BASE_ADDRESS + u64::from(self.text_bytes)
    }

    /// Returns how much room `__DATA` takes in memory: whole pages, at least one.
    #[must_use]
    pub const fn data_bytes(&self) -> u32 {
        self.data_bytes
    }

    /// Returns how many bytes the stack takes.
    #[must_use]
    pub const fn stack_bytes(&self) -> u32 {
        self.stack_bytes
    }

    /// Returns the address just past the stack's highest byte, where `sp` starts.
    #[must_use]
    pub fn stack_top_address(&self) -> u64 {
        self.data_address() + u64::from(self.stack_bytes)
    }

    /// Returns where `__LINKEDIT` starts in memory, just after `__DATA`.
    #[must_use]
    pub fn linkedit_address(&self) -> u64 {
        self.data_address() + u64::from(self.data_bytes)
    }

    /// Returns where `__LINKEDIT` starts in the file, just after `__TEXT`. The signature starts
    /// there too.
    #[must_use]
    pub const fn linkedit_offset(&self) -> u32 {
        self.text_bytes
    }

    /// Returns how many bytes `__LINKEDIT` takes in the file: the signature, padded. It ends the
    /// file, and it's the size the signature's command gives.
    #[must_use]
    pub const fn linkedit_bytes(&self) -> u32 {
        self.linkedit_bytes
    }

    /// Returns how many bytes the signature itself takes, which is the size it says it is.
    #[must_use]
    pub const fn signature_bytes(&self) -> u32 {
        self.signature_bytes
    }

    /// Returns how many 4 KiB pages the signature hashes: all of `__TEXT`.
    #[must_use]
    pub const fn hash_pages_count(&self) -> u32 {
        self.text_bytes / HASH_PAGE_BYTES
    }

    /// Returns how many bytes the whole file takes.
    #[must_use]
    pub const fn image_bytes(&self) -> u32 {
        self.text_bytes + self.linkedit_bytes
    }
}

/// Returns where everything goes for `sizes`.
///
/// # Panics
///
/// Panics if a size is one no program has: code that isn't whole instructions, or is none, or is
/// more than `CODE_BYTES_MAX`; a stack that isn't aligned, or is more than `STACK_BYTES_MAX`; a
/// name of no bytes, or of more than `NAME_BYTES_MAX`. The compiler works each size out, so any
/// of those is a bug in it.
#[must_use]
pub fn layout(sizes: Sizes) -> Layout {
    let Sizes { code_bytes, stack_bytes, name_bytes } = sizes;
    assert!(code_bytes >= INSTRUCTION_BYTES, "the code is at least one instruction");
    assert!(code_bytes.is_multiple_of(INSTRUCTION_BYTES), "the code is whole instructions");
    assert!(code_bytes <= CODE_BYTES_MAX, "the code is at most `CODE_BYTES_MAX` bytes");
    assert!(stack_bytes.is_multiple_of(STACK_ALIGNMENT_BYTES), "the stack keeps `sp` aligned");
    assert!(stack_bytes <= STACK_BYTES_MAX, "the stack is at most `STACK_BYTES_MAX` bytes");
    assert!(name_bytes >= 1, "a binary's name is at least one byte");
    assert!(name_bytes <= NAME_BYTES_MAX, "a binary's name is at most `NAME_BYTES_MAX` bytes");
    let text_bytes = (CODE_OFFSET + code_bytes).next_multiple_of(PAGE_BYTES);
    // `__DATA` is at least a page, so the stack's top is mapped even when the stack is empty.
    let data_bytes = stack_bytes.next_multiple_of(PAGE_BYTES).max(PAGE_BYTES);
    let hashes_bytes = HASH_BYTES_U32 * (text_bytes / HASH_PAGE_BYTES);
    // The name is followed by a 0, which ends it.
    let signature_bytes = SIGNATURE_HEADERS_BYTES + name_bytes + 1 + hashes_bytes;
    let linkedit_bytes = signature_bytes.next_multiple_of(LINKEDIT_ALIGNMENT_BYTES);
    let layout = Layout {
        code_address: BASE_ADDRESS + u64::from(CODE_OFFSET),
        code_bytes,
        text_bytes,
        data_bytes,
        stack_bytes,
        linkedit_bytes,
        signature_bytes,
    };
    assert_laid_out(&layout);
    layout
}

/// Asserts that `layout` holds together: the segments one after another, each on a page, the
/// stack inside `__DATA`, the code inside `__TEXT`, the hashes covering `__TEXT` exactly, and
/// the file within its room.
fn assert_laid_out(layout: &Layout) {
    let page_bytes = u64::from(PAGE_BYTES);
    assert!(layout.text_bytes.is_multiple_of(PAGE_BYTES), "`__TEXT` is whole pages");
    assert!(layout.text_bytes <= TEXT_BYTES_MAX, "`__TEXT` is at most `TEXT_BYTES_MAX`");
    assert!(layout.data_address().is_multiple_of(page_bytes), "`__DATA` starts on a page");
    assert!(layout.data_bytes >= PAGE_BYTES, "`__DATA` is at least a page");
    assert!(layout.linkedit_address().is_multiple_of(page_bytes), "`__LINKEDIT` starts on a page");
    let stack_in_data = layout.stack_top_address() <= layout.linkedit_address();
    assert!(stack_in_data, "the stack's top is at most the end of `__DATA`");
    assert!(layout.code_address > BASE_ADDRESS, "the code follows the header and the commands");
    let code_end = layout.code_address + u64::from(layout.code_bytes);
    assert!(code_end <= layout.data_address(), "the code is in `__TEXT`");
    let hashed_bytes = layout.hash_pages_count() * HASH_PAGE_BYTES;
    assert_eq!(hashed_bytes, layout.text_bytes, "the hashes cover `__TEXT` exactly");
    let signature_fits = layout.signature_bytes <= layout.linkedit_bytes;
    assert!(signature_fits, "the signature is in `__LINKEDIT`");
    assert!(layout.signature_bytes <= SIGNATURE_BYTES_MAX, "the signature is within its most");
    assert!(layout.image_bytes() <= IMAGE_BYTES_MAX, "the file is at most `IMAGE_BYTES_MAX`");
}
