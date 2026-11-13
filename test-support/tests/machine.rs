// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for asking the system's tools what machine code is. Each assembles a few
//! instructions whose words the Arm Architecture Reference Manual gives outright, and checks
//! they come back as those words, in order.

#![forbid(unsafe_code)]

use std::path::PathBuf;

use stella_test_support::{Scratch, Text, assemble, text};

/// `ret`, which is one word, whole, with no fields to fill in.
const RET: u32 = 0xD65F_03C0;

/// `nop`, which is one word, whole, with no fields to fill in.
const NOP: u32 = 0xD503_201F;

/// Returns a new, empty directory for one test to work in.
fn fresh_directory(name: &str) -> PathBuf {
    Scratch::of("machine").fresh(name)
}

#[test]
fn assembled_code_reads_back_word_for_word() {
    // One instruction, and then six, which `otool` prints across two lines. Each comes back as
    // its word, in the order it was written, from an address of 0, since nothing is linked.
    let directory = fresh_directory("words");
    let one = assemble(&directory, "one", "ret\n");
    assert_eq!(text(&one), Text { address: 0, words: vec![RET] });
    let six = assemble(&directory, "six", "nop\nret\nret\nnop\nnop\nret\n");
    assert_eq!(text(&six), Text { address: 0, words: vec![NOP, RET, RET, NOP, NOP, RET] });
}

#[test]
#[should_panic(expected = "`as` should assemble")]
fn assembly_the_assembler_refuses_fails_the_test() {
    // What isn't an instruction is refused by `as`, which fails the test that asked.
    let directory = fresh_directory("refused");
    let object = assemble(&directory, "bad", "not an instruction\n");
    unreachable!("`as` shouldn't have made {}", object.display());
}
