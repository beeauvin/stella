// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for layouts. `layout` takes three sizes and works out where everything goes, so
//! the tests are three sweeps and the edges: every size of code, every length of name, and a
//! stack on either side of every page edge near the bottom and the top of what it may be.
//!
//! An oracle states each number a layout holds on its own, from the Mach-O format's numbers
//! written out here: a header of 32 bytes, load commands of 624, pages of 16 KiB, hashes of 32
//! bytes for every 4 KiB. It shares nothing with `layout` but the sizes it's asked about, and
//! every layout is compared with it one accessor at a time. Before `layout` sees any sizes, the
//! oracle is checked two ways. Its numbers must hold together, each segment after the one before
//! and on a page. And it must give the file sizes of binaries that macOS ran: `RAN` holds three,
//! and the tests of `write` run the first two again every time, at these very sizes.
//!
//! A size no program has is a bug in the compiler, so each one panics, with a message that says
//! which.

#![forbid(unsafe_code)]

use stella_arm64::{CODE_BYTES_MAX, INSTRUCTION_BYTES, STACK_ALIGNMENT_BYTES};
use stella_macho::{
    BASE_ADDRESS, CODE_OFFSET, HASH_BYTES, HASH_PAGE_BYTES, IMAGE_BYTES_MAX, NAME_BYTES_MAX,
    PAGE_BYTES, SIGNATURE_BYTES_MAX, STACK_BYTES_MAX, Sizes, TEXT_BYTES_MAX, layout,
};
use stella_test_support::Rng;

/// These are the sizes of the smallest program, named `hello`: 11 instructions, no stack, and a
/// name of 5 bytes.
const SMALLEST: Sizes = Sizes { code_bytes: 44, stack_bytes: 0, name_bytes: 5 };

/// These are binaries that macOS ran, each as its sizes and how many bytes its file took. The
/// first two are the smallest program under a short name and the longest. The third was written
/// by hand while the format was worked out, with more code than any program has yet and a stack
/// of a page and 16 bytes, so nothing can run it again today.
const RAN: [(Sizes, u32); 3] = [
    (Sizes { code_bytes: 44, stack_bytes: 0, name_bytes: 5 }, 16_640),
    (Sizes { code_bytes: 44, stack_bytes: 0, name_bytes: 248 }, 16_880),
    (Sizes { code_bytes: 56, stack_bytes: 16_400, name_bytes: 7 }, 16_640),
];

/// This is the most code that leaves `__TEXT` one page: 16 KiB, less the 656 bytes of header and
/// load commands before the code.
const CODE_BYTES_ONE_PAGE: u32 = 16_384 - 656;

/// These are sizes of code at each edge: one instruction, the smallest program, either side of
/// where `__TEXT` needs a second page, and the most.
const CODE_EDGES: [u32; 6] =
    [4, 44, CODE_BYTES_ONE_PAGE - 4, CODE_BYTES_ONE_PAGE, CODE_BYTES_ONE_PAGE + 4, CODE_BYTES_MAX];

/// These are sizes of stack at each edge: none, the least there is, either side of one page and
/// of two, either side of a mebibyte, and the last page and the last 16 bytes of the most.
const STACK_EDGES: [u32; 16] = [
    0,
    16,
    16_368,
    16_384,
    16_400,
    32_752,
    32_768,
    32_784,
    (1 << 20) - 16,
    1 << 20,
    (1 << 20) + 16,
    (1 << 30) - 16_400,
    (1 << 30) - 16_384,
    (1 << 30) - 16_368,
    (1 << 30) - 16,
    1 << 30,
];

/// These are lengths of name at each edge: the shortest, either side of where the signature's
/// padding grows by 16 bytes, the name `hello`, and the longest.
const NAME_EDGES: [u32; 5] = [1, 3, 4, 5, 248];

/// What a layout should be: each number an accessor returns, stated on its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Expected {
    code_address: u64,
    code_bytes: u32,
    text_bytes: u32,
    data_address: u64,
    data_bytes: u32,
    stack_bytes: u32,
    stack_top_address: u64,
    linkedit_address: u64,
    linkedit_offset: u32,
    linkedit_bytes: u32,
    signature_bytes: u32,
    hash_pages_count: u32,
    image_bytes: u32,
}

/// Returns where everything should go for `sizes`, worked out apart from `layout`, from the
/// format's own numbers.
fn oracle(sizes: Sizes) -> Expected {
    let Sizes { code_bytes, stack_bytes, name_bytes } = sizes;
    // `__TEXT` starts above the 4 GiB that `__PAGEZERO` keeps unmapped. It holds the header's 32
    // bytes, the load commands' 624, and then the code, in whole pages of 16 KiB.
    let text_pages_count = (32 + 624 + code_bytes).div_ceil(16_384);
    let text_bytes = text_pages_count * 16_384;
    // `__DATA` follows `__TEXT` in memory. It's the stack, in whole pages, and one page when
    // there's no stack. It takes nothing in the file.
    let data_address = 0x1_0000_0000 + u64::from(text_bytes);
    let data_pages_count = stack_bytes.div_ceil(16_384).max(1);
    let data_bytes = data_pages_count * 16_384;
    // The signature hashes `__TEXT` 4 KiB at a time, so four hashes to a page.
    let hash_pages_count = text_pages_count * 4;
    // The signature is a `SuperBlob`'s header of 12 bytes, its one entry of 8, a `CodeDirectory`'s
    // header of 88, the name and the 0 that ends it, and 32 bytes for each hash.
    let signature_bytes = 12 + 8 + 88 + name_bytes + 1 + 32 * hash_pages_count;
    // `__LINKEDIT` follows `__TEXT` in the file and `__DATA` in memory. It's the signature,
    // padded with zeros to 16 bytes.
    let linkedit_bytes = signature_bytes.div_ceil(16) * 16;
    Expected {
        code_address: 0x1_0000_0000 + 32 + 624,
        code_bytes,
        text_bytes,
        data_address,
        data_bytes,
        stack_bytes,
        stack_top_address: data_address + u64::from(stack_bytes),
        linkedit_address: data_address + u64::from(data_bytes),
        linkedit_offset: text_bytes,
        linkedit_bytes,
        signature_bytes,
        hash_pages_count,
        image_bytes: text_bytes + linkedit_bytes,
    }
}

/// Asserts that `expected`, which the oracle worked out, holds together as a file macOS loads:
/// the segments one after another, each on a page, the code and the stack inside theirs, `sp`
/// aligned, and the signature inside `__LINKEDIT` with a hash for every 4 KiB before it.
fn assert_holds_together(expected: &Expected) {
    let page_bytes: u64 = 16_384;
    let text_end = 0x1_0000_0000 + u64::from(expected.text_bytes);
    assert_eq!(expected.data_address, text_end, "`__DATA` follows `__TEXT`");
    assert!(expected.data_address.is_multiple_of(page_bytes), "`__DATA` starts on a page");
    assert!(expected.linkedit_address.is_multiple_of(page_bytes), "`__LINKEDIT` starts on a page");
    assert!(expected.text_bytes >= 16_384, "`__TEXT` is at least a page");
    assert!(expected.data_bytes >= 16_384, "`__DATA` is at least a page");
    let code_end = expected.code_address + u64::from(expected.code_bytes);
    assert!(code_end <= expected.data_address, "the code ends inside `__TEXT`");
    let data_end = expected.data_address + u64::from(expected.data_bytes);
    assert_eq!(expected.linkedit_address, data_end, "`__LINKEDIT` follows `__DATA`");
    assert!(expected.stack_top_address >= expected.data_address, "the stack is in `__DATA`");
    assert!(expected.stack_top_address <= data_end, "the stack's top is in `__DATA`, or ends it");
    assert!(expected.stack_top_address.is_multiple_of(16), "the stack's top keeps `sp` aligned");
    assert_eq!(expected.linkedit_offset, expected.text_bytes, "the signature follows `__TEXT`");
    assert_eq!(expected.hash_pages_count * 4_096, expected.text_bytes, "a hash for every 4 KiB");
    assert!(
        expected.signature_bytes <= expected.linkedit_bytes,
        "the signature is in `__LINKEDIT`"
    );
    assert!(expected.linkedit_bytes - expected.signature_bytes < 16, "padded by less than 16");
    assert!(expected.linkedit_bytes.is_multiple_of(16), "`__LINKEDIT` is padded to 16 bytes");
    let file_end = expected.linkedit_offset + expected.linkedit_bytes;
    assert_eq!(expected.image_bytes, file_end, "the file ends where `__LINKEDIT` does");
}

/// Checks the layout of `sizes` against the oracle, one accessor at a time, once the oracle's
/// own numbers hold together. Returns what the oracle expected.
fn check(sizes: Sizes) -> Expected {
    let expected = oracle(sizes);
    assert_holds_together(&expected);
    let found = layout(sizes);
    assert_eq!(found.code_address(), expected.code_address, "`code_address` for {sizes:?}");
    assert_eq!(found.code_bytes(), expected.code_bytes, "`code_bytes` for {sizes:?}");
    assert_eq!(found.text_bytes(), expected.text_bytes, "`text_bytes` for {sizes:?}");
    assert_eq!(found.data_address(), expected.data_address, "`data_address` for {sizes:?}");
    assert_eq!(found.data_bytes(), expected.data_bytes, "`data_bytes` for {sizes:?}");
    assert_eq!(found.stack_bytes(), expected.stack_bytes, "`stack_bytes` for {sizes:?}");
    let stack_top_address = found.stack_top_address();
    assert_eq!(stack_top_address, expected.stack_top_address, "`stack_top_address`: {sizes:?}");
    let linkedit_address = found.linkedit_address();
    assert_eq!(linkedit_address, expected.linkedit_address, "`linkedit_address` for {sizes:?}");
    assert_eq!(found.linkedit_offset(), expected.linkedit_offset, "`linkedit_offset`: {sizes:?}");
    assert_eq!(found.linkedit_bytes(), expected.linkedit_bytes, "`linkedit_bytes` for {sizes:?}");
    assert_eq!(found.signature_bytes(), expected.signature_bytes, "`signature_bytes`: {sizes:?}");
    assert_eq!(
        found.hash_pages_count(),
        expected.hash_pages_count,
        "`hash_pages_count`: {sizes:?}"
    );
    assert_eq!(found.image_bytes(), expected.image_bytes, "`image_bytes` for {sizes:?}");
    assert!(found.image_bytes() <= IMAGE_BYTES_MAX, "a file is within the most: {sizes:?}");
    expected
}

#[test]
fn binaries_that_ran() {
    // The oracle is checked first, against the sizes of files macOS ran. Then `layout` is
    // checked against the oracle for the same sizes.
    for (sizes, image_bytes) in RAN {
        let expected = oracle(sizes);
        assert_holds_together(&expected);
        assert_eq!(expected.image_bytes, image_bytes, "the oracle, for {sizes:?}");
    }
    for (sizes, image_bytes) in RAN {
        assert_eq!(check(sizes).image_bytes, image_bytes, "for {sizes:?}");
        assert_eq!(layout(sizes).image_bytes(), image_bytes, "for {sizes:?}");
    }
}

#[test]
fn the_smallest_program() {
    // The smallest program's layout, with every number written out by hand. They're the numbers
    // `otool -l` prints for its binary: `__TEXT` at 4 GiB and one page long, the code 656 bytes
    // in, `__DATA` a page above with the stack's top at its bottom, `__LINKEDIT` a page above
    // that, and a signature of 242 bytes padded to 256.
    let expected = Expected {
        code_address: 0x1_0000_0290,
        code_bytes: 44,
        text_bytes: 16_384,
        data_address: 0x1_0000_4000,
        data_bytes: 16_384,
        stack_bytes: 0,
        stack_top_address: 0x1_0000_4000,
        linkedit_address: 0x1_0000_8000,
        linkedit_offset: 16_384,
        linkedit_bytes: 256,
        signature_bytes: 242,
        hash_pages_count: 4,
        image_bytes: 16_640,
    };
    assert_eq!(oracle(SMALLEST), expected);
    assert_eq!(check(SMALLEST), expected);
}

#[test]
fn the_limits() {
    // Each number the crate gives out is the format's, written out here. The most of everything
    // at once is the largest layout, and it takes exactly the most a file can.
    assert_eq!(BASE_ADDRESS, 0x1_0000_0000);
    assert_eq!(CODE_OFFSET, 656);
    assert_eq!(PAGE_BYTES, 16_384);
    assert_eq!(HASH_PAGE_BYTES, 4_096);
    assert_eq!(HASH_BYTES, 32);
    assert_eq!(NAME_BYTES_MAX, 248);
    assert_eq!(STACK_BYTES_MAX, 1_073_741_824);
    // The most code needs a second page of `__TEXT`, and not a third.
    assert_eq!(TEXT_BYTES_MAX, 32_768);
    assert_eq!((656 + CODE_BYTES_MAX).div_ceil(16_384), 2);
    // The headers' 108 bytes, the longest name and its 0, and eight hashes.
    assert_eq!(SIGNATURE_BYTES_MAX, 108 + 249 + 8 * 32);
    assert_eq!(IMAGE_BYTES_MAX, 33_392);
    let most = Sizes {
        code_bytes: CODE_BYTES_MAX,
        stack_bytes: STACK_BYTES_MAX,
        name_bytes: NAME_BYTES_MAX,
    };
    let expected = check(most);
    assert_eq!(expected.text_bytes, TEXT_BYTES_MAX);
    assert_eq!(expected.signature_bytes, SIGNATURE_BYTES_MAX);
    assert_eq!(expected.image_bytes, IMAGE_BYTES_MAX);
    assert_eq!(expected.data_bytes, STACK_BYTES_MAX, "the most stack is whole pages");
    // The stack's top is as far from the code as it gets: a little over 1 GiB, well within the
    // 4 GiB that `adrp` reaches.
    assert_eq!(expected.stack_top_address - expected.code_address, 1_073_741_824 + 32_768 - 656);
    // The edges are the sizes they're said to be.
    assert_eq!(CODE_EDGES.map(|code_bytes| code_bytes % INSTRUCTION_BYTES), [0; 6]);
    assert_eq!(STACK_EDGES.map(|stack_bytes| stack_bytes % STACK_ALIGNMENT_BYTES), [0; 16]);
    assert_eq!(CODE_EDGES.last(), Some(&CODE_BYTES_MAX));
    assert_eq!(STACK_EDGES.last(), Some(&STACK_BYTES_MAX));
    assert_eq!(NAME_EDGES.last(), Some(&NAME_BYTES_MAX));
}

#[test]
fn every_code_size() {
    // Every size of code there is, from one instruction to the most, with each edge of stack and
    // of name. Code that fills the first page exactly leaves `__TEXT` one page, and one
    // instruction more makes it two, with four more hashes.
    let mut tried_count: u32 = 0;
    let mut two_pages_count: u32 = 0;
    for code_bytes in (INSTRUCTION_BYTES..=CODE_BYTES_MAX).step_by(4) {
        for stack_bytes in STACK_EDGES {
            for name_bytes in NAME_EDGES {
                let expected = check(Sizes { code_bytes, stack_bytes, name_bytes });
                let pages_count = if code_bytes <= CODE_BYTES_ONE_PAGE { 1 } else { 2 };
                assert_eq!(expected.text_bytes, pages_count * 16_384, "for {code_bytes} bytes");
                assert_eq!(expected.hash_pages_count, pages_count * 4, "for {code_bytes} bytes");
                tried_count += 1;
                two_pages_count += u32::from(pages_count == 2);
            }
        }
    }
    assert_eq!(tried_count, (CODE_BYTES_MAX / 4) * 16 * 5);
    assert_eq!(two_pages_count, ((CODE_BYTES_MAX - CODE_BYTES_ONE_PAGE) / 4) * 16 * 5);
    assert!(two_pages_count > 0, "some code needs a second page");
}

#[test]
fn every_name_length() {
    // Every length of name there is, with each edge of code and of stack. A name changes only
    // the signature: each byte makes it a byte longer, and `__LINKEDIT` 16 bytes longer when the
    // padding runs out. Nothing in memory moves.
    for code_bytes in CODE_EDGES {
        for stack_bytes in STACK_EDGES {
            let shortest = check(Sizes { code_bytes, stack_bytes, name_bytes: 1 });
            for name_bytes in 1..=NAME_BYTES_MAX {
                let expected = check(Sizes { code_bytes, stack_bytes, name_bytes });
                let longer_bytes = name_bytes - 1;
                assert_eq!(expected.signature_bytes, shortest.signature_bytes + longer_bytes);
                let moved = Expected {
                    linkedit_bytes: shortest.linkedit_bytes,
                    signature_bytes: shortest.signature_bytes,
                    image_bytes: shortest.image_bytes,
                    ..expected
                };
                assert_eq!(moved, shortest, "a name of {name_bytes} bytes moves nothing else");
            }
        }
    }
    // With the smallest program, a name of 3 bytes fills the padding exactly, and 4 need more.
    assert_eq!(check(Sizes { name_bytes: 3, ..SMALLEST }).linkedit_bytes, 240);
    assert_eq!(check(Sizes { name_bytes: 3, ..SMALLEST }).signature_bytes, 240);
    assert_eq!(check(Sizes { name_bytes: 4, ..SMALLEST }).linkedit_bytes, 256);
}

#[test]
fn every_stack_edge() {
    // Every stack from none to five pages, 16 bytes at a time, and each edge up to the most, with
    // each edge of code. `__DATA` is the stack in whole pages, and a page when there's none. The
    // stack's top is its size above `__DATA`'s start, and a stack changes nothing in the file.
    let near: Vec<u32> = (0..=5 * 16_384).step_by(16).collect();
    for code_bytes in CODE_EDGES {
        let none = check(Sizes { code_bytes, stack_bytes: 0, name_bytes: 5 });
        for &stack_bytes in near.iter().chain(&STACK_EDGES) {
            let expected = check(Sizes { code_bytes, stack_bytes, name_bytes: 5 });
            let pages_count = stack_bytes.div_ceil(16_384).max(1);
            assert_eq!(expected.data_bytes, pages_count * 16_384, "for {stack_bytes} bytes");
            let stack_top_address = none.data_address + u64::from(stack_bytes);
            assert_eq!(expected.stack_top_address, stack_top_address, "for {stack_bytes} bytes");
            assert_eq!(expected.image_bytes, none.image_bytes, "for {stack_bytes} bytes");
            assert_eq!(expected.linkedit_offset, none.linkedit_offset, "for {stack_bytes} bytes");
        }
    }
    // A stack of exactly a page fills `__DATA`, so its top is where `__LINKEDIT` starts.
    let page = check(Sizes { stack_bytes: 16_384, ..SMALLEST });
    assert_eq!(page.data_bytes, 16_384);
    assert_eq!(page.stack_top_address, page.linkedit_address);
    assert_eq!(check(Sizes { stack_bytes: 16_400, ..SMALLEST }).data_bytes, 32_768);
}

#[test]
fn fuzz_against_the_oracle() {
    // 100,000 random sizes, each one a program could have: whole instructions, a stack that keeps
    // `sp` aligned, a name of any length. Every layout agrees with the oracle. One in eight has the
    // most of something, and layouts of one page of `__TEXT` and of two are both counted, so the
    // test fails if the sizes stop reaching either.
    let mut rng = Rng::new(0x4C41_594F_5554_0001);
    let below = |rng: &mut Rng, count: u32| {
        let count = usize::try_from(count).expect("a `u32` fits in a `usize`");
        u32::try_from(rng.below(count)).expect("a number below a `u32` is a `u32`")
    };
    let mut one_page_count: u32 = 0;
    let mut two_pages_count: u32 = 0;
    for _ in 0..100_000_u32 {
        let most = if rng.below(8) == 0 { rng.below(3) } else { 3 };
        let code_instructions_count = 1 + below(&mut rng, CODE_BYTES_MAX / INSTRUCTION_BYTES);
        let stack_steps_count = below(&mut rng, STACK_BYTES_MAX / STACK_ALIGNMENT_BYTES + 1);
        let sizes = Sizes {
            code_bytes: if most == 0 { CODE_BYTES_MAX } else { code_instructions_count * 4 },
            stack_bytes: if most == 1 { STACK_BYTES_MAX } else { stack_steps_count * 16 },
            name_bytes: if most == 2 { NAME_BYTES_MAX } else { 1 + below(&mut rng, 248) },
        };
        let expected = check(sizes);
        one_page_count += u32::from(expected.text_bytes == 16_384);
        two_pages_count += u32::from(expected.text_bytes == 32_768);
    }
    assert_eq!(one_page_count + two_pages_count, 100_000, "`__TEXT` is one page or two");
    assert!(one_page_count >= 10_000, "{one_page_count} layouts of one page");
    assert!(two_pages_count >= 1_000, "{two_pages_count} layouts of two pages");
}

#[test]
fn layouts_compare_by_their_numbers() {
    // A layout is a value: the same sizes always give an equal one, a copy is equal to what it
    // was copied from, and changing any one size gives a layout that isn't equal.
    let smallest = layout(SMALLEST);
    let copy = smallest;
    assert_eq!(smallest, copy);
    assert_eq!(smallest, layout(SMALLEST));
    assert_ne!(smallest, layout(Sizes { code_bytes: 48, ..SMALLEST }));
    assert_ne!(smallest, layout(Sizes { stack_bytes: 16, ..SMALLEST }));
    assert_ne!(smallest, layout(Sizes { name_bytes: 6, ..SMALLEST }));
    assert_eq!(SMALLEST, Sizes { code_bytes: 44, stack_bytes: 0, name_bytes: 5 });
}

#[test]
#[should_panic(expected = "the code is at least one instruction")]
fn no_code() {
    // Every program has the switch, so code of no bytes is a bug, caught when it's laid out.
    _ = layout(Sizes { code_bytes: 0, ..SMALLEST });
}

#[test]
#[should_panic(expected = "the code is whole instructions")]
fn part_of_an_instruction() {
    // An instruction is 4 bytes, so 46 bytes of code is eleven instructions and half of another.
    _ = layout(Sizes { code_bytes: 46, ..SMALLEST });
}

#[test]
#[should_panic(expected = "the code is at most `CODE_BYTES_MAX` bytes")]
fn more_code_than_fits() {
    // One instruction more than the most code is past what the file has room for.
    _ = layout(Sizes { code_bytes: CODE_BYTES_MAX + 4, ..SMALLEST });
}

#[test]
#[should_panic(expected = "the stack keeps `sp` aligned")]
fn a_stack_that_isnt_aligned() {
    // A stack of 8 bytes would leave its top, where `sp` starts, off a multiple of 16.
    _ = layout(Sizes { stack_bytes: 8, ..SMALLEST });
}

#[test]
#[should_panic(expected = "the stack is at most `STACK_BYTES_MAX` bytes")]
fn more_stack_than_fits() {
    // 16 bytes more than the most stack is past what the code is sure to reach.
    _ = layout(Sizes { stack_bytes: STACK_BYTES_MAX + 16, ..SMALLEST });
}

#[test]
#[should_panic(expected = "a binary's name is at least one byte")]
fn a_name_of_no_bytes() {
    // `BinaryName::new` refuses an empty name, so a name of no bytes can't reach a layout.
    _ = layout(Sizes { name_bytes: 0, ..SMALLEST });
}

#[test]
#[should_panic(expected = "a binary's name is at most `NAME_BYTES_MAX` bytes")]
fn a_name_too_long() {
    // `BinaryName::new` refuses a name of 249 bytes, so one can't reach a layout.
    _ = layout(Sizes { name_bytes: NAME_BYTES_MAX + 1, ..SMALLEST });
}
