// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Writing puts a program's code in a Mach-O executable that macOS 27 runs, byte by byte, with
//! no assembler and no linker.
//!
//! The file is written from its first byte to its last: the header and the load commands, the
//! code, placed where the layout put it, zeros to the end of `__TEXT`, and then the signature.
//! The one thing filled in afterwards is the UUID. It's taken from the SHA-256 of `__TEXT`,
//! with the UUID itself left as zeros, followed by the binary's name. So the same program and
//! name always make the same bytes, and binaries of two names never share a UUID.

use stella_arm64::{Code, Placement};
use stella_source::usize_from_u32;

use crate::commands::{
    COMMAND_KINDS, COMMANDS_BYTES, COMMANDS_COUNT, HEADER_BYTES, MAGIC, UUID_BYTES, UUID_OFFSET,
    push_header_and_commands,
};
use crate::image::Image;
use crate::layout::{CODE_OFFSET, Layout, Sizes, layout};
use crate::name::BinaryName;
use crate::sha256::sha256;
use crate::signature::{SUPER_BLOB_MAGIC, push_signature};

/// The bytes of a signed executable, and where everything in it is.
///
/// Only `write` makes it, so holding an `Executable` proves it holds code `generate` made, laid
/// out by `layout`.
#[derive(Debug)]
pub struct Executable {
    bytes: Vec<u8>,
    layout: Layout,
}

impl Executable {
    /// Returns the file's bytes, exactly as they're written to disk.
    #[must_use]
    pub const fn bytes(&self) -> &[u8] {
        self.bytes.as_slice()
    }

    /// Returns where everything in the file is.
    #[must_use]
    pub const fn layout(&self) -> Layout {
        self.layout
    }
}

/// Writes `code` as an executable named `name`, and returns it. Nothing can be refused: every
/// program's code fits in the largest file.
///
/// # Panics
///
/// Never, whatever the code and the name. The assertions check what `generate` made and the
/// writing's own work, so one that fails is a bug in the compiler.
///
/// If there isn't the memory for the file, Rust ends the process, which isn't a panic.
#[must_use]
pub fn write(code: &Code, name: BinaryName<'_>) -> Executable {
    let sizes = Sizes {
        code_bytes: code.size_bytes(),
        stack_bytes: code.stack_bytes(),
        name_bytes: name.size_bytes(),
    };
    let layout = layout(sizes);
    let mut image = Image::with_room_for(layout.image_bytes());
    push_header_and_commands(&mut image, &layout);
    let placement = Placement {
        code_address: layout.code_address(),
        stack_top_address: layout.stack_top_address(),
    };
    let placed = code.placed(placement);
    image.push_bytes(&placed);
    assert_eq!(
        image.offset(),
        CODE_OFFSET + layout.code_bytes(),
        "the code is as long as laid out"
    );
    image.pad_to(layout.text_bytes());
    image.fill(UUID_OFFSET, &write_uuid(image.as_slice(), name));
    push_signature(&mut image, name, &layout);
    image.pad_to(layout.image_bytes());
    let bytes = image.into_bytes();
    assert_written(&bytes, &placed, &layout);
    assert_written_signature(&bytes, name, &layout);
    Executable { bytes, layout }
}

/// These are the bits of a UUID that say what kind it is, and what they're set to: version 8,
/// which RFC 9562 gives to a UUID made a program's own way, in the layout that RFC defines.
const UUID_VERSION_INDEX: usize = 6;
const UUID_VERSION_MASK: u8 = 0xF0;
const UUID_VERSION_8: u8 = 0x80;
const UUID_VARIANT_INDEX: usize = 8;
const UUID_VARIANT_MASK: u8 = 0xC0;
const UUID_VARIANT_RFC: u8 = 0x80;

/// Returns the UUID of a binary named `name` whose `__TEXT` is `text`, with the UUID in it left
/// as zeros: the first 16 bytes of the SHA-256 of `text` and then the name, marked as a UUID of
/// version 8.
fn write_uuid(text: &[u8], name: BinaryName<'_>) -> [u8; usize_from_u32(UUID_BYTES)] {
    let (start, end) = (usize_from_u32(UUID_OFFSET), usize_from_u32(UUID_OFFSET + UUID_BYTES));
    assert!(end <= text.len(), "the UUID is in `__TEXT`");
    assert!(text[start..end].iter().all(|&byte| byte == 0), "the UUID is zeros while it's hashed");
    let mut hashed = Vec::with_capacity(text.len() + name.as_str().len());
    hashed.extend_from_slice(text);
    hashed.extend_from_slice(name.as_str().as_bytes());
    let hash = sha256(&hashed);
    let (uuid, _) = hash.split_first_chunk().expect("a hash is longer than a UUID");
    let mut uuid: [u8; usize_from_u32(UUID_BYTES)] = *uuid;
    uuid[UUID_VERSION_INDEX] = (uuid[UUID_VERSION_INDEX] & !UUID_VERSION_MASK) | UUID_VERSION_8;
    uuid[UUID_VARIANT_INDEX] = (uuid[UUID_VARIANT_INDEX] & !UUID_VARIANT_MASK) | UUID_VARIANT_RFC;
    uuid
}

/// Asserts that `bytes`, written for `placed` code, read back as `layout` laid them out: the
/// header, each command's kind in order, the code itself with zeros after it to the end of
/// `__TEXT`, the UUID set, and the last command, which says where the signature is and ends
/// the file there.
fn assert_written(bytes: &[u8], placed: &[u8], layout: &Layout) {
    let read = |offset: u32| {
        let start = usize_from_u32(offset);
        let (word, _) = bytes[start..].split_first_chunk().expect("a number is in the file");
        u32::from_le_bytes(*word)
    };
    assert_eq!(bytes.len(), usize_from_u32(layout.image_bytes()), "the file is as laid out");
    assert_eq!(read(0), MAGIC, "the file is a 64-bit Mach-O");
    assert_eq!(read(16), COMMANDS_COUNT, "the header counts the commands");
    assert_eq!(read(20), COMMANDS_BYTES, "the header measures the commands");
    // Each command starts with its kind and then its size, which leads to the next.
    let mut offset = HEADER_BYTES;
    for kind in COMMAND_KINDS {
        assert_eq!(read(offset), kind, "the commands are in order");
        offset += read(offset + 4);
    }
    assert_eq!(offset, CODE_OFFSET, "the commands end where the code starts");
    let code_start = usize_from_u32(CODE_OFFSET);
    let written = &bytes[code_start..code_start + placed.len()];
    assert_eq!(written, placed, "the code is written as it was placed");
    let text_end = usize_from_u32(layout.text_bytes());
    let after_code = &bytes[code_start + placed.len()..text_end];
    assert!(after_code.iter().all(|&byte| byte == 0), "zeros follow the code to `__TEXT`'s end");
    let (uuid_start, uuid_end) =
        (usize_from_u32(UUID_OFFSET), usize_from_u32(UUID_OFFSET + UUID_BYTES));
    let is_set = bytes[uuid_start..uuid_end].iter().any(|&byte| byte != 0);
    assert!(is_set, "the UUID is filled in");
    // The last command says where the signature is, and how long with its padding, which is
    // all of `__LINKEDIT`, to the end of the file.
    let signature_command = CODE_OFFSET - 8;
    assert_eq!(read(signature_command), layout.linkedit_offset(), "the signature's offset");
    assert_eq!(read(signature_command + 4), layout.linkedit_bytes(), "the signature's room");
    let signature_end = read(signature_command) + read(signature_command + 4);
    assert_eq!(usize_from_u32(signature_end), bytes.len(), "the signature's room ends the file");
}

/// Asserts that the signature in `bytes` is where `layout` put it, is as long, and is in the
/// name `name`, with nothing but zeros after it.
fn assert_written_signature(bytes: &[u8], name: BinaryName<'_>, layout: &Layout) {
    let start = usize_from_u32(layout.linkedit_offset());
    let end = start + usize_from_u32(layout.signature_bytes());
    let signature = &bytes[start..end];
    let (magic, after_magic) = signature.split_first_chunk().expect("a signature has a magic");
    assert_eq!(u32::from_be_bytes(*magic), SUPER_BLOB_MAGIC, "a SuperBlob is there");
    let (size, _) = after_magic.split_first_chunk().expect("a signature has a size");
    assert_eq!(u32::from_be_bytes(*size), layout.signature_bytes(), "the SuperBlob is its size");
    let name_start = usize_from_u32(crate::signature::SIGNATURE_HEADERS_BYTES);
    let name_end = name_start + name.as_str().len();
    assert_eq!(&signature[name_start..name_end], name.as_str().as_bytes(), "it's in this name");
    assert_eq!(signature[name_end], 0, "the name ends with a zero");
    assert!(bytes[end..].iter().all(|&byte| byte == 0), "zeros follow the signature");
}
