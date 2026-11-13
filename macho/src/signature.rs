// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The code signature, which macOS checks before it runs a page of the program.
//!
//! The signature is ad hoc: no certificate, just a SHA-256 hash of each 4 KiB page of everything
//! before it, which the kernel checks as it maps each page in. It's marked as a linker's own,
//! which is what a compiler's is. Its identifier is the binary's name, so a binary is signed
//! with the name it will have.
//!
//! It's a table, the `SuperBlob`, that holds one thing, the `CodeDirectory`: a header, then the
//! name, then the hashes. Every number in it is big-endian.

use stella_source::usize_from_u32;

use crate::image::Image;
use crate::layout::{HASH_PAGE_BYTES, Layout};
use crate::name::BinaryName;
use crate::sha256::{HASH_BYTES, sha256};

/// This is how many bytes the signature takes before the name: the `SuperBlob`'s header and its
/// one entry, then the `CodeDirectory`'s header.
pub(crate) const SIGNATURE_HEADERS_BYTES: u32 = SUPER_BLOB_BYTES + DIRECTORY_HEADER_BYTES;

/// A `SuperBlob` starts with this (`CSMAGIC_EMBEDDED_SIGNATURE`).
pub(crate) const SUPER_BLOB_MAGIC: u32 = 0xFADE_0CC0;

/// The `SuperBlob`'s header is its magic, its size and how many things it holds. Its one entry is
/// a kind and an offset.
const SUPER_BLOB_BYTES: u32 = 12 + 8;

/// The `SuperBlob` holds one thing, of the kind that's a `CodeDirectory` (`CSSLOT_CODEDIRECTORY`).
const SUPER_BLOB_ENTRIES_COUNT: u32 = 1;
const ENTRY_KIND_DIRECTORY: u32 = 0;

/// A `CodeDirectory` starts with this (`CSMAGIC_CODEDIRECTORY`).
const DIRECTORY_MAGIC: u32 = 0xFADE_0C02;

/// This is how many bytes the `CodeDirectory`'s header takes, at the version written here.
const DIRECTORY_HEADER_BYTES: u32 = 88;

/// This is the version of `CodeDirectory` written: the first that says which part of the file is
/// the program's code (`CS_SUPPORTSEXECSEG`).
const DIRECTORY_VERSION: u32 = 0x0002_0400;

/// The signature is ad hoc (`CS_ADHOC`), and a linker's own (`CS_LINKER_SIGNED`).
const DIRECTORY_FLAGS: u32 = 0x2 | 0x2_0000;

/// The hashes are SHA-256 (`CS_HASHTYPE_SHA256`).
const HASH_KIND_SHA256: u8 = 2;

/// This is the size of the pages hashed, as a power of two.
const HASH_PAGE_BYTES_POWER: u8 = 12;

/// The code's segment is the main binary's (`CS_EXECSEG_MAIN_BINARY`).
const CODE_SEGMENT_MAIN_BINARY: u32 = 1;

/// This is how many bytes a hash takes, as a count the signature holds in one byte.
const HASH_BYTES_U8: u8 = 32;

const _: () = assert!(HASH_BYTES == 32, "a hash is 32 bytes, however the count is held");
const _: () = assert!(
    HASH_PAGE_BYTES == 1 << HASH_PAGE_BYTES_POWER,
    "the size of the pages hashed is the power of two the signature says it is"
);

/// Adds the signature of `image`, which holds all of `__TEXT` as `layout` lays it out, to it,
/// under the name `name`.
pub(crate) fn push_signature(image: &mut Image, name: BinaryName<'_>, layout: &Layout) {
    let start = image.offset();
    assert_eq!(start, layout.linkedit_offset(), "the signature follows `__TEXT`");
    image.push_u32s_big_endian(&[
        SUPER_BLOB_MAGIC,
        layout.signature_bytes(),
        SUPER_BLOB_ENTRIES_COUNT,
        ENTRY_KIND_DIRECTORY,
        SUPER_BLOB_BYTES,
    ]);
    assert_eq!(image.offset() - start, SUPER_BLOB_BYTES, "the SuperBlob's header and its entry");
    push_signature_directory_header(image, name, layout);
    assert_eq!(image.offset() - start, SIGNATURE_HEADERS_BYTES, "the name follows the headers");
    image.push_bytes(name.as_str().as_bytes());
    image.push_bytes(&[0]);
    // Each page is hashed as it's in the image by now, and the signature isn't in any of them.
    let page_bytes = usize_from_u32(HASH_PAGE_BYTES);
    let text_bytes = usize_from_u32(layout.text_bytes());
    let (pages, after) =
        image.as_slice()[..text_bytes].as_chunks::<{ usize_from_u32(HASH_PAGE_BYTES) }>();
    assert!(after.is_empty(), "`__TEXT` is whole pages of {page_bytes} bytes");
    assert_eq!(pages.len(), usize_from_u32(layout.hash_pages_count()), "a hash for each page");
    let hashes: Vec<[u8; HASH_BYTES]> = pages.iter().map(|page| sha256(page)).collect();
    for hash in &hashes {
        image.push_bytes(hash);
    }
    assert_eq!(image.offset() - start, layout.signature_bytes(), "the signature is as laid out");
}

/// Adds the `CodeDirectory`'s header for `layout` to `image`, for a binary named `name`.
fn push_signature_directory_header(image: &mut Image, name: BinaryName<'_>, layout: &Layout) {
    let start = image.offset();
    // The name comes right after the header, then a 0, then the hashes.
    let name_offset = DIRECTORY_HEADER_BYTES;
    let hashes_offset = name_offset + name.size_bytes() + 1;
    let (special_hashes_count, code_limit) = (0, layout.text_bytes());
    image.push_u32s_big_endian(&[
        DIRECTORY_MAGIC,
        layout.signature_bytes() - SUPER_BLOB_BYTES,
        DIRECTORY_VERSION,
        DIRECTORY_FLAGS,
        hashes_offset,
        name_offset,
        special_hashes_count,
        layout.hash_pages_count(),
        code_limit,
    ]);
    // No platform is named, which is what any program but the system's own says.
    let platform = 0;
    image.push_bytes(&[HASH_BYTES_U8, HASH_KIND_SHA256, platform, HASH_PAGE_BYTES_POWER]);
    // Nothing is scattered and no team signed it, and the code's limit fits in 32 bits, so the
    // 64-bit one isn't used. The code's segment is `__TEXT`: from the file's start, all of it.
    let (unused, code_segment_offset) = (0, 0);
    image.push_u32s_big_endian(&[unused, unused, unused, unused, unused, unused]);
    image.push_u32s_big_endian(&[unused, code_segment_offset, unused, layout.text_bytes()]);
    image.push_u32s_big_endian(&[unused, CODE_SEGMENT_MAIN_BINARY]);
    assert_eq!(image.offset() - start, DIRECTORY_HEADER_BYTES, "the header is 88 bytes");
}
