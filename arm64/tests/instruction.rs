// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for the instructions. They're a crate of their own, so the compiler lets them
//! reach only what the crate makes public: an `Instruction` in, its word out, and back.
//!
//! The system's assembler says what each word is. Every test that pins a word assembles the
//! instruction with `as` as it runs, so nothing here is a number copied down once. An oracle
//! builds each word from the instruction's fields, as the Arm Architecture Reference Manual
//! lays them out, by arithmetic and apart from the encoder, and every row checks the oracle
//! against `as` before the encoder is asked. Then every register in every place, and every
//! number a field holds, is checked against the oracle, where `as` would take too long.

#![forbid(unsafe_code)]

use std::iter;
use std::path::PathBuf;

use stella_arm64::{
    ADD_IMMEDIATE_MAX, BRANCH_INSTRUCTIONS_MAX, BRANCH_INSTRUCTIONS_MIN, Instruction,
    PAGE_ADDRESS_PAGE_BYTES, PAGE_ADDRESS_PAGES_MAX, PAGE_ADDRESS_PAGES_MIN, Register,
    StackOrRegister,
};
use stella_test_support::{Rng, Scratch, assemble, text};

/// This is how many bytes an instruction takes, which `bl` counts in.
const INSTRUCTION_BYTES: i64 = 4;

/// The register these tests use where which register it is doesn't matter.
const ANY: Register = Register::new(11);

/// Returns a new, empty directory for one test to work in.
fn fresh_directory(name: &str) -> PathBuf {
    Scratch::of("instruction").fresh(name)
}

/// Returns the register numbered `number`, which is chosen from 0 to 30.
fn register(number: usize) -> Register {
    Register::new(u8::try_from(number).expect("a register's number fits in a byte"))
}

/// Returns every general register, `x0` to `x30`.
fn registers() -> Vec<Register> {
    (0_usize..=30).map(register).collect()
}

/// Returns everything `add` reads or writes: every general register, and `sp`.
fn stack_or_registers() -> Vec<StackOrRegister> {
    let mut all: Vec<StackOrRegister> =
        registers().into_iter().map(StackOrRegister::Register).collect();
    all.push(StackOrRegister::StackPointer);
    all
}

/// Returns the name `as` knows `place` by, on 64 bits.
fn name_of(place: StackOrRegister) -> String {
    match place {
        StackOrRegister::StackPointer => String::from("sp"),
        StackOrRegister::Register(register) => format!("x{}", register.number()),
    }
}

/// Returns `instruction` as it's written for `as`. `adrp` and `bl` are written with how far
/// they reach in bytes, which is how `as` takes a distance that isn't a label.
fn assembly_of(instruction: Instruction) -> String {
    match instruction {
        Instruction::AddImmediate { target, source, immediate } => {
            format!("add {}, {}, #{immediate}", name_of(target), name_of(source))
        }
        Instruction::MoveRegister { target, source } => {
            format!("mov x{}, x{}", target.number(), source.number())
        }
        Instruction::PageAddress { target, pages } => {
            let bytes = i64::from(pages) * i64::from(PAGE_ADDRESS_PAGE_BYTES);
            format!("adrp x{}, #{bytes}", target.number())
        }
        Instruction::BranchWithLink { instructions } => {
            format!("bl #{}", i64::from(instructions) * INSTRUCTION_BYTES)
        }
        Instruction::MoveWideWithZero { target, immediate } => {
            format!("movz w{}, #{immediate}", target.number())
        }
        Instruction::Return => String::from("ret"),
    }
}

/// Returns the number an `add`'s register field holds for `place`: 31 for `sp`.
fn field_of(place: StackOrRegister) -> i64 {
    match place {
        StackOrRegister::StackPointer => 31,
        StackOrRegister::Register(register) => i64::from(register.number()),
    }
}

/// Returns the word `instruction` is, worked out from its fields by arithmetic, as the Arm
/// Architecture Reference Manual lays each form out, and apart from the encoder.
fn word_oracle(instruction: Instruction) -> u32 {
    let word: i64 = match instruction {
        // sf=1 op=0 S=0 100010 sh=0, then imm12 from bit 10, Rn from bit 5, Rd from bit 0.
        Instruction::AddImmediate { target, source, immediate } => {
            0x9100_0000 + i64::from(immediate) * 1_024 + field_of(source) * 32 + field_of(target)
        }
        // `orr`: sf=1 opc=01 01010 shift=00 N=0, Rm from bit 16, imm6=0, Rn=31, Rd from bit 0.
        Instruction::MoveRegister { target, source } => {
            0xAA00_03E0 + i64::from(source.number()) * 65_536 + i64::from(target.number())
        }
        // op=1, immlo from bit 29, 10000, immhi from bit 5, Rd from bit 0. The count is 21
        // bits in two's complement: its low 2 bits are immlo, and the other 19 immhi.
        Instruction::PageAddress { target, pages } => {
            let count = i64::from(pages).rem_euclid(1 << 21);
            let (low, high) = (count.rem_euclid(4), count.div_euclid(4));
            0x9000_0000 + low * 536_870_912 + high * 32 + i64::from(target.number())
        }
        // op=1 00101, then imm26 in two's complement.
        Instruction::BranchWithLink { instructions } => {
            0x9400_0000 + i64::from(instructions).rem_euclid(1 << 26)
        }
        // sf=0 opc=10 100101 hw=00, then imm16 from bit 5, Rd from bit 0.
        Instruction::MoveWideWithZero { target, immediate } => {
            0x5280_0000 + i64::from(immediate) * 32 + i64::from(target.number())
        }
        // 1101011 0 0 10 11111 0000 0 0, Rn=30, 00000.
        Instruction::Return => 0xD65F_03C0,
    };
    u32::try_from(word).expect("an instruction's word is 32 bits")
}

/// Returns the words `as` makes of `instructions`, in order, assembled as `<name>.o`.
fn assembled(name: &str, instructions: &[Instruction]) -> Vec<u32> {
    let lines: Vec<String> = instructions.iter().map(|&each| assembly_of(each)).collect();
    let object = assemble(&fresh_directory(name), name, &format!("{}\n", lines.join("\n")));
    let words = text(&object).words;
    assert_eq!(words.len(), instructions.len(), "`as` makes one word of each instruction");
    words
}

/// Returns an instruction chosen by `rng`: any form, with any registers and any numbers its
/// fields hold.
fn random_instruction(rng: &mut Rng) -> Instruction {
    let all = stack_or_registers();
    match rng.below(6) {
        0 => Instruction::AddImmediate {
            target: all[rng.below(all.len())],
            source: all[rng.below(all.len())],
            immediate: u16::try_from(rng.below(4_096)).expect("12 bits fit in 16"),
        },
        1 => Instruction::MoveRegister {
            target: register(rng.below(31)),
            source: register(rng.below(31)),
        },
        2 => Instruction::PageAddress {
            target: register(rng.below(31)),
            pages: signed_below(rng, 1 << 20),
        },
        3 => Instruction::BranchWithLink { instructions: signed_below(rng, 1 << 25) },
        4 => Instruction::MoveWideWithZero {
            target: register(rng.below(31)),
            immediate: u16::try_from(rng.below(65_536)).expect("16 bits fit in 16"),
        },
        _ => Instruction::Return,
    }
}

/// Returns a number chosen by `rng` from `-reach` up to, but not including, `reach`.
fn signed_below(rng: &mut Rng, reach: usize) -> i32 {
    let chosen = i32::try_from(rng.below(2 * reach)).expect("the reach fits in an `i32`");
    chosen - i32::try_from(reach).expect("the reach fits in an `i32`")
}

/// Returns the rows every instruction's edges are tried by: every register in each place, and
/// the least and the most of each number.
fn edge_rows() -> Vec<Instruction> {
    let mut rows = vec![Instruction::Return];
    let any = StackOrRegister::Register(ANY);
    for place in stack_or_registers() {
        rows.push(Instruction::AddImmediate { target: place, source: any, immediate: 0 });
        rows.push(Instruction::AddImmediate { target: any, source: place, immediate: 0 });
    }
    for immediate in [0, 1, 2, 2_047, 2_048, ADD_IMMEDIATE_MAX] {
        rows.push(Instruction::AddImmediate { target: any, source: any, immediate });
    }
    for each in registers() {
        rows.push(Instruction::MoveRegister { target: each, source: ANY });
        rows.push(Instruction::MoveRegister { target: ANY, source: each });
        rows.push(Instruction::PageAddress { target: each, pages: 1_i32 });
        rows.push(Instruction::MoveWideWithZero { target: each, immediate: 1 });
    }
    let pages_edges: [i32; 10] =
        [PAGE_ADDRESS_PAGES_MIN, -4, -3, -1, 0, 1, 2, 3, 4, PAGE_ADDRESS_PAGES_MAX];
    for pages in pages_edges {
        rows.push(Instruction::PageAddress { target: ANY, pages });
    }
    let call_edges: [i32; 6] = [BRANCH_INSTRUCTIONS_MIN, -1, 0, 1, 5, BRANCH_INSTRUCTIONS_MAX];
    for instructions in call_edges {
        rows.push(Instruction::BranchWithLink { instructions });
    }
    for immediate in [0, 1, 0x7FFF, 0x8000, u16::MAX] {
        rows.push(Instruction::MoveWideWithZero { target: ANY, immediate });
    }
    rows
}

#[test]
fn the_limits_are_what_each_field_holds() {
    // Each limit, stated by hand: 12 bits for `add`'s immediate, 4 KiB pages, 21 signed bits
    // of pages for `adrp`, 26 signed bits of instructions for `bl`, and registers to `x30`.
    assert_eq!(ADD_IMMEDIATE_MAX, 4_095);
    assert_eq!(PAGE_ADDRESS_PAGE_BYTES, 4_096);
    assert_eq!(PAGE_ADDRESS_PAGES_MIN, -1_048_576_i32);
    assert_eq!(PAGE_ADDRESS_PAGES_MAX, 1_048_575_i32);
    assert_eq!(BRANCH_INSTRUCTIONS_MIN, -33_554_432_i32);
    assert_eq!(BRANCH_INSTRUCTIONS_MAX, 33_554_431_i32);
    assert_eq!(Register::NUMBER_MAX, 30);
    for number in 0..=Register::NUMBER_MAX {
        assert_eq!(Register::new(number).number(), number, "a register keeps its number");
    }
}

#[test]
fn the_assembler_makes_the_same_word_of_every_edge() {
    // Every register in each place of each instruction, and the least and most of each number,
    // assembled by `as`. The oracle agrees with `as` first. Then the encoder makes the same
    // word, and the decoder reads `as`'s word back as the instruction.
    let rows = edge_rows();
    assert_eq!(rows.len(), 1 + 64 + 6 + 124 + 10 + 6 + 5, "every edge is a row");
    let words = assembled("edges", &rows);
    for (&row, &word) in rows.iter().zip(&words) {
        let written = assembly_of(row);
        assert_eq!(word_oracle(row), word, "the oracle, for `{written}`: {word:#010X}");
        assert_eq!(row.encode(), word, "for `{written}`");
        assert_eq!(Instruction::decode(word), Some(row), "for `{written}`: {word:#010X}");
    }
}

#[test]
fn the_assembler_makes_the_same_word_of_random_instructions() {
    // 4,000 seeded random instructions, of every form, assembled by `as`. Each form comes up
    // often, and each is checked as the edges are.
    let mut rng = Rng::new(0xA64_0001);
    let rows: Vec<Instruction> =
        iter::repeat_with(|| random_instruction(&mut rng)).take(4_000).collect();
    let words = assembled("random", &rows);
    let mut forms_count = [0_u32; 6];
    for (&row, &word) in rows.iter().zip(&words) {
        let written = assembly_of(row);
        assert_eq!(word_oracle(row), word, "the oracle, for `{written}`: {word:#010X}");
        assert_eq!(row.encode(), word, "for `{written}`");
        assert_eq!(Instruction::decode(word), Some(row), "for `{written}`: {word:#010X}");
        let form = match row {
            Instruction::AddImmediate { .. } => 0,
            Instruction::MoveRegister { .. } => 1,
            Instruction::PageAddress { .. } => 2,
            Instruction::BranchWithLink { .. } => 3,
            Instruction::MoveWideWithZero { .. } => 4,
            Instruction::Return => 5,
        };
        forms_count[form] += 1;
    }
    assert!(forms_count.iter().all(|&count| count >= 500), "each form is tried often");
}

#[test]
fn every_register_and_every_number_agrees_with_the_oracle() {
    // Every pair of places `add` and `mov` take, every immediate `add` and `movz` hold, and
    // every page count `adrp` holds: more than `as` is asked, each against the oracle.
    for target in stack_or_registers() {
        for source in stack_or_registers() {
            for immediate in [0, ADD_IMMEDIATE_MAX] {
                let add = Instruction::AddImmediate { target, source, immediate };
                assert_eq!(add.encode(), word_oracle(add), "{add:?}");
            }
        }
    }
    let any = StackOrRegister::Register(ANY);
    for immediate in 0..=ADD_IMMEDIATE_MAX {
        let add = Instruction::AddImmediate { target: any, source: any, immediate };
        assert_eq!(add.encode(), word_oracle(add), "{add:?}");
    }
    for target in registers() {
        for source in registers() {
            let mov = Instruction::MoveRegister { target, source };
            assert_eq!(mov.encode(), word_oracle(mov), "{mov:?}");
        }
        for immediate in [0, 1, 0x00FF, 0x0100, 0x7FFF, 0x8000, u16::MAX] {
            let movz = Instruction::MoveWideWithZero { target, immediate };
            assert_eq!(movz.encode(), word_oracle(movz), "{movz:?}");
        }
    }
    for immediate in 0..=u16::MAX {
        let movz = Instruction::MoveWideWithZero { target: ANY, immediate };
        assert_eq!(movz.encode(), word_oracle(movz), "{movz:?}");
    }
    for pages in PAGE_ADDRESS_PAGES_MIN..=PAGE_ADDRESS_PAGES_MAX {
        let adrp = Instruction::PageAddress { target: ANY, pages };
        assert_eq!(adrp.encode(), word_oracle(adrp), "{adrp:?}");
    }
}

#[test]
fn every_reach_of_a_call_agrees_with_the_oracle() {
    // `bl` near each end of its reach and around none, and 200,000 seeded random reaches, with
    // each direction tried often.
    let near: i32 = 1_000;
    let near_ends = (BRANCH_INSTRUCTIONS_MIN..BRANCH_INSTRUCTIONS_MIN + near)
        .chain(-near..=near)
        .chain(BRANCH_INSTRUCTIONS_MAX - near..=BRANCH_INSTRUCTIONS_MAX);
    for instructions in near_ends {
        let call = Instruction::BranchWithLink { instructions };
        assert_eq!(call.encode(), word_oracle(call), "{call:?}");
    }
    let mut rng = Rng::new(0xA64_0002);
    let (mut back_count, mut on_count) = (0_u32, 0_u32);
    for _ in 0_u32..200_000 {
        let instructions = signed_below(&mut rng, 1 << 25);
        let call = Instruction::BranchWithLink { instructions };
        assert_eq!(call.encode(), word_oracle(call), "{call:?}");
        if instructions < 0_i32 {
            back_count += 1;
        } else {
            on_count += 1;
        }
    }
    assert!(back_count > 50_000 && on_count > 50_000, "each direction is tried often");
}

#[test]
fn other_instructions_are_none_of_stellas() {
    // Instructions `as` assembles that Stella's code doesn't use: `nop` and `udf`, and then
    // ones a step from an instruction it does use, on the other width, with a shift, with the
    // zero register, another operation, or another register. The decoder reads none of them as
    // an instruction.
    let others = [
        "nop",
        "udf #0",
        "add w11, w11, #0",
        "add x11, x11, #1, lsl #12",
        "adds x11, x11, #0",
        "sub x11, x11, #0",
        "orr x11, xzr, xzr",
        "orr xzr, xzr, x11",
        "orr x11, x1, x11",
        "orr x11, xzr, x11, lsl #1",
        "orr w11, wzr, w11",
        "adr x11, #0",
        "adrp xzr, #0",
        "b #0",
        "movz x11, #0",
        "movz w11, #0, lsl #16",
        "movz wzr, #0",
        "movn w11, #0",
        "ret x1",
        "br x30",
    ];
    let directory = fresh_directory("others");
    let object = assemble(&directory, "others", &format!("{}\n", others.join("\n")));
    let words = text(&object).words;
    assert_eq!(words.len(), others.len(), "`as` makes one word of each instruction");
    for (written, &word) in others.iter().zip(&words) {
        assert_eq!(Instruction::decode(word), None, "for `{written}`: {word:#010X}");
    }
}

#[test]
fn a_word_is_at_most_one_instruction() {
    // 200,000 words, each an instruction's word with one bit changed. Each is no instruction,
    // or it's exactly one: the instruction it reads as makes that very word. Both come up
    // often, so no two words read as one instruction, and no word reads as what it isn't.
    let mut rng = Rng::new(0xA64_0003);
    let (mut none_count, mut some_count) = (0_u32, 0_u32);
    for _ in 0_u32..200_000 {
        let word = random_instruction(&mut rng).encode() ^ (1_u32 << rng.below(32));
        match Instruction::decode(word) {
            None => none_count += 1,
            Some(instruction) => {
                assert_eq!(instruction.encode(), word, "{instruction:?} from {word:#010X}");
                some_count += 1;
            }
        }
    }
    assert!(none_count > 20_000 && some_count > 20_000, "{none_count} none, {some_count} some");
}

#[test]
#[should_panic(expected = "a general register is numbered 0 to 30")]
fn a_register_past_the_last_is_a_bug() {
    // 31 is `sp` or the zero register, by instruction, so it's no general register.
    _ = Register::new(31);
}

#[test]
#[should_panic(expected = "`add`'s immediate fits in 12 bits")]
fn an_add_past_its_largest_immediate_is_a_bug() {
    // One more than the most `add` can add, which its 12 bits can't hold.
    let any = StackOrRegister::Register(ANY);
    let add =
        Instruction::AddImmediate { target: any, source: any, immediate: ADD_IMMEDIATE_MAX + 1 };
    _ = add.encode();
}

#[test]
#[should_panic(expected = "`adrp` reaches at most 4 GiB back")]
fn a_page_past_4_gib_back_is_a_bug() {
    // One page further back than `adrp`'s field can say.
    let adrp = Instruction::PageAddress { target: ANY, pages: PAGE_ADDRESS_PAGES_MIN - 1_i32 };
    _ = adrp.encode();
}

#[test]
#[should_panic(expected = "`adrp` reaches less than 4 GiB on")]
fn a_page_4_gib_on_is_a_bug() {
    // One page further on than `adrp`'s field can say.
    let adrp = Instruction::PageAddress { target: ANY, pages: PAGE_ADDRESS_PAGES_MAX + 1_i32 };
    _ = adrp.encode();
}

#[test]
#[should_panic(expected = "`bl` reaches at most 128 MiB back")]
fn a_call_past_128_mib_back_is_a_bug() {
    // One instruction further back than `bl`'s field can say.
    let call = Instruction::BranchWithLink { instructions: BRANCH_INSTRUCTIONS_MIN - 1_i32 };
    _ = call.encode();
}

#[test]
#[should_panic(expected = "`bl` reaches less than 128 MiB on")]
fn a_call_128_mib_on_is_a_bug() {
    // One instruction further on than `bl`'s field can say.
    let call = Instruction::BranchWithLink { instructions: BRANCH_INSTRUCTIONS_MAX + 1_i32 };
    _ = call.encode();
}
