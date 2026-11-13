// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Generates arm64 machine code for a program, the step after the meaning checks.
//!
//! Stella compiles to machine code itself, with no C and no assembler. `generate` takes the
//! `Program` that only `check_program` makes, so every check has passed and it has nothing to
//! refuse. It returns `Code`, which only it makes: the program's instructions, the switch to
//! Stella's own stack first, and how many bytes that stack needs.
//!
//! Two of the switch's instructions depend on where the file format puts the code and the stack,
//! which only it knows. So the format lays the file out from the code's size, and then asks
//! `Code::placed` for the bytes to write there. The format never encodes an instruction itself.
//!
//! An `Instruction` is one instruction Stella's code uses, with each of its parts named. It
//! encodes to the word the processor reads, and decodes back from it.

#![forbid(unsafe_code)]

pub use generate::{
    CODE_BYTES_MAX, CODE_INSTRUCTIONS_MAX, Code, INSTRUCTION_BYTES, Placement,
    STACK_ALIGNMENT_BYTES, generate,
};
pub use instruction::{
    ADD_IMMEDIATE_MAX, BRANCH_INSTRUCTIONS_MAX, BRANCH_INSTRUCTIONS_MIN, Instruction,
    PAGE_ADDRESS_PAGE_BYTES, PAGE_ADDRESS_PAGES_MAX, PAGE_ADDRESS_PAGES_MIN, Register,
    StackOrRegister,
};

mod generate;
mod instruction;
