// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for generating code. They're a crate of their own, so the compiler lets them
//! reach only what the crates make public: every source goes through every check, from `lint`
//! to `check_program`, and then `generate`.
//!
//! Every program today has the same code, so it's stated here as the instructions it is, and
//! the system's assembler and linker say what their words are, as each test runs: `as` makes
//! the switch from its assembly, and `ld` places it against stacks of several sizes. Then
//! placements are swept across their edges against an oracle that works out, from two
//! addresses alone, how far `adrp` must reach. The instructions' own words are the
//! instruction tests' to check.

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use stella_arm64::{
    CODE_BYTES_MAX, CODE_INSTRUCTIONS_MAX, Code, INSTRUCTION_BYTES, Instruction,
    PAGE_ADDRESS_PAGE_BYTES, Placement, Register, STACK_ALIGNMENT_BYTES, StackOrRegister, generate,
};
use stella_ast::build;
use stella_blocks::nest;
use stella_grammar::parse;
use stella_lexer::lex;
use stella_linter_text::lint;
use stella_linter_tokens::lint_tokens;
use stella_meaning::{check_program, resolve};
use stella_source::{SOURCE_BYTES_MAX, usize_from_u32};
use stella_test_support::{Run, Scratch, assemble, text};

/// This is the smallest program, laid out the one way.
const SMALLEST: &str = "function main() returns Void {\n}\n";

/// This is the switch and `main`, as they're written for `as`, with the stack's top left as a
/// place to say where it is: `{PAGE}` and `{OFFSET}`.
const SWITCH_ASSEMBLY: &str = "\
.text
.globl _main
.p2align 2
_main:
    mov x9, sp
    mov x10, x30
    adrp x11, {PAGE}
    add x11, x11, {OFFSET}
    mov sp, x11
    bl _stella_main
    mov sp, x9
    mov x30, x10
    mov w0, #0
    ret
_stella_main:
    ret
";

/// This is how many instructions the code of today's one program takes: ten for the switch, and
/// one for `main`.
const INSTRUCTIONS_COUNT: usize = 11;

/// This is the index, in the code, of the `adrp` that finds the stack's top. The `add` that
/// finishes it comes next.
const PAGE_INDEX: usize = 2;

/// This is how far back `adrp` reaches, in bytes: 4 GiB. It reaches one page less on.
const REACH_BYTES: u64 = 1 << 32;

/// The register the switch finds the stack's top in.
const STACK_TOP: Register = Register::new(11);

/// Returns a new, empty directory for one test, or one row of a table, to work in.
fn fresh_directory(name: &str) -> PathBuf {
    Scratch::of("generate").fresh(name)
}

/// Generates the code for `source`, which must pass every check.
fn generated(source: &str) -> Code {
    let text = lint(source.as_bytes()).expect("source should pass the text lint");
    let tokens = lex(text).expect("source should lex");
    let layout = lint_tokens(tokens).expect("source should be laid out the one way");
    let forms = parse(nest(layout)).expect("source should pass the grammar");
    let types = resolve(build(forms)).expect("source should pass the types");
    let program = check_program(types).expect("source should be a program");
    generate(&program)
}

/// Returns `bytes`, a code's bytes as they're written, as the words the processor reads.
fn words_of(bytes: &[u8]) -> Vec<u32> {
    let (words, rest) = bytes.as_chunks::<4>();
    assert!(rest.is_empty(), "code is whole instructions of four bytes");
    words.iter().map(|&word| u32::from_le_bytes(word)).collect()
}

/// Returns the words of `code` placed with its first instruction at `code_address` and its
/// stack's top at `stack_top_address`.
fn placed(code: &Code, placement: Placement) -> Vec<u32> {
    words_of(&code.placed(placement))
}

/// Returns the two instructions that find a stack's top at `stack_top_address` from code at
/// `code_address`, worked out from the two addresses alone: how many 4 KiB pages on the top's
/// page is from the page the `adrp` is in, and where the top is in its page.
fn stack_top_oracle(placement: Placement) -> [Instruction; 2] {
    let Placement { code_address, stack_top_address } = placement;
    let page_bytes = i128::from(PAGE_ADDRESS_PAGE_BYTES);
    let instruction_bytes = i128::from(INSTRUCTION_BYTES);
    let page_index = i128::try_from(PAGE_INDEX).expect("an index in the switch is small");
    let adrp_address = i128::from(code_address) + page_index * instruction_bytes;
    let stack_top = i128::from(stack_top_address);
    let pages = stack_top.div_euclid(page_bytes) - adrp_address.div_euclid(page_bytes);
    let offset = stack_top.rem_euclid(page_bytes);
    let top = StackOrRegister::Register(STACK_TOP);
    [
        Instruction::PageAddress {
            target: STACK_TOP,
            pages: i32::try_from(pages).expect("the tests place a stack `adrp` reaches"),
        },
        Instruction::AddImmediate {
            target: top,
            source: top,
            immediate: u16::try_from(offset).expect("an offset in a page fits in 16 bits"),
        },
    ]
}

/// Returns the switch and `main` as they're written for `as`, with the stack's top written
/// as `page`, for the `adrp`, and `offset`, for the `add`.
fn switch_assembly(page: &str, offset: &str) -> String {
    SWITCH_ASSEMBLY.replace("{PAGE}", page).replace("{OFFSET}", offset)
}

/// Returns the address `nm` gives the symbol `symbol` in the binary at `path`.
fn symbol_address(path: &Path, symbol: &str) -> u64 {
    let run = Run::of(Command::new("/usr/bin/nm").arg(path));
    assert!(run.status == 0_i32, "`nm` should read {}: {}", path.display(), run.stderr);
    let line = run.stdout.lines().find(|line| line.split_whitespace().last() == Some(symbol));
    let line = line.expect("the binary should hold the symbol");
    let address = line.split_whitespace().next().expect("a symbol's line starts with its address");
    u64::from_str_radix(address, 16).expect("an address is written in hexadecimal")
}

/// Returns a source of exactly `SOURCE_BYTES_MAX` bytes: `main` holding the longest comment
/// that fits.
fn longest_comment() -> String {
    let around = "function main() returns Void {\n    // \n}\n".len();
    let comment = "a".repeat(usize_from_u32(SOURCE_BYTES_MAX) - around);
    let source = format!("function main() returns Void {{\n    // {comment}\n}}\n");
    assert_eq!(source.len(), usize_from_u32(SOURCE_BYTES_MAX), "the source is the largest");
    source
}

/// Returns a source of `count` comments and then `main`.
fn comments_then_main(count: usize) -> String {
    format!("{}{SMALLEST}", "//\n".repeat(count))
}

#[test]
fn the_smallest_program() {
    // The smallest program's code is 11 instructions, 44 bytes, starts at its first, and needs
    // no stack. Placed where a stack's top is four pages on and at the start of its page, it's
    // word for word what `as` makes of the switch and `main` written out, with `adrp` and
    // `add` given those very numbers.
    let code = generated(SMALLEST);
    assert_eq!(code.size_bytes(), 44);
    assert_eq!(code.stack_bytes(), 0);
    assert_eq!(Code::ENTRY_OFFSET_BYTES, 0);
    assert_eq!((INSTRUCTION_BYTES, STACK_ALIGNMENT_BYTES), (4, 16));
    let directory = fresh_directory("smallest");
    let object = assemble(&directory, "switch", &switch_assembly("#16384", "#0"));
    let assembled = text(&object).words;
    assert_eq!(assembled.len(), INSTRUCTIONS_COUNT, "the switch and `main` are 11 instructions");
    let placement = Placement { code_address: 0x1_0000_0290, stack_top_address: 0x1_0000_4000 };
    assert_eq!(placed(&code, placement), assembled);
    // Each word is one of Stella's instructions, and the last two are the returns: the
    // switch's, to the loader, and `main`'s.
    let read: Vec<Option<Instruction>> =
        assembled.iter().map(|&word| Instruction::decode(word)).collect();
    assert!(read.iter().all(Option::is_some), "every word is an instruction of Stella's");
    assert_eq!(read[9..], [Some(Instruction::Return), Some(Instruction::Return)]);
    assert_eq!(read[5], Some(Instruction::BranchWithLink { instructions: 5_i32 }), "`bl main`");
}

#[test]
fn the_linker_finds_the_stack_the_same_way() {
    // The switch, assembled with its stack's top as a symbol and an offset, and linked by `ld`
    // against stacks of several sizes. `ld` puts the code and the stack where it likes and
    // makes the `adrp` and the `add` for there. Placed at the same two addresses, Stella's
    // code is the same words, and the oracle's two instructions are what `ld` made.
    let code = generated(SMALLEST);
    for stack_bytes in [16_u32, 48, 4_080, 4_096, 16_384, 65_536 + 32] {
        let directory = fresh_directory(&format!("linked-{stack_bytes}"));
        let page = format!("_stack@PAGE+{stack_bytes}");
        let offset = format!("_stack@PAGEOFF+{stack_bytes}");
        // The stack is zeroed memory after the code, as Stella's is.
        let stack = format!(".zerofill __STELLA,__stack,_stack,{stack_bytes},4\n");
        let assembly = format!("{}{stack}", switch_assembly(&page, &offset));
        let object = assemble(&directory, "switch", &assembly);
        let binary = directory.join("linked");
        let mut ld = Command::new("/usr/bin/ld");
        ld.args(["-arch", "arm64", "-static", "-e", "_main", "-o"]).arg(&binary);
        let linked = Run::of(ld.arg(&object));
        assert_eq!(linked, Run::exits(0, ""), "`ld` should link a stack of {stack_bytes} bytes");
        let linked_text = text(&binary);
        let placement = Placement {
            code_address: linked_text.address,
            stack_top_address: symbol_address(&binary, "_stack") + u64::from(stack_bytes),
        };
        assert_eq!(linked_text.address, symbol_address(&binary, "_main"), "the code is `_main`");
        let expected = stack_top_oracle(placement).map(Instruction::encode);
        let found = &linked_text.words[PAGE_INDEX..PAGE_INDEX + 2];
        assert_eq!(found, expected, "the oracle, for a stack of {stack_bytes} bytes");
        assert_eq!(placed(&code, placement), linked_text.words, "for {stack_bytes} bytes");
    }
}

#[test]
fn the_largest_sources() {
    // The largest sources have the same code as the smallest, since comments mean nothing to
    // the code. And the most code there can be is the switch and a body for each of the most
    // functions, though a program has one.
    let placement = Placement { code_address: 0x1_0000_0290, stack_top_address: 0x1_0000_4000 };
    let smallest = placed(&generated(SMALLEST), placement);
    let longest = generated(&longest_comment());
    assert_eq!(placed(&longest, placement), smallest);
    assert_eq!((longest.size_bytes(), longest.stack_bytes()), (44, 0));
    // This is the most comments that fit before `main`: one more doesn't.
    let most = comments_then_main(5_450);
    assert!(most.len() <= usize_from_u32(SOURCE_BYTES_MAX), "5,450 comments fit");
    let one_more = comments_then_main(5_451);
    assert!(one_more.len() > usize_from_u32(SOURCE_BYTES_MAX), "5,451 comments don't");
    assert_eq!(placed(&generated(&most), placement), smallest);
    assert_eq!(CODE_INSTRUCTIONS_MAX, 10 + 4_096);
    assert_eq!(CODE_BYTES_MAX, 16_424);
}

#[test]
fn placements() {
    // The code at each end of a page, the stack's top at each end of a page, and distances
    // from none to the most `adrp` reaches, either way. At each, the two instructions that
    // find the stack are the oracle's, and every other instruction is as it was. One code is
    // placed every time, so placing it changes nothing in it.
    let code = generated(SMALLEST);
    let elsewhere = Placement { code_address: 0x2_0000_0000, stack_top_address: 0x2_0000_0000 };
    let unplaced = placed(&code, elsewhere);
    let mut placements_count: u32 = 0;
    // From the lower base, the furthest back `adrp` reaches is the first page there is.
    let bases = [0x1_0000_0000_u64, 0x2_0000_0000];
    let code_offsets = [0_u64, 4, 4_096 - 12, 4_096 - 8, 4_096, 0x290, 0x7FFC];
    for (base, code_offset) in bases.into_iter().flat_map(|base| code_offsets.map(|at| (base, at)))
    {
        let code_address = base + code_offset;
        let adrp_page = (code_address + 8) / 4_096 * 4_096;
        for distance in [0_u64, 4_096, 16_384, 32_768, 1_048_576, REACH_BYTES - 4_096] {
            for offset in [0_u64, 16, 4_096 - 16] {
                let mut tops = vec![adrp_page + distance + offset, adrp_page - distance + offset];
                // `adrp` reaches exactly 4 GiB back, but not quite 4 GiB on.
                if distance == REACH_BYTES - 4_096 {
                    tops.push(adrp_page - REACH_BYTES + offset);
                }
                for stack_top_address in tops {
                    let placement = Placement { code_address, stack_top_address };
                    let words = placed(&code, placement);
                    let expected = stack_top_oracle(placement).map(Instruction::encode);
                    assert_eq!(words[PAGE_INDEX..PAGE_INDEX + 2], expected, "{placement:?}");
                    assert_eq!(words[..PAGE_INDEX], unplaced[..PAGE_INDEX], "{placement:?}");
                    assert_eq!(words[PAGE_INDEX + 2..], unplaced[PAGE_INDEX + 2..]);
                    placements_count += 1;
                }
            }
        }
    }
    assert_eq!(placements_count, 2 * (7 * 6 * 3 * 2 + 7 * 3), "every placement is tried");
}

#[test]
#[should_panic(expected = "`adrp` reaches less than 4 GiB on")]
fn a_stack_4_gib_on_is_a_bug() {
    // The file format works a placement out, so a stack `adrp` can't reach is its bug.
    let code_address: u64 = 0x1_0000_0290;
    let stack_top_address = (code_address + 8) / 4_096 * 4_096 + REACH_BYTES;
    _ = generated(SMALLEST).placed(Placement { code_address, stack_top_address });
}

#[test]
#[should_panic(expected = "`adrp` reaches at most 4 GiB back")]
fn a_stack_past_4_gib_back_is_a_bug() {
    // One page further back than `adrp` reaches, so placing the code stops at the encoder's own
    // limit.
    let code_address: u64 = 0x2_0000_0290;
    let stack_top_address = (code_address + 8) / 4_096 * 4_096 - REACH_BYTES - 4_096;
    _ = generated(SMALLEST).placed(Placement { code_address, stack_top_address });
}

#[test]
#[should_panic(expected = "`adrp` reaches less than 4 GiB on")]
fn a_stack_far_out_of_reach_is_a_bug() {
    // Further on than any count of pages an `adrp` could be asked for, or 32 bits could hold.
    let placement = Placement { code_address: 0x290, stack_top_address: 1 << 60 };
    _ = generated(SMALLEST).placed(placement);
}

#[test]
#[should_panic(expected = "the stack's top keeps `sp` aligned")]
fn a_stack_top_that_isnt_aligned_is_a_bug() {
    // A stack's top 8 bytes past a multiple of 16, where `sp` can't start.
    let placement = Placement { code_address: 0x1_0000_0290, stack_top_address: 0x1_0000_4008 };
    _ = generated(SMALLEST).placed(placement);
}

#[test]
#[should_panic(expected = "code starts on an instruction")]
fn code_that_isnt_on_an_instruction_is_a_bug() {
    // Code 2 bytes past a multiple of 4, where no instruction starts.
    let placement = Placement { code_address: 0x1_0000_0292, stack_top_address: 0x1_0000_4000 };
    _ = generated(SMALLEST).placed(placement);
}
