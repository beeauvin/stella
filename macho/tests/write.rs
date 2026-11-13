// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for writing the executable. They're a crate of their own, so the compiler lets them
//! reach only what the crates make public: every source goes through every check and `generate`,
//! and then `write`.
//!
//! An oracle builds the whole file, byte by byte, from the Mach-O format's numbers written out
//! here: the header, each load command, the code, and the signature with each page's hash. Every
//! file a test makes is compared with it whole, except in `the_code_finds_the_stack`, whose two
//! names other tests compare. The oracle shares two things with `write`, and
//! each is checked from outside. It's written by the same hand, so it could misread the format
//! the same way. And it hashes with the crate's `sha256`, so it can't catch a wrong hash. What
//! checks both is macOS:
//!
//! - `RAN` pins the SHA-256 of the files for two names, as `shasum -a 256` printed it on macOS
//!   27.2 on 2026-11-12 for files that `codesign --verify --strict` accepted and that ran there
//!   and exited with 0. `shasum` hashes the files again every time.
//! - The code is `SMALLEST_CODE`, read out of a file that ran, not asked of the code generator.
//! - Every time, `codesign --verify --strict` checks each file's signature, which checks every
//!   page's hash, and the file is run. `otool -l` reads each number back from the field the
//!   format gives it. `shasum` works out each page's hash and the UUID again from the file's
//!   bytes. And `codesign` signs each file again, and takes its signature off, which it only
//!   does for a file laid out as the system's linker lays one out.
//!
//! A machine without one of those tools fails these tests, rather than passing unchecked.

#![forbid(unsafe_code)]

use std::fmt::Write as _;
use std::fs;
use std::ops::Range;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use stella_arm64::{Code, Instruction, Placement, Register, StackOrRegister, generate};
use stella_ast::build;
use stella_blocks::nest;
use stella_grammar::parse;
use stella_lexer::lex;
use stella_linter_text::lint;
use stella_linter_tokens::lint_tokens;
use stella_macho::{BinaryName, Executable, NAME_BYTES_MAX, Sizes, layout, sha256, write};
use stella_meaning::{check_program, resolve};
use stella_test_support::{Rng, Scratch};

/// This is the smallest program, laid out the one way. For now every program is this one, give
/// or take its comments, so every file holds the same code.
const SMALLEST: &str = "function main() returns Void {\n}\n";

/// These are the smallest program's eleven instructions as its file holds them: the 44 bytes
/// that start 656 bytes into the file pinned first in `RAN`, which macOS ran, read with `xxd` as
/// little-endian words. Beside each is how `otool -tv` disassembles it in that file.
const SMALLEST_CODE: [u32; 11] = [
    0x9100_03E9, // mov x9, sp
    0xAA1E_03EA, // mov x10, x30
    0x9000_002B, // adrp x11, 4 ; 0x100004000
    0x9100_016B, // add x11, x11, #0x0
    0x9100_017F, // mov sp, x11
    0x9400_0005, // bl 0x1000002b8
    0x9100_013F, // mov sp, x9
    0xAA0A_03FE, // mov x30, x10
    0x5280_0000, // mov w0, #0x0
    0xD65F_03C0, // ret
    0xD65F_03C0, // ret
];

/// This is where the code is in the file, after the header's 32 bytes and the load commands'
/// 624, and where that puts it in memory, with `__TEXT` at 4 GiB.
const CODE_OFFSET: usize = 32 + 624;
const CODE_ADDRESS: u64 = 0x1_0000_0290;

/// This is where the smallest program's stack ends, where `sp` starts: at the bottom of `__DATA`,
/// a page above `__TEXT`, since it has no stack.
const STACK_TOP_ADDRESS: u64 = 0x1_0000_4000;

/// This is how many bytes `__TEXT` takes for the smallest program: one page of 16 KiB.
const TEXT_BYTES: usize = 16_384;

/// This is where the UUID's 16 bytes are in the file: in the sixth load command, after the
/// header, the four segments' commands, the loader's, and the command's own kind and size.
const UUID_RANGE: Range<usize> = 520..536;

/// This is where the signature's name starts, from the signature's start: after the `SuperBlob`'s
/// header of 12 bytes, its one entry of 8, and the `CodeDirectory`'s header of 88.
const SIGNATURE_HEADERS_BYTES: usize = 12 + 8 + 88;

/// This is the size of the pages the signature hashes, and how many of them `__TEXT` is.
const HASH_PAGE_BYTES: usize = 4_096;
const HASH_PAGES_COUNT: usize = 4;

/// This is the longest name a binary can have: 248 bytes.
const LONGEST: &str = concat!(
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
);

/// This is a name of every character a name can hold, each once: every ASCII character that
/// leaves a mark, but `/`.
const EVERY_CHARACTER: &str = concat!(
    "!\"#$%&'()*+,-.0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ",
    "[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~",
);

/// These are the names whose files macOS's tools are run on: the two that are pinned, the
/// shortest there is, one beside `hello`, one of every character, and one whose signature
/// needs no padding, since it's 240 bytes.
const NAMES: [&str; 6] = ["hello", LONGEST, "a", "world", EVERY_CHARACTER, "abc"];

/// These are the files for the smallest program under two names, each as its name and its
/// SHA-256. Each hash is what `shasum -a 256` printed on macOS 27.2 on 2026-11-12 for the file
/// this crate wrote, after `codesign --verify --strict` accepted that file and it ran and exited
/// with 0.
const RAN: [(&str, &str); 2] = [
    ("hello", "6361643d044060de3f8cf65ff45d179527d3e6bf0d829a1768cc047ee8c5a318"),
    (LONGEST, "b0675528a635d29a661349458b0c98560f53a4a9a38e4bd7f9981435bbe52e25"),
];

/// This is `hello`'s UUID as `otool -l` printed it on macOS 27.2 for the file pinned in `RAN`.
/// Its third group starts with the version, 8, and its fourth with the variant's digit, B.
const HELLO_UUID_TEXT: &str = "86C5D861-48A3-86F5-B6E9-03AAC0E6FB55";

/// This is the SHA-256 of a page of zeros, as `head -c 4096 /dev/zero | shasum -a 256` printed it
/// on macOS 27.2. The smallest program's last three pages are all zeros.
const ZERO_PAGE_SHA256: &str = "ad7facb2586fc6e966c004d7d1d16b024f5805ff7cb47c7a85dabd8b48892ca7";

/// A segment as the oracle states it: a run of memory, and the part of the file it's loaded from.
struct Segment {
    name: &'static str,
    address: u64,
    memory_bytes: u64,
    file_offset: u64,
    file_bytes: u64,
    /// What the memory may be used for: 1 to read, 2 to write, 4 to run.
    protection: u32,
    section: Option<Section>,
}

/// A segment's one section, as the oracle states it.
struct Section {
    name: &'static str,
    address: u64,
    memory_bytes: u64,
    file_offset: u32,
    alignment_power: u32,
    /// What the section holds: its type and attributes.
    kind: u32,
}

/// The oracle's file so far. Everything is added at the end, in the order it's in the file.
#[derive(Default)]
struct Image {
    bytes: Vec<u8>,
}

impl Image {
    /// Adds each of `values`, little-endian, as the header and the load commands hold numbers.
    fn u32s(&mut self, values: &[u32]) {
        for value in values {
            self.bytes.extend_from_slice(&value.to_le_bytes());
        }
    }

    /// Adds each of `values`, little-endian.
    fn u64s(&mut self, values: &[u64]) {
        for value in values {
            self.bytes.extend_from_slice(&value.to_le_bytes());
        }
    }

    /// Adds each of `values`, big-endian, as the signature holds numbers.
    fn u32s_big_endian(&mut self, values: &[u32]) {
        for value in values {
            self.bytes.extend_from_slice(&value.to_be_bytes());
        }
    }

    /// Adds `text` and then zeros, `size_bytes` in all.
    fn padded_text(&mut self, text: &str, size_bytes: usize) {
        assert!(text.len() < size_bytes, "{text:?} and a zero fit in {size_bytes} bytes");
        let end = self.bytes.len() + size_bytes;
        self.bytes.extend_from_slice(text.as_bytes());
        self.bytes.resize(end, 0);
    }

    /// Adds `segment`'s load command (`LC_SEGMENT_64`, 72 bytes), and its section's 80 bytes
    /// after it, if it has one.
    fn segment(&mut self, segment: &Segment) {
        let sections_count = u32::from(segment.section.is_some());
        self.u32s(&[0x19, 72 + 80 * sections_count]);
        self.padded_text(segment.name, 16);
        self.u64s(&[
            segment.address,
            segment.memory_bytes,
            segment.file_offset,
            segment.file_bytes,
        ]);
        // The most the memory may ever be used for, what it may be used for from the start, how
        // many sections follow, and no flags.
        self.u32s(&[segment.protection, segment.protection, sections_count, 0]);
        if let Some(section) = &segment.section {
            self.padded_text(section.name, 16);
            self.padded_text(segment.name, 16);
            self.u64s(&[section.address, section.memory_bytes]);
            // After the alignment: no relocations, by offset and by count. After the kind: three
            // fields that nothing uses.
            self.u32s(&[section.file_offset, section.alignment_power, 0, 0, section.kind, 0, 0, 0]);
        }
    }
}

/// What `otool -l` should print after a field's name: a number, in hex or in decimal, or text.
#[derive(Debug)]
enum Printed {
    Number(u64),
    Text(String),
}

/// What `otool -l` should print for one load command, or for one section: the line that starts
/// it, and each field worth pinning, by its name.
type PrintedBlock = (String, Vec<(&'static str, Printed)>);

/// One load command, or one section, as `otool -l` printed it: the line that starts it, and
/// every line after, up to the next.
#[derive(Debug)]
struct OtoolBlock {
    heading: String,
    lines: Vec<String>,
}

/// Returns the code `generate` makes for `source`, which must pass every check.
fn code_of(source: &str) -> Code {
    let text = lint(source.as_bytes()).expect("source should pass the text lint");
    let tokens = lex(text).expect("source should lex");
    let laid_out = lint_tokens(tokens).expect("source should pass the tokens lint");
    let forms = parse(nest(laid_out)).expect("source should parse");
    let types = resolve(build(forms)).expect("source's types should resolve");
    let program = check_program(types).expect("source should be a program");
    generate(&program)
}

/// Writes `source`, which must pass every check, as a binary named `name`, which must be a name
/// a binary can have. For now every program's code is the smallest's, so this checks that the
/// code is `SMALLEST_CODE` once it's placed where the format puts it, before `write` sees it.
fn written(source: &str, name: &str) -> Executable {
    let code = code_of(source);
    assert_eq!(code.size_bytes(), 44, "the code is the smallest program's eleven instructions");
    assert_eq!(code.stack_bytes(), 0, "the smallest program needs no stack");
    let placement = Placement { code_address: CODE_ADDRESS, stack_top_address: STACK_TOP_ADDRESS };
    assert_eq!(code.placed(placement), smallest_code_bytes(), "the code that ran, placed");
    let name = BinaryName::new(name).expect("the name should be one a binary can have");
    write(&code, name)
}

/// Returns `SMALLEST_CODE` as the bytes the file holds: each word little-endian.
fn smallest_code_bytes() -> Vec<u8> {
    SMALLEST_CODE.iter().flat_map(|word| word.to_le_bytes()).collect()
}

/// Returns `count` as a `u32`, to write in a file.
fn u32_of(count: usize) -> u32 {
    u32::try_from(count).expect("no size in these files is anywhere near 4 GiB")
}

/// Returns how many bytes the signature takes for a binary named `name`: its headers, the name
/// and the 0 that ends it, and a hash of 32 bytes for each of the four pages.
const fn signature_bytes_of(name: &str) -> usize {
    SIGNATURE_HEADERS_BYTES + name.len() + 1 + 32 * HASH_PAGES_COUNT
}

/// Returns how many bytes `__LINKEDIT` takes in the file for a binary named `name`: the
/// signature, padded with zeros to 16 bytes.
const fn linkedit_bytes_of(name: &str) -> usize {
    signature_bytes_of(name).div_ceil(16) * 16
}

/// Returns where the signature's hash of the page at `page_index` is, in the file of a binary
/// named `name`: after `__TEXT`, the signature's headers, the name and its 0, and the hashes
/// before it.
fn hash_range(name: &str, page_index: usize) -> Range<usize> {
    assert!(page_index < HASH_PAGES_COUNT, "`__TEXT` is four pages of 4 KiB");
    let start = TEXT_BYTES + SIGNATURE_HEADERS_BYTES + name.len() + 1 + 32 * page_index;
    start..start + 32
}

/// Returns `uuid`, 16 bytes of a hash, marked as a UUID the way RFC 9562 lays one out: version
/// 8 in the high four bits of byte 6, and the variant `10` in the top two bits of byte 8.
const fn marked(mut uuid: [u8; 16]) -> [u8; 16] {
    uuid[6] = 0x80 | (uuid[6] & 0x0F);
    uuid[8] = 0x80 | (uuid[8] & 0x3F);
    uuid
}

/// Returns the four segments of the smallest program's file, as they should be when `__LINKEDIT`
/// takes `linkedit_bytes` in it. `__PAGEZERO` keeps the lowest 4 GiB unmapped. `__TEXT` is the
/// file's first page, to read and run. `__DATA` is a page of memory to read and write, with the
/// empty stack at its bottom, and nothing in the file. `__LINKEDIT` is the signature, to read.
fn segments_oracle(linkedit_bytes: usize) -> [Segment; 4] {
    let code = Section {
        name: "__text",
        address: CODE_ADDRESS,
        memory_bytes: 44,
        file_offset: 656,
        alignment_power: 2,
        // Only instructions (`S_ATTR_PURE_INSTRUCTIONS` and `S_ATTR_SOME_INSTRUCTIONS`).
        kind: 0x8000_0400,
    };
    let stack = Section {
        name: "__bss",
        address: 0x1_0000_4000,
        memory_bytes: 0,
        file_offset: 0,
        alignment_power: 4,
        // Zeros that take no room in the file (`S_ZEROFILL`).
        kind: 1,
    };
    let segment = |name, address| Segment {
        name,
        address,
        memory_bytes: 16_384,
        file_offset: 0,
        file_bytes: 0,
        protection: 0,
        section: None,
    };
    [
        Segment { memory_bytes: 0x1_0000_0000, ..segment("__PAGEZERO", 0) },
        Segment {
            file_bytes: 16_384,
            protection: 1 | 4,
            section: Some(code),
            ..segment("__TEXT", 0x1_0000_0000)
        },
        Segment { protection: 1 | 2, section: Some(stack), ..segment("__DATA", 0x1_0000_4000) },
        Segment {
            file_offset: 16_384,
            file_bytes: u64::from(u32_of(linkedit_bytes)),
            protection: 1,
            ..segment("__LINKEDIT", 0x1_0000_8000)
        },
    ]
}

/// Returns the file the oracle expects for the smallest program as a binary named `name`.
fn image_oracle(name: &str) -> Vec<u8> {
    let mut image = Image::default();
    // The header: a 64-bit Mach-O, for arm64 of any model, a program to run, with ten load
    // commands of 624 bytes, and the flags `MH_NOUNDEFS`, `MH_DYLDLINK`, `MH_TWOLEVEL`, `MH_PIE`.
    image.u32s(&[0xFEED_FACF, 0x0100_000C, 0, 2, 10, 624, 0x0020_0085, 0]);
    for segment in &segments_oracle(linkedit_bytes_of(name)) {
        image.segment(segment);
    }
    // The loader's path (`LC_LOAD_DYLINKER`), 12 bytes into its command.
    image.u32s(&[0xE, 32, 12]);
    image.padded_text("/usr/lib/dyld", 20);
    // The UUID (`LC_UUID`), left as zeros until the page it's in can be hashed.
    image.u32s(&[0x1B, 24]);
    assert_eq!(image.bytes.len(), UUID_RANGE.start, "the UUID is 520 bytes in");
    image.bytes.extend_from_slice(&[0; 16]);
    // The build version (`LC_BUILD_VERSION`): for macOS, which is platform 1, needing 27.0, made
    // against the SDK of 27.0, by no tools that are named.
    image.u32s(&[0x32, 24, 1, 0x001B_0000, 0x001B_0000, 0]);
    // Where the program starts (`LC_MAIN`): at the code's first byte, on a stack the system
    // sizes.
    image.u32s(&[0x8000_0028, 24]);
    image.u64s(&[656, 0]);
    // libSystem (`LC_LOAD_DYLIB`): its path 24 bytes into the command, a timestamp of 2, its
    // version 1359.40.3, and the version it must be compatible with, 1.0.0.
    image.u32s(&[0xC, 56, 24, 2, 0x054F_2803, 0x0001_0000]);
    image.padded_text("/usr/lib/libSystem.B.dylib", 32);
    // Where the signature is (`LC_CODE_SIGNATURE`): after `__TEXT`, and how long with the
    // zeros that pad it, which is all of `__LINKEDIT`, to the end of the file. The system's
    // linker counts its padding the same way: for binaries named `t9`, `t9999` and `abcdefg`,
    // `ld` on macOS 27.2 gave sizes of 272, 280 and 280 for signatures that say they're 271,
    // 274 and 276 bytes, with zeros between. It pads to 8 bytes where Stella pads to 16, and
    // `codesign` takes either.
    image.u32s(&[0x1D, 16, 16_384, u32_of(linkedit_bytes_of(name))]);
    assert_eq!(image.bytes.len(), CODE_OFFSET, "the header and the commands are 656 bytes");
    image.u32s(&SMALLEST_CODE);
    image.bytes.resize(TEXT_BYTES, 0);
    let uuid = uuid_oracle(&image.bytes, name);
    image.bytes[UUID_RANGE].copy_from_slice(&uuid);
    signature_oracle(&mut image, name);
    image.bytes.resize(TEXT_BYTES + linkedit_bytes_of(name), 0);
    image.bytes
}

/// Returns what the UUID of the binary named `name` is the hash of, given `bytes`, at least its
/// `__TEXT`: all of `__TEXT`, with zeros where the UUID goes, and then the name.
fn uuid_source_of(bytes: &[u8], name: &str) -> Vec<u8> {
    let mut uuid_source = bytes[..TEXT_BYTES].to_vec();
    uuid_source[UUID_RANGE].fill(0);
    uuid_source.extend_from_slice(name.as_bytes());
    uuid_source
}

/// Returns the UUID of a binary named `name` whose `__TEXT` is `text`: the first 16 bytes of the
/// SHA-256 of what `uuid_source_of` gives, marked as a UUID.
fn uuid_oracle(text: &[u8], name: &str) -> [u8; 16] {
    assert_eq!(text.len(), TEXT_BYTES, "`__TEXT` is one page");
    let hash = sha256(&uuid_source_of(text, name));
    let (uuid, _) = hash.split_first_chunk().expect("a hash is longer than a UUID");
    marked(*uuid)
}

/// Adds the signature the oracle expects to `image`, which holds all of `__TEXT`, for a binary
/// named `name`: a `SuperBlob` that holds one `CodeDirectory`, with every number big-endian.
fn signature_oracle(image: &mut Image, name: &str) {
    assert_eq!(image.bytes.len(), TEXT_BYTES, "the signature follows `__TEXT`");
    let hashes: Vec<[u8; 32]> = image.bytes.chunks(HASH_PAGE_BYTES).map(sha256).collect();
    assert_eq!(hashes.len(), HASH_PAGES_COUNT, "a hash for each 4 KiB of `__TEXT`");
    let signature_bytes = u32_of(signature_bytes_of(name));
    // The `SuperBlob`: its magic, its size, and its one entry, a `CodeDirectory` 20 bytes in.
    image.u32s_big_endian(&[0xFADE_0CC0, signature_bytes, 1, 0, 20]);
    // The `CodeDirectory`: its magic, its size, its version, and the flags `CS_ADHOC` and
    // `CS_LINKER_SIGNED`. Then where the hashes start, where the name starts, no special
    // hashes, four hashes of the code, and the 16,384 bytes they cover.
    let hashes_offset = 88 + u32_of(name.len()) + 1;
    image.u32s_big_endian(&[0xFADE_0C02, signature_bytes - 20, 0x0002_0400, 0x0002_0002]);
    image.u32s_big_endian(&[hashes_offset, 88, 0, 4, 16_384]);
    // A hash is 32 bytes, of SHA-256, which is kind 2. No platform. Pages of 2 to the 12.
    image.bytes.extend_from_slice(&[32, 2, 0, 12]);
    // A spare field, no scattered pages, no team, another spare, and no 64-bit limit.
    image.u32s_big_endian(&[0, 0, 0, 0, 0, 0]);
    // The code's segment, each number in 64 bits: it starts the file, it's 16,384 bytes, and
    // it's the main binary's (`CS_EXECSEG_MAIN_BINARY`).
    image.u32s_big_endian(&[0, 0, 0, 16_384, 0, 1]);
    image.bytes.extend_from_slice(name.as_bytes());
    image.bytes.push(0);
    for hash in hashes {
        image.bytes.extend_from_slice(&hash);
    }
}

/// Checks `executable`, written for the smallest program as a binary named `name`, against the
/// oracle: its bytes whole, and its layout against `layout`'s for the same sizes.
fn check(executable: &Executable, name: &str) {
    let expected = image_oracle(name);
    let bytes = executable.bytes();
    assert_eq!(bytes.len(), expected.len(), "the size of the file named {name:?}");
    // Comparing byte by byte says which byte is wrong, where comparing whole would print them
    // all.
    for (index, (byte, expected_byte)) in bytes.iter().zip(&expected).enumerate() {
        assert_eq!(byte, expected_byte, "byte {index} of the file named {name:?}");
    }
    let sizes = Sizes { code_bytes: 44, stack_bytes: 0, name_bytes: u32_of(name.len()) };
    assert_eq!(executable.layout(), layout(sizes), "the layout of the file named {name:?}");
    let image_bytes = u32_of(bytes.len());
    assert_eq!(executable.layout().image_bytes(), image_bytes, "the layout measures the file");
}

/// Returns the instruction in `bytes`, a file, at `instruction_index` in its code.
fn instruction_at(bytes: &[u8], instruction_index: usize) -> u32 {
    let start = CODE_OFFSET + 4 * instruction_index;
    let (word, _) = bytes[start..].split_first_chunk().expect("the instruction is in the file");
    u32::from_le_bytes(*word)
}

/// Returns the address the third and fourth instructions in `bytes`, a file, leave in `x11`:
/// what the processor works out from `adrp x11` and `add x11, x11`, read from the fields the Arm
/// manual gives each, apart from the code generator.
fn stack_top_found(bytes: &[u8]) -> u64 {
    let (page_address, add) = (instruction_at(bytes, 2), instruction_at(bytes, 3));
    // `adrp`: bit 31 is 1, bits 28 to 24 are 10000, and bits 4 to 0 are the register, 11.
    assert_eq!(page_address & 0x9F00_001F, 0x9000_000B, "the third instruction is `adrp x11`");
    // `add`, on 64 bits, of an immediate that isn't shifted: bits 31 to 22 are 1001000100, bits
    // 9 to 5 are the register read, and bits 4 to 0 the register written, both 11.
    assert_eq!(add & 0xFFC0_03FF, 0x9100_016B, "the fourth instruction is `add x11, x11`");
    // `adrp` counts 4 KiB pages in 21 bits: the low 2 in bits 30 and 29, the rest in bits 23 to
    // 5. The top bit is the sign, and the stack is above the code, so it's clear.
    let pages_low = u64::from((page_address >> 29_u32) & 0b11);
    let pages_high = u64::from((page_address >> 5_u32) & 0x7_FFFF);
    let pages_count = (pages_high << 2_u32) | pages_low;
    assert!(pages_count < 1 << 20_u32, "the stack is above the code, so the count isn't negative");
    // The pages are counted from the page the `adrp` itself is in, two instructions into the
    // code.
    let instruction_address = CODE_ADDRESS + 2 * 4;
    let page = instruction_address / 4_096 + pages_count;
    // `add` holds its immediate in bits 21 to 10.
    page * 4_096 + u64::from((add >> 10_u32) & 0xFFF)
}

/// Returns `bytes` as hex, lowercase, as `shasum` and `codesign` write a hash.
fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(2 * bytes.len());
    for byte in bytes {
        write!(text, "{byte:02x}").expect("writing to a string can't fail");
    }
    text
}

/// Returns `uuid` as `otool` prints one: uppercase hex, in groups of 4, 2, 2, 2 and 6 bytes.
fn uuid_text(uuid: &[u8]) -> String {
    assert_eq!(uuid.len(), 16, "a UUID is 16 bytes");
    let mut text = String::with_capacity(36);
    for (index, byte) in uuid.iter().enumerate() {
        if matches!(index, 4 | 6 | 8 | 10) {
            text.push('-');
        }
        write!(text, "{byte:02X}").expect("writing to a string can't fail");
    }
    text
}

/// Writes `bytes` as an executable file named `name` in a new scratch directory named
/// `directory_name`, and returns the file's path. Each file gets a directory of its own, so no
/// file is ever written over one that has run.
fn executable_file(directory_name: &str, name: &str, bytes: &[u8]) -> PathBuf {
    let directory = Scratch::of("macho-write").fresh(directory_name);
    stella_test_support::write(&directory, name, bytes);
    let path = directory.join(name);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
        .expect("a test's own file takes the permissions it's given");
    path
}

/// Runs `tool`, one of macOS's own, with `options` and then `path`, and returns what it did.
fn run_tool(tool: &str, options: &[&str], path: &Path) -> Output {
    let output = Command::new(tool).args(options).arg(path).stdin(Stdio::null()).output();
    assert!(output.is_ok(), "`{tool}` is there to run on macOS with Xcode: {output:?}");
    output.expect("the tool ran")
}

/// Returns each line of `printed`, which one of macOS's tools printed.
fn lines_of(printed: &[u8]) -> Vec<String> {
    let printed = str::from_utf8(printed).expect("macOS's tools print UTF-8");
    printed.lines().map(String::from).collect()
}

/// Hashes each of `messages` with `/usr/bin/shasum -a 256`, all in one run, from files in a new
/// scratch directory named `directory_name`, and returns the hashes in order, as hex.
fn shasum(directory_name: &str, messages: &[&[u8]]) -> Vec<String> {
    let directory = Scratch::of("macho-write").fresh(directory_name);
    let mut file_names = Vec::with_capacity(messages.len());
    for (index, message) in messages.iter().enumerate() {
        let file_name = format!("{index:04}");
        stella_test_support::write(&directory, &file_name, message);
        file_names.push(file_name);
    }
    let output = Command::new("/usr/bin/shasum")
        .args(["-a", "256"])
        .args(&file_names)
        .current_dir(&directory)
        .stdin(Stdio::null())
        .output()
        .expect("`/usr/bin/shasum` is there to run on macOS");
    let said = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "shasum hashes files it can read, but said: {said}");
    // Each line is a file's hash, then its name.
    let mut hashes = Vec::with_capacity(messages.len());
    for (line, file_name) in lines_of(&output.stdout).iter().zip(&file_names) {
        let mut words = line.split_whitespace();
        let hash = words.next().expect("a line starts with a hash");
        assert_eq!(words.next(), Some(file_name.as_str()), "the hashes are in the files' order");
        assert_eq!(words.next(), None, "a line is a hash and a name: {line:?}");
        assert_eq!(hash.len(), 64, "a hash is 32 bytes, so 64 digits: {hash:?}");
        hashes.push(String::from(hash));
    }
    assert_eq!(hashes.len(), messages.len(), "shasum prints one line for each file");
    hashes
}

/// Returns the first 16 bytes `hash` stands for, a hash as `shasum` printed it.
fn first_16_bytes_of(hash: &str) -> [u8; 16] {
    let digits: Vec<u32> = hash.chars().map_while(|digit| digit.to_digit(16)).collect();
    assert_eq!(digits.len(), 64, "a hash is 64 hex digits: {hash:?}");
    let mut bytes = [0_u8; 16];
    let (pairs, _) = digits.as_chunks::<2>();
    for (byte, [high, low]) in bytes.iter_mut().zip(pairs) {
        *byte = u8::try_from(high * 16 + low).expect("two hex digits are a byte");
    }
    bytes
}

/// Returns what `otool -l` prints for the file at `path`, as its load commands and their
/// sections, in order. A command starts at a line that says `Load command` and its number, and a
/// section at a line that says `Section`.
fn otool_blocks(path: &Path) -> Vec<OtoolBlock> {
    let output = run_tool("/usr/bin/otool", &["-l"], path);
    let said = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "otool reads a Mach-O file, but said: {said}");
    let mut blocks: Vec<OtoolBlock> = Vec::new();
    for line in lines_of(&output.stdout) {
        let line = line.trim();
        if line.starts_with("Load command ") || line == "Section" {
            blocks.push(OtoolBlock { heading: String::from(line), lines: Vec::new() });
        } else if let Some(block) = blocks.last_mut() {
            block.lines.push(String::from(line));
        } else {
            // Before the first command, otool prints the file's path.
            assert!(line.ends_with(':'), "otool starts with the file's path, not {line:?}");
        }
    }
    blocks
}

/// Returns what `block` says after the field named `key`: the rest of the one line that starts
/// with the name and then a space.
fn printed_after<'block>(block: &'block OtoolBlock, key: &str) -> &'block str {
    let mut values = block.lines.iter().filter_map(|line| {
        let after = line.strip_prefix(key)?;
        after.starts_with(' ').then_some(after.trim())
    });
    let value = values.next();
    assert!(value.is_some(), "{} has a line for `{key}`: {block:?}", block.heading);
    assert!(values.next().is_none(), "{} has one line for `{key}`: {block:?}", block.heading);
    value.expect("the line was found")
}

/// Returns the number `text` is, as otool prints one: hex after `0x`, or decimal.
fn number_of(text: &str) -> u64 {
    let number = match text.strip_prefix("0x") {
        Some(hex_digits) => u64::from_str_radix(hex_digits, 16),
        None => text.parse(),
    };
    assert!(number.is_ok(), "otool prints a number here, not {text:?}");
    number.expect("the number was read")
}

/// Returns what `otool -l` should print for `segment`, the load command numbered
/// `command_index`, and for its section after it, if it has one.
fn printed_segment(command_index: usize, segment: &Segment) -> Vec<PrintedBlock> {
    let sections_count = u64::from(segment.section.is_some());
    let protection = u64::from(segment.protection);
    let mut blocks = vec![(
        format!("Load command {command_index}"),
        vec![
            ("cmd", Printed::Text(String::from("LC_SEGMENT_64"))),
            ("cmdsize", Printed::Number(72 + 80 * sections_count)),
            ("segname", Printed::Text(String::from(segment.name))),
            ("vmaddr", Printed::Number(segment.address)),
            ("vmsize", Printed::Number(segment.memory_bytes)),
            ("fileoff", Printed::Number(segment.file_offset)),
            ("filesize", Printed::Number(segment.file_bytes)),
            ("maxprot", Printed::Number(protection)),
            ("initprot", Printed::Number(protection)),
            ("nsects", Printed::Number(sections_count)),
            ("flags", Printed::Number(0)),
        ],
    )];
    if let Some(section) = &segment.section {
        blocks.push((
            String::from("Section"),
            vec![
                ("sectname", Printed::Text(String::from(section.name))),
                ("segname", Printed::Text(String::from(segment.name))),
                ("addr", Printed::Number(section.address)),
                ("size", Printed::Number(section.memory_bytes)),
                ("offset", Printed::Number(u64::from(section.file_offset))),
                ("reloff", Printed::Number(0)),
                ("nreloc", Printed::Number(0)),
                ("flags", Printed::Number(u64::from(section.kind))),
            ],
        ));
    }
    blocks
}

/// Returns what `otool -l` should print for the smallest program's file under `name`, whose UUID
/// is `uuid`: each load command and section in order, with the fields worth pinning. The
/// library's timestamp is left out, since otool prints it as a date in the machine's own time.
fn printed_oracle(name: &str, uuid: &[u8]) -> Vec<PrintedBlock> {
    let text = |text: &str| Printed::Text(String::from(text));
    let command = |index: u32, kind: &str, size_bytes: u64| {
        (
            format!("Load command {index}"),
            vec![("cmd", text(kind)), ("cmdsize", Printed::Number(size_bytes))],
        )
    };
    let mut blocks = Vec::new();
    for (index, segment) in segments_oracle(linkedit_bytes_of(name)).iter().enumerate() {
        blocks.extend(printed_segment(index, segment));
    }
    let mut loader = command(4, "LC_LOAD_DYLINKER", 32);
    loader.1.push(("name", text("/usr/lib/dyld (offset 12)")));
    let mut identity = command(5, "LC_UUID", 24);
    identity.1.push(("uuid", text(&uuid_text(uuid))));
    let mut version = command(6, "LC_BUILD_VERSION", 24);
    version.1.extend([
        ("minos", text("27.0")),
        ("sdk", text("27.0")),
        ("ntools", Printed::Number(0)),
    ]);
    let mut entry = command(7, "LC_MAIN", 24);
    entry.1.extend([("entryoff", Printed::Number(656)), ("stacksize", Printed::Number(0))]);
    let mut library = command(8, "LC_LOAD_DYLIB", 56);
    library.1.extend([
        ("name", text("/usr/lib/libSystem.B.dylib (offset 24)")),
        ("current version", text("1359.40.3")),
        ("compatibility version", text("1.0.0")),
    ]);
    let mut signature = command(9, "LC_CODE_SIGNATURE", 16);
    let signature_room_bytes = u64::from(u32_of(linkedit_bytes_of(name)));
    signature.1.extend([
        ("dataoff", Printed::Number(16_384)),
        ("datasize", Printed::Number(signature_room_bytes)),
    ]);
    blocks.extend([loader, identity, version, entry, library, signature]);
    blocks
}

#[test]
fn the_smallest_program() {
    // The smallest program, named `hello`, is the oracle's file byte for byte: 16,640 bytes,
    // which start with a 64-bit Mach-O's magic and hold the code that ran, 656 bytes in. Its
    // UUID is the one otool printed for the file that ran, and it's marked as a UUID: version 8,
    // and RFC 9562's variant.
    let hello = written(SMALLEST, "hello");
    check(&hello, "hello");
    let bytes = hello.bytes();
    assert_eq!(bytes.len(), 16_640);
    assert_eq!(bytes[..4], [0xCF, 0xFA, 0xED, 0xFE]);
    assert_eq!(bytes[CODE_OFFSET..CODE_OFFSET + 44], smallest_code_bytes());
    assert!(bytes[CODE_OFFSET + 44..TEXT_BYTES].iter().all(|&byte| byte == 0));
    let uuid: [u8; 16] = bytes[UUID_RANGE].try_into().expect("a UUID is 16 bytes");
    assert_eq!(uuid_text(&uuid), HELLO_UUID_TEXT);
    assert_eq!(uuid[6] >> 4_u32, 8, "the version is 8");
    assert_eq!(uuid[8] >> 6_u32, 0b10, "the variant is RFC 9562's");
    assert_eq!(hello.layout(), layout(Sizes { code_bytes: 44, stack_bytes: 0, name_bytes: 5 }));
    assert_eq!(hello.layout().code_address(), CODE_ADDRESS);
    assert_eq!(hello.layout().stack_top_address(), STACK_TOP_ADDRESS);
}

#[test]
fn the_code_finds_the_stack() {
    // The `adrp` and `add` in the file put the address of the stack's top in `x11`, and the next
    // instruction moves it to `sp`. The address is read from the two instructions' fields by
    // hand, and again with the code generator's own `decode`, and it's where the layout puts the
    // stack's top: the bottom of `__DATA`, four 4 KiB pages above the page the code is in.
    for name in ["hello", LONGEST] {
        let executable = written(SMALLEST, name);
        let bytes = executable.bytes();
        assert_eq!(stack_top_found(bytes), executable.layout().stack_top_address(), "for {name}");
        assert_eq!(stack_top_found(bytes), 0x1_0000_4000, "for {name}");
        let stack_top = Register::new(11);
        assert_eq!(
            Instruction::decode(instruction_at(bytes, 2)),
            Some(Instruction::PageAddress { target: stack_top, pages: 4_i32 }),
        );
        let stack_top = StackOrRegister::Register(stack_top);
        assert_eq!(
            Instruction::decode(instruction_at(bytes, 3)),
            Some(Instruction::AddImmediate { target: stack_top, source: stack_top, immediate: 0 }),
        );
        assert_eq!(
            Instruction::decode(instruction_at(bytes, 4)),
            Some(Instruction::AddImmediate {
                target: StackOrRegister::StackPointer,
                source: stack_top,
                immediate: 0,
            }),
        );
    }
}

#[test]
fn every_name_length() {
    // A name of every length there is, from 1 byte to 248, twice over: one of `a`s, and one of
    // random characters a name can hold. Each file is the oracle's byte for byte, and two names
    // of one length give files of one size.
    assert_eq!(LONGEST.len(), 248);
    assert_eq!(usize::try_from(NAME_BYTES_MAX), Ok(LONGEST.len()));
    assert_eq!(EVERY_CHARACTER.len(), 93);
    let mut rng = Rng::new(0x5752_4954_4500_0001);
    for size_bytes in 1..=LONGEST.len() {
        let letters = "a".repeat(size_bytes);
        let mut random = String::with_capacity(size_bytes);
        for _ in 0..size_bytes {
            let character = EVERY_CHARACTER.as_bytes()[rng.below(EVERY_CHARACTER.len())];
            random.push(char::from(character));
        }
        let (of_letters, of_random) = (written(SMALLEST, &letters), written(SMALLEST, &random));
        check(&of_letters, &letters);
        check(&of_random, &random);
        assert_eq!(of_letters.bytes().len(), of_random.bytes().len(), "{size_bytes} bytes");
        assert_eq!(of_letters.layout(), of_random.layout(), "{size_bytes} bytes");
    }
    check(&written(SMALLEST, EVERY_CHARACTER), EVERY_CHARACTER);
}

#[test]
fn the_same_bytes_every_time() {
    // The same program and name always make the same bytes: nothing in the file is a time, a
    // path or a random number. Comments mean nothing, so a program with them makes the same
    // bytes too.
    let first = written(SMALLEST, "hello");
    check(&first, "hello");
    for _ in 0..10_u32 {
        assert_eq!(written(SMALLEST, "hello").bytes(), first.bytes());
    }
    for commented in [
        "// a\nfunction main() returns Void {\n}\n",
        "function main() returns Void {\n    // a\n}\n",
        "function main() returns Void {\n}\n// a\n",
    ] {
        assert_eq!(written(commented, "hello").bytes(), first.bytes(), "for {commented:?}");
    }
}

#[test]
fn names_of_one_length() {
    // Two names of one length give files that differ only where the name is, or is hashed: the
    // UUID, the name in the signature, and the hash of the first page, which holds the UUID. So
    // binaries of two names never share a UUID, and everything else is the same.
    let (hello, world) = (written(SMALLEST, "hello"), written(SMALLEST, "world"));
    check(&hello, "hello");
    check(&world, "world");
    assert_eq!(hello.layout(), world.layout());
    let (hello, world) = (hello.bytes(), world.bytes());
    assert_eq!(hello.len(), world.len());
    let name_start = TEXT_BYTES + SIGNATURE_HEADERS_BYTES;
    let name_range = name_start..name_start + 5;
    assert_eq!(hello[name_range.clone()], *b"hello");
    assert_eq!(world[name_range.clone()], *b"world");
    let first_hash_range = hash_range("hello", 0);
    assert_eq!(first_hash_range.start, name_range.end + 1, "the hashes follow the name's 0");
    for index in 0..hello.len() {
        let is_named = UUID_RANGE.contains(&index)
            || name_range.contains(&index)
            || first_hash_range.contains(&index);
        if !is_named {
            assert_eq!(hello[index], world[index], "byte {index}");
        }
    }
    assert_ne!(hello[UUID_RANGE], world[UUID_RANGE], "the UUIDs differ");
    assert_ne!(hello[first_hash_range.clone()], world[first_hash_range], "the hashes differ");
    for page_index in 1..HASH_PAGES_COUNT {
        let range = hash_range("hello", page_index);
        assert_eq!(hello[range.clone()], world[range], "the hash of page {page_index}");
    }
}

#[test]
fn names_change_only_what_they_name() {
    // The shortest name and the longest give files of different sizes. Before the signature,
    // they differ only in the UUID and in the two places that say how much room the signature
    // takes with its padding: the size of `__LINKEDIT` in the file, in its segment's command,
    // and the same size in the last command. Of the four pages hashed, only the first holds
    // any of those.
    let (short, long) = (written(SMALLEST, "a"), written(SMALLEST, LONGEST));
    check(&short, "a");
    check(&long, LONGEST);
    assert_eq!(short.layout().signature_bytes(), 20 + 88 + 2 + 4 * 32);
    assert_eq!(long.layout().signature_bytes(), 20 + 88 + 249 + 4 * 32);
    assert_eq!((short.layout().linkedit_bytes(), long.layout().linkedit_bytes()), (240, 496));
    let (short, long) = (short.bytes(), long.bytes());
    assert_eq!((short.len(), long.len()), (16_624, 16_880));
    // `__LINKEDIT`'s command follows the header and three segments' commands, and its size in
    // the file is 48 bytes into it, in 8 bytes. The size the signature's command gives is the
    // last 4 bytes of the commands.
    let linkedit_size_range = 32 + 72 + 152 + 152 + 48..32 + 72 + 152 + 152 + 56;
    let signature_size_range = CODE_OFFSET - 4..CODE_OFFSET;
    let measuring = [UUID_RANGE, linkedit_size_range, signature_size_range];
    for index in 0..TEXT_BYTES {
        if !measuring.iter().any(|range| range.contains(&index)) {
            assert_eq!(short[index], long[index], "byte {index}");
        }
    }
    for range in measuring {
        assert_ne!(short[range.clone()], long[range.clone()], "bytes {range:?}");
    }
    assert_ne!(short[hash_range("a", 0)], long[hash_range(LONGEST, 0)]);
    for page_index in 1..HASH_PAGES_COUNT {
        let (short_range, long_range) =
            (hash_range("a", page_index), hash_range(LONGEST, page_index));
        assert_eq!(short[short_range], long[long_range], "the hash of page {page_index}");
    }
}

#[test]
fn the_bytes_that_ran() {
    // The files for the two pinned names hash to the pins, as macOS's `shasum` hashes the bytes
    // today. Each pin was taken from a file that `codesign` accepted and that ran, which
    // `macos_checks_the_signature_and_runs_it` checks of the same bytes every time.
    for (index, (name, pinned)) in RAN.into_iter().enumerate() {
        let executable = written(SMALLEST, name);
        check(&executable, name);
        let hashes = shasum(&format!("ran-{index}"), &[executable.bytes()]);
        assert_eq!(hashes, [pinned], "the SHA-256 of the file named {name}");
        assert_eq!(hex(&sha256(executable.bytes())), pinned, "for {name}");
    }
}

#[test]
fn macos_checks_the_signature_and_runs_it() {
    // Each file is written to disk, and macOS's own tools are run on it. `codesign --verify
    // --strict` accepts the signature, which means every page's hash is right. `codesign
    // --display` reads the signature back: the name it identifies the binary by, an ad hoc
    // signature a linker made, four hashes, and the hash of the `CodeDirectory` itself. Then the
    // file is run: the program exits with 0 and says nothing.
    for (index, name) in NAMES.into_iter().enumerate() {
        let executable = written(SMALLEST, name);
        check(&executable, name);
        let path = executable_file(&format!("runs-{index}"), name, executable.bytes());
        let verified = run_tool("/usr/bin/codesign", &["--verify", "--strict"], &path);
        let said = String::from_utf8_lossy(&verified.stderr);
        assert!(verified.status.success(), "codesign refused the file named {name}: {said}");
        let displayed = run_tool("/usr/bin/codesign", &["--display", "--verbose=4"], &path);
        assert!(displayed.status.success(), "codesign displays the signature of {name}");
        let lines = lines_of(&displayed.stderr);
        let has = |line: &str| lines.iter().any(|displayed_line| displayed_line == line);
        assert!(has(&format!("Identifier={name}")), "{lines:?}");
        assert!(has("Signature=adhoc"), "{lines:?}");
        let directory_bytes = signature_bytes_of(name) - 20;
        let directory = format!(
            "CodeDirectory v=20400 size={directory_bytes} flags=0x20002(adhoc,linker-signed) \
             hashes=4+0 location=embedded"
        );
        assert!(has(&directory), "{directory:?} isn't in {lines:?}");
        let directory_start = TEXT_BYTES + 20;
        let directory = &executable.bytes()[directory_start..directory_start + directory_bytes];
        assert!(has(&format!("CDHash={}", hex(&sha256(directory)[..20]))), "{lines:?}");
        let run = Command::new(&path).stdin(Stdio::null()).output();
        let run = run.expect("a file the test made executable can be run");
        assert_eq!(run.status.code(), Some(0_i32), "{name} exits with 0, not {:?}", run.status);
        assert!(run.stdout.is_empty(), "{name} writes nothing to stdout");
        assert!(run.stderr.is_empty(), "{name} writes nothing to stderr");
    }
}

#[test]
fn codesign_signs_it_again() {
    // `codesign` signs each file again, which it then accepts, and the file still runs. And it
    // takes each file's signature off. The signature of every name but `abc` is padded, so the
    // signature's command points at bytes that end the file either way, which is what
    // `codesign` needs to do either.
    for (index, name) in NAMES.into_iter().enumerate() {
        let executable = written(SMALLEST, name);
        check(&executable, name);
        let bytes = executable.bytes();
        let is_padded = !signature_bytes_of(name).is_multiple_of(16);
        assert_eq!(is_padded, name != "abc", "only `abc`'s signature needs no padding");
        let path = executable_file(&format!("again-{index}"), name, bytes);
        let signed = run_tool("/usr/bin/codesign", &["--force", "--sign", "-"], &path);
        let said = String::from_utf8_lossy(&signed.stderr);
        assert!(signed.status.success(), "codesign should sign {name} again: {said}");
        let again = fs::read(&path).expect("the file the test wrote can be read");
        assert_ne!(again, bytes, "the signature of {name} was made again");
        let verified = run_tool("/usr/bin/codesign", &["--verify", "--strict"], &path);
        assert!(verified.status.success(), "codesign accepts {name} signed again");
        let run = Command::new(&path).stdin(Stdio::null()).output();
        let run = run.expect("a file the test made executable can be run");
        assert_eq!(run.status.code(), Some(0_i32), "{name}, signed again, exits with 0");
        let path = executable_file(&format!("unsigned-{index}"), name, bytes);
        let unsigned = run_tool("/usr/bin/codesign", &["--remove-signature"], &path);
        let said = String::from_utf8_lossy(&unsigned.stderr);
        assert!(unsigned.status.success(), "codesign should unsign {name}: {said}");
    }
}

#[test]
fn otool_reads_what_the_oracle_states() {
    // `otool -l` prints each load command field by field, as macOS reads the format. For each
    // file, it prints the ten commands and two sections in the oracle's order, and these
    // numbers the oracle states are in the fields otool names for them: every segment's
    // address, size, offset and protection, the code's section, where the program starts, the
    // UUID as a well-formed one, macOS 27.0, and where the signature is.
    for (index, name) in NAMES.into_iter().enumerate() {
        let executable = written(SMALLEST, name);
        check(&executable, name);
        let bytes = executable.bytes();
        let path = executable_file(&format!("otool-{index}"), name, bytes);
        let blocks = otool_blocks(&path);
        let expected = printed_oracle(name, &bytes[UUID_RANGE]);
        let headings: Vec<&str> = blocks.iter().map(|block| block.heading.as_str()).collect();
        let expected_headings: Vec<&str> =
            expected.iter().map(|(heading, _)| heading.as_str()).collect();
        assert_eq!(headings, expected_headings, "the commands and sections of {name}");
        for (block, (heading, fields)) in blocks.iter().zip(&expected) {
            for (key, printed) in fields {
                let found = printed_after(block, key);
                match printed {
                    Printed::Number(number) => {
                        assert_eq!(number_of(found), *number, "`{key}` of {heading} of {name}");
                    }
                    Printed::Text(text) => {
                        assert_eq!(found, text, "`{key}` of {heading} of {name}");
                    }
                }
            }
        }
        // The UUID as otool printed it, by its own text: the third group starts with the
        // version, 8, and the fourth with the variant, which makes its first digit 8 to B.
        let is_identity = |block: &&OtoolBlock| block.heading == "Load command 5";
        let identity = blocks.iter().find(is_identity).expect("the UUID's command is the sixth");
        let groups: Vec<&str> = printed_after(identity, "uuid").split('-').collect();
        assert_eq!(groups.iter().map(|group| group.len()).collect::<Vec<_>>(), [8, 4, 4, 4, 12]);
        assert!(groups[2].starts_with('8'), "the version of {name}'s UUID");
        assert!(groups[3].starts_with(['8', '9', 'A', 'B']), "the variant of {name}'s UUID");
    }
}

#[test]
fn shasum_gives_every_hash_and_the_uuid() {
    // The oracle hashes with the crate's own `sha256`, so here macOS's `shasum` works each hash
    // out again from the file's bytes. The signature holds its hash of each 4 KiB page of
    // `__TEXT`, and the last three pages are zeros. The UUID is the first 16 bytes of its hash
    // of `__TEXT`, with zeros where the UUID goes, and then the name, marked as a UUID.
    for (index, name) in NAMES.into_iter().enumerate() {
        let executable = written(SMALLEST, name);
        check(&executable, name);
        let bytes = executable.bytes();
        let uuid_source = uuid_source_of(bytes, name);
        let mut messages: Vec<&[u8]> = bytes[..TEXT_BYTES].chunks(HASH_PAGE_BYTES).collect();
        assert_eq!(messages.len(), HASH_PAGES_COUNT);
        messages.push(&uuid_source);
        let hashes = shasum(&format!("hashes-{index}"), &messages);
        for page_index in 0..HASH_PAGES_COUNT {
            let held = hex(&bytes[hash_range(name, page_index)]);
            assert_eq!(held, hashes[page_index], "the hash of page {page_index} of {name}");
        }
        assert_eq!(hashes[1..HASH_PAGES_COUNT], [ZERO_PAGE_SHA256; 3], "pages of zeros");
        let uuid = marked(first_16_bytes_of(&hashes[HASH_PAGES_COUNT]));
        assert_eq!(bytes[UUID_RANGE], uuid, "the UUID of {name}");
    }
}
