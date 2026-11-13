// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The header and the load commands, which start the file and say what the rest of it is.
//!
//! macOS insists on most of what's here: the `__PAGEZERO` segment, which keeps the lowest 4 GiB
//! unmapped, the loader's path, a UUID, where the program starts, libSystem, and a signature.
//! The build version says the binary needs macOS 15.7.5 or later. Without any one of those,
//! macOS won't run the binary.
//!
//! The symbol table and the loader's view of it are there though both are empty, since a
//! program has no symbols. The loader on macOS 15 and 26 reads a binary that has nothing to fix
//! up through the second, and ends the program with a segmentation fault when it isn't there,
//! before any of the program's code runs. It wants the first wherever there's the second. The
//! system's linker writes both into every program.
//!
//! Each number's name here is this module's own. The name macOS's `<mach-o/loader.h>` gives it
//! is beside it, to look it up by.

use stella_arm64::{Code, INSTRUCTION_BYTES, STACK_ALIGNMENT_BYTES};
use stella_source::usize_from_u32;

use crate::image::Image;
use crate::layout::{BASE_ADDRESS, CODE_OFFSET, Layout, PAGE_BYTES};

/// This is how many bytes the header takes.
pub(crate) const HEADER_BYTES: u32 = 32;

/// This is how many load commands there are.
pub(crate) const COMMANDS_COUNT: u32 = 12;

/// This is how many bytes the load commands take, all twelve, in the order they're written.
pub(crate) const COMMANDS_BYTES: u32 = SEGMENT_BYTES
    + (SEGMENT_BYTES + SECTION_BYTES)
    + (SEGMENT_BYTES + SECTION_BYTES)
    + SEGMENT_BYTES
    + SYMBOLS_BYTES
    + LOADER_SYMBOLS_BYTES
    + LOADER_BYTES
    + UUID_COMMAND_BYTES
    + BUILD_VERSION_BYTES
    + MAIN_BYTES
    + LIBRARY_BYTES
    + SIGNATURE_COMMAND_BYTES;

/// This is how many bytes a UUID takes.
pub(crate) const UUID_BYTES: u32 = 16;

/// This is where the UUID is in the file: in the eighth command, after its kind and its size.
pub(crate) const UUID_OFFSET: u32 = HEADER_BYTES
    + SEGMENT_BYTES
    + (SEGMENT_BYTES + SECTION_BYTES)
    + (SEGMENT_BYTES + SECTION_BYTES)
    + SEGMENT_BYTES
    + SYMBOLS_BYTES
    + LOADER_SYMBOLS_BYTES
    + LOADER_BYTES
    + COMMAND_START_BYTES;

/// These are the load commands' kinds, in the order they're written, which is the order the
/// system's linker writes them in.
pub(crate) const COMMAND_KINDS: [u32; usize_from_u32(COMMANDS_COUNT)] = [
    SEGMENT,
    SEGMENT,
    SEGMENT,
    SEGMENT,
    SYMBOLS,
    LOADER_SYMBOLS,
    LOADER,
    UUID,
    BUILD_VERSION,
    MAIN,
    LIBRARY,
    SIGNATURE,
];

/// A file that starts with this is a 64-bit Mach-O (`MH_MAGIC_64`).
pub(crate) const MAGIC: u32 = 0xFEED_FACF;

/// Every load command starts with its kind and its size, 4 bytes each.
const COMMAND_START_BYTES: u32 = 8;

/// A segment command, before its sections (`LC_SEGMENT_64`), and how many bytes it takes.
const SEGMENT: u32 = 0x19;
const SEGMENT_BYTES: u32 = 72;

/// This is how many bytes a section takes, after its segment's command.
const SECTION_BYTES: u32 = 80;

/// The symbol table (`LC_SYMTAB`), and how many bytes it takes: where the symbols are and how
/// many, and where their names are and how many bytes those take. All four are 0.
const SYMBOLS: u32 = 0x2;
const SYMBOLS_BYTES: u32 = 24;

/// The loader's view of the symbol table (`LC_DYSYMTAB`), and how many bytes it takes: nine
/// places in the file, each with how many things are there, which are the symbols in three
/// groups, the tables of contents, of modules and of symbols this binary refers to, the
/// indirect symbols, and the relocations against outside symbols and local ones. All eighteen
/// are 0.
const LOADER_SYMBOLS: u32 = 0xB;
const LOADER_SYMBOLS_BYTES: u32 = 80;

/// The loader's path (`LC_LOAD_DYLINKER`), how many bytes it takes, and where its text starts.
const LOADER: u32 = 0xE;
const LOADER_BYTES: u32 = 32;
const LOADER_TEXT_OFFSET: u32 = 12;

/// The UUID (`LC_UUID`), and how many bytes its command takes.
const UUID: u32 = 0x1B;
const UUID_COMMAND_BYTES: u32 = COMMAND_START_BYTES + UUID_BYTES;

/// The build version (`LC_BUILD_VERSION`), and how many bytes it takes.
const BUILD_VERSION: u32 = 0x32;
const BUILD_VERSION_BYTES: u32 = 24;

/// Where the program starts (`LC_MAIN`), and how many bytes it takes.
const MAIN: u32 = 0x8000_0028;
const MAIN_BYTES: u32 = 24;

/// A library the program loads (`LC_LOAD_DYLIB`), how many bytes it takes, and where its text
/// starts.
const LIBRARY: u32 = 0xC;
const LIBRARY_BYTES: u32 = 56;
const LIBRARY_TEXT_OFFSET: u32 = 24;

/// Where the code signature is (`LC_CODE_SIGNATURE`), and how many bytes it takes.
const SIGNATURE: u32 = 0x1D;
const SIGNATURE_COMMAND_BYTES: u32 = 16;

/// The processor is arm64 (`CPU_TYPE_ARM64`), of any model (`CPU_SUBTYPE_ARM64_ALL`).
const PROCESSOR_ARM64: u32 = 0x0100_000C;
const PROCESSOR_MODEL_ANY: u32 = 0;

/// The file is a program to run (`MH_EXECUTE`).
const FILE_KIND_EXECUTABLE: u32 = 2;

/// The header's flags: no undefined symbols (`MH_NOUNDEFS`), for the loader (`MH_DYLDLINK`),
/// with two-level names (`MH_TWOLEVEL`), loadable at any address (`MH_PIE`).
const HEADER_FLAGS: u32 = 0x1 | 0x4 | 0x80 | 0x20_0000;

/// What a segment's memory may be used for (`VM_PROT_READ`, `VM_PROT_WRITE`,
/// `VM_PROT_EXECUTE`).
const MAY_READ: u32 = 1;
const MAY_WRITE: u32 = 2;
const MAY_RUN: u32 = 4;

/// A section that holds only instructions (`S_ATTR_PURE_INSTRUCTIONS` and
/// `S_ATTR_SOME_INSTRUCTIONS`).
const SECTION_INSTRUCTIONS: u32 = 0x8000_0400;

/// A section that starts as zeros and takes no room in the file (`S_ZEROFILL`).
const SECTION_ZEROS: u32 = 0x1;

/// The code is aligned to 4 bytes, and the stack to 16, each as a power of two.
const CODE_ALIGNMENT_POWER: u32 = 2;
const STACK_ALIGNMENT_POWER: u32 = 4;

const _: () = assert!(
    INSTRUCTION_BYTES == 1 << CODE_ALIGNMENT_POWER,
    "the code's section is aligned to an instruction"
);
const _: () = assert!(
    STACK_ALIGNMENT_BYTES == 1 << STACK_ALIGNMENT_POWER,
    "the stack's section is aligned as the stack is"
);

/// The platform is macOS (`PLATFORM_MACOS`). Each version holds its major number in the high 16
/// bits, then its minor and its patch in 8 bits each.
const PLATFORM_MACOS: u32 = 1;

/// This is the oldest macOS a binary says it runs on: 15.7.5, the oldest release it's been seen
/// to run on, which CI runs it on every time. Nothing older was there to try. The loader
/// doesn't hold a binary to this, so it's only true if it's what was seen.
const MACOS_MINIMUM: u32 = (15 << 16) | (7 << 8) | 5;

/// This is the SDK a binary says it was made against: macOS 27.0's, the newest, so the system's
/// libraries give it their behaviour of today rather than an older one they keep for programs
/// made against older SDKs.
const MACOS_SDK: u32 = 27 << 16;

/// libSystem's version on macOS 27.2, 1359.40.3, and the version a program needs it to
/// be compatible with, 1.0.0. The loader checks only the second.
const LIBRARY_VERSION: u32 = (1359 << 16) | (40 << 8) | 3;
const LIBRARY_COMPATIBILITY_VERSION: u32 = 1 << 16;

/// The timestamp a linker gives a library it didn't bind to in advance.
const LIBRARY_TIMESTAMP: u32 = 2;

/// A segment's or a section's name is 16 bytes, padded with zeros.
const NAME_FIELD_BYTES: u32 = 16;

/// A segment is a run of the program's memory, and the part of the file it's loaded from.
struct Segment<'name> {
    /// The segment's name, such as `__TEXT`.
    name: &'name str,
    /// Where the segment starts in memory.
    address: u64,
    /// How many bytes the segment takes in memory.
    memory_bytes: u64,
    /// Where the segment's bytes start in the file.
    file_offset: u64,
    /// How many bytes the segment takes in the file.
    file_bytes: u64,
    /// What the segment's memory may be used for.
    protection: u32,
    /// The segment's one section, if it has one.
    section: Option<Section<'name>>,
}

/// A section is a part of a segment that holds one kind of thing.
struct Section<'name> {
    /// The section's name, such as `__text`.
    name: &'name str,
    /// Where the section starts in memory.
    address: u64,
    /// How many bytes the section takes in memory.
    memory_bytes: u64,
    /// Where the section's bytes start in the file, or 0 when it takes none.
    file_offset: u32,
    /// The section's alignment, as a power of two.
    alignment_power: u32,
    /// What the section holds.
    kind: u32,
}

/// Adds the header and the load commands for `layout` to `image`, which holds nothing yet, with
/// the UUID left as zeros to be filled in.
pub(crate) fn push_header_and_commands(image: &mut Image, layout: &Layout) {
    assert_eq!(image.offset(), 0, "the header starts the file");
    let reserved = 0;
    image.push_u32s(&[
        MAGIC,
        PROCESSOR_ARM64,
        PROCESSOR_MODEL_ANY,
        FILE_KIND_EXECUTABLE,
        COMMANDS_COUNT,
        COMMANDS_BYTES,
        HEADER_FLAGS,
        reserved,
    ]);
    assert_eq!(image.offset(), HEADER_BYTES, "the header is 32 bytes");
    push_segments(image, layout);
    // The symbol tables say there's nothing in them: every place and every count is 0.
    image.push_u32s(&[SYMBOLS, SYMBOLS_BYTES]);
    image.pad_to(image.offset() + SYMBOLS_BYTES - COMMAND_START_BYTES);
    image.push_u32s(&[LOADER_SYMBOLS, LOADER_SYMBOLS_BYTES]);
    image.pad_to(image.offset() + LOADER_SYMBOLS_BYTES - COMMAND_START_BYTES);
    image.push_u32s(&[LOADER, LOADER_BYTES, LOADER_TEXT_OFFSET]);
    image.push_padded_text("/usr/lib/dyld", LOADER_BYTES - LOADER_TEXT_OFFSET);
    image.push_u32s(&[UUID, UUID_COMMAND_BYTES]);
    assert_eq!(image.offset(), UUID_OFFSET, "the UUID is where it's filled in later");
    image.pad_to(UUID_OFFSET + UUID_BYTES);
    let tools_count = 0;
    image.push_u32s(&[
        BUILD_VERSION,
        BUILD_VERSION_BYTES,
        PLATFORM_MACOS,
        MACOS_MINIMUM,
        MACOS_SDK,
        tools_count,
    ]);
    image.push_u32s(&[MAIN, MAIN_BYTES]);
    // The program starts at the code's entry, and the system's stack is whatever size the
    // system makes it: Stella's code runs on a stack of its own.
    let system_stack_bytes = 0;
    image.push_u64s(&[u64::from(CODE_OFFSET + Code::ENTRY_OFFSET_BYTES), system_stack_bytes]);
    image.push_u32s(&[
        LIBRARY,
        LIBRARY_BYTES,
        LIBRARY_TEXT_OFFSET,
        LIBRARY_TIMESTAMP,
        LIBRARY_VERSION,
        LIBRARY_COMPATIBILITY_VERSION,
    ]);
    image.push_padded_text("/usr/lib/libSystem.B.dylib", LIBRARY_BYTES - LIBRARY_TEXT_OFFSET);
    // The command counts the padding after the signature as the signature's, as the system's
    // linker does, so what it points at ends the file. `codesign` won't sign a binary again,
    // or take its signature off, unless it does.
    image.push_u32s(&[
        SIGNATURE,
        SIGNATURE_COMMAND_BYTES,
        layout.linkedit_offset(),
        layout.linkedit_bytes(),
    ]);
    assert_eq!(image.offset(), CODE_OFFSET, "the code follows the commands");
}

/// Adds the four segments' commands for `layout` to `image`: `__PAGEZERO`, `__TEXT` with the
/// code, `__DATA` with the stack, and `__LINKEDIT`.
fn push_segments(image: &mut Image, layout: &Layout) {
    let text_bytes = u64::from(layout.text_bytes());
    let code = Section {
        name: "__text",
        address: layout.code_address(),
        memory_bytes: u64::from(layout.code_bytes()),
        file_offset: CODE_OFFSET,
        alignment_power: CODE_ALIGNMENT_POWER,
        kind: SECTION_INSTRUCTIONS,
    };
    let stack = Section {
        name: "__bss",
        address: layout.data_address(),
        memory_bytes: u64::from(layout.stack_bytes()),
        file_offset: 0,
        alignment_power: STACK_ALIGNMENT_POWER,
        kind: SECTION_ZEROS,
    };
    let segments = [
        // The lowest 4 GiB can't be read, written or run, so a null pointer faults.
        Segment {
            name: "__PAGEZERO",
            address: 0,
            memory_bytes: BASE_ADDRESS,
            file_offset: 0,
            file_bytes: 0,
            protection: 0,
            section: None,
        },
        Segment {
            name: "__TEXT",
            address: BASE_ADDRESS,
            memory_bytes: text_bytes,
            file_offset: 0,
            file_bytes: text_bytes,
            protection: MAY_READ | MAY_RUN,
            section: Some(code),
        },
        // `__DATA` starts as zeros, so it takes no room in the file.
        Segment {
            name: "__DATA",
            address: layout.data_address(),
            memory_bytes: u64::from(layout.data_bytes()),
            file_offset: 0,
            file_bytes: 0,
            protection: MAY_READ | MAY_WRITE,
            section: Some(stack),
        },
        Segment {
            name: "__LINKEDIT",
            address: layout.linkedit_address(),
            memory_bytes: u64::from(layout.linkedit_bytes().next_multiple_of(PAGE_BYTES)),
            file_offset: u64::from(layout.linkedit_offset()),
            file_bytes: u64::from(layout.linkedit_bytes()),
            protection: MAY_READ,
            section: None,
        },
    ];
    for segment in &segments {
        push_segment(image, segment);
    }
}

/// Adds `segment`'s command to `image`, and its section's after it, if it has one.
fn push_segment(image: &mut Image, segment: &Segment<'_>) {
    assert!(segment.protection <= MAY_READ | MAY_WRITE | MAY_RUN, "a protection is these three");
    assert!(segment.file_bytes <= segment.memory_bytes, "a segment is loaded into its memory");
    let start = image.offset();
    let sections_count = u32::from(segment.section.is_some());
    image.push_u32s(&[SEGMENT, SEGMENT_BYTES + SECTION_BYTES * sections_count]);
    image.push_padded_text(segment.name, NAME_FIELD_BYTES);
    image.push_u64s(&[
        segment.address,
        segment.memory_bytes,
        segment.file_offset,
        segment.file_bytes,
    ]);
    // The most the memory may ever be used for is what it may be used for from the start.
    let flags = 0;
    image.push_u32s(&[segment.protection, segment.protection, sections_count, flags]);
    assert_eq!(image.offset() - start, SEGMENT_BYTES, "a segment's command is 72 bytes");
    let Some(section) = &segment.section else {
        return;
    };
    let start = image.offset();
    image.push_padded_text(section.name, NAME_FIELD_BYTES);
    image.push_padded_text(segment.name, NAME_FIELD_BYTES);
    image.push_u64s(&[section.address, section.memory_bytes]);
    // No relocations, and nothing in the three fields a kind of section may use.
    let (relocations_offset, relocations_count, reserved) = (0, 0, 0);
    image.push_u32s(&[
        section.file_offset,
        section.alignment_power,
        relocations_offset,
        relocations_count,
        section.kind,
        reserved,
        reserved,
        reserved,
    ]);
    assert_eq!(image.offset() - start, SECTION_BYTES, "a section is 80 bytes");
}
