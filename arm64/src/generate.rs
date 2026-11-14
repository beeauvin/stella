// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Generating turns a `Program` into arm64 machine code.
//!
//! The code starts with the switch, which is where the program starts. Stella's code runs on a
//! stack of its own, a region of the binary sized exactly, rather than on the one macOS gives the
//! main thread, where the loader, the arguments and the environment live. So the switch keeps the
//! system's stack pointer and return address in `x9` and `x10`, points `sp` at the top of
//! Stella's stack, and calls `main`. When `main` returns, it puts both back and returns the
//! program's exit status to the loader in `w0`: 0 when `main` returns `Void`, which the switch
//! sets, and `main`'s own value when it returns `U8`, which `main` left in `w0` and the switch
//! leaves alone. Stella's code never uses `x9` or `x10`, so nothing is written to memory to keep
//! them, and a program that writes nothing needs 0 bytes of running memory.
//!
//! The functions follow the switch, in the order they're in the source. A function that returns
//! `Void` is `ret`, so it returns when its body ends. One that returns `U8` puts its value in
//! `w0`, where a call's result goes, with `movz w0`, which clears the rest of `x0`, and then is
//! `ret`.
//!
//! Where the stack is depends on where the file format puts the code and the stack, which only
//! it knows. So the two instructions of the switch that find the stack's top aren't made here.
//! `Code::placed` makes them for a placement, and returns the bytes to write there. Nothing is
//! relocated when the program is loaded: a placement is two numbers the format worked out.

use stella_ast::FUNCTIONS_MAX;
use stella_meaning::{Program, Returned};
use stella_source::usize_from_u32;

use crate::instruction::{
    BRANCH_INSTRUCTIONS_MAX, Instruction, PAGE_ADDRESS_PAGE_BYTES, PAGE_ADDRESS_PAGES_MAX,
    PAGE_ADDRESS_PAGES_MIN, Register, StackOrRegister,
};

/// This is how many bytes an instruction takes. Every arm64 instruction takes 4.
pub const INSTRUCTION_BYTES: u32 = 4;

/// This is the most instructions a program's code can take: the longest switch, and the most
/// functions with the longest bodies.
pub const CODE_INSTRUCTIONS_MAX: u32 =
    SWITCH_INSTRUCTIONS_MAX + FUNCTIONS_MAX * BODY_INSTRUCTIONS_MAX;

/// This is the most bytes a program's code can take.
pub const CODE_BYTES_MAX: u32 = CODE_INSTRUCTIONS_MAX * INSTRUCTION_BYTES;

/// Stella's stack is a multiple of this many bytes, and so is its top's address, so `sp` stays
/// aligned as arm64 needs.
pub const STACK_ALIGNMENT_BYTES: u32 = 16;

/// This is the most instructions the switch takes: when `main` returns `Void`, it sets the exit
/// status itself, with one more instruction than when `main` returns `U8` and sets it.
const SWITCH_INSTRUCTIONS_MAX: u32 = 10;

/// This is the fewest instructions the switch takes.
const SWITCH_INSTRUCTIONS_MIN: u32 = SWITCH_INSTRUCTIONS_MAX - 1;

/// This is the most instructions a function's body takes: `movz` and `ret`, for one that
/// returns `U8`.
const BODY_INSTRUCTIONS_MAX: u32 = 2;

/// This is how many bytes of stack a function's frame takes today. A body puts its value in a
/// register, so it keeps nothing.
const FRAME_BYTES: u32 = 0;

/// This is the index, in the switch, of the `adrp` that finds the page the stack's top is in.
/// `generate` leaves it for `Code::placed` to make.
const STACK_TOP_PAGE_INDEX: u32 = 2;

/// This is the index, in the switch, of the `add` that finds the stack's top in its page.
/// `generate` leaves it for `Code::placed` to make.
const STACK_TOP_OFFSET_INDEX: u32 = 3;

/// This is the index, in the switch, of the `bl` that calls `main`.
const CALL_INDEX: u32 = 5;

/// This is the word `generate` leaves where `Code::placed` makes an instruction. No instruction
/// Stella's code uses is 0, which the processor refuses to run, so code that was never placed
/// would stop at once instead of running on.
const UNPLACED: u32 = 0;

/// This register holds the address a call returns to.
const LINK: Register = Register::new(30);

/// This register keeps the system's stack pointer while Stella's code runs. Stella's code never
/// uses it.
const SYSTEM_STACK: Register = Register::new(9);

/// This register keeps the system's return address while Stella's code runs. Stella's code
/// never uses it.
const SYSTEM_RETURN: Register = Register::new(10);

/// This register holds the stack's top while the switch finds it.
const STACK_TOP: Register = Register::new(11);

/// This register holds the status the program hands back to the loader.
const STATUS: Register = Register::new(0);

const _: () = assert!(
    CODE_INSTRUCTIONS_MAX <= BRANCH_INSTRUCTIONS_MAX.unsigned_abs(),
    "the switch's `bl` reaches `main` wherever it is in the most code there can be"
);
const _: () =
    assert!(FRAME_BYTES.is_multiple_of(STACK_ALIGNMENT_BYTES), "a frame keeps `sp` aligned");
const _: () = assert!(
    STACK_TOP_PAGE_INDEX + 1 == STACK_TOP_OFFSET_INDEX,
    "the `add` that finds the stack's top comes right after the `adrp` it finishes"
);
const _: () = assert!(
    STACK_TOP_OFFSET_INDEX < CALL_INDEX,
    "the stack's top is found before `main` is called on it"
);
const _: () = assert!(
    CALL_INDEX < SWITCH_INSTRUCTIONS_MIN - 1,
    "the switch goes on after `main` returns, to put the system's stack back"
);

/// A placement is where the file format put the code and the top of the stack, in memory. Each
/// address is named, so the two can't be given the wrong way round.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    /// The address of the code's first instruction.
    pub code_address: u64,
    /// The address just past the stack's last byte, where `sp` starts.
    pub stack_top_address: u64,
}

/// The machine code of a program, and the stack it needs.
///
/// Only `generate` makes it, so holding `Code` proves it was made from a `Program`.
#[derive(Debug)]
pub struct Code {
    /// The instructions' words, the switch first, with `UNPLACED` where `placed` makes one.
    words: Vec<u32>,
    /// How many bytes of stack the program needs.
    stack_bytes: u32,
}

impl Code {
    /// This is where a program starts, in bytes from its code's first: at the switch, which
    /// comes first.
    pub const ENTRY_OFFSET_BYTES: u32 = 0;

    /// Returns how many bytes the code takes, wherever it's placed.
    ///
    /// # Panics
    ///
    /// Never, for code `generate` made. The assertions check the generating's own work.
    #[must_use]
    pub fn size_bytes(&self) -> u32 {
        let size_bytes = instructions_count(&self.words) * INSTRUCTION_BYTES;
        assert!(size_bytes >= SWITCH_INSTRUCTIONS_MIN * INSTRUCTION_BYTES, "the switch is there");
        assert!(size_bytes <= CODE_BYTES_MAX, "no program's code is more than `CODE_BYTES_MAX`");
        size_bytes
    }

    /// Returns how many bytes of stack the program needs: what it writes as it runs.
    ///
    /// # Panics
    ///
    /// Never, for code `generate` made. The assertion checks the generating's own work.
    #[must_use]
    pub const fn stack_bytes(&self) -> u32 {
        assert!(
            self.stack_bytes.is_multiple_of(STACK_ALIGNMENT_BYTES),
            "a stack keeps `sp` aligned"
        );
        self.stack_bytes
    }

    /// Returns the code's bytes as they're written for `placement`: every instruction in order,
    /// each as the processor reads it, with the two that find the stack's top made for where
    /// the code and the stack are.
    ///
    /// # Panics
    ///
    /// Panics if the code doesn't start on an instruction, if the stack's top isn't aligned, or
    /// if the top is further from the code than `adrp` reaches. The file format works a
    /// placement out, so any of those is a bug in the compiler.
    #[must_use]
    pub fn placed(&self, placement: Placement) -> Vec<u8> {
        let (page_index, offset_index) =
            (usize_from_u32(STACK_TOP_PAGE_INDEX), usize_from_u32(STACK_TOP_OFFSET_INDEX));
        assert!(offset_index < self.words.len(), "the switch's instructions are in the code");
        assert_eq!(self.words[page_index], UNPLACED, "the stack's `adrp` is left to be placed");
        assert_eq!(self.words[offset_index], UNPLACED, "the stack's `add` is left to be placed");
        let [page_word, offset_word] = placed_stack_top(placement);
        let size_bytes = usize_from_u32(self.size_bytes());
        let mut bytes = Vec::with_capacity(size_bytes);
        for (index, &word) in self.words.iter().enumerate() {
            let word = match index {
                index if index == page_index => page_word,
                index if index == offset_index => offset_word,
                _ => word,
            };
            assert_ne!(word, UNPLACED, "every instruction is made by the time it's written");
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        assert_eq!(bytes.len(), size_bytes, "every instruction is four bytes");
        bytes
    }
}

/// Generates the machine code for `program`, and returns it. Nothing can be refused: every
/// program the checks passed has code.
///
/// # Panics
///
/// Never, whatever the program. The assertions check what the checks proved and the generating's
/// own work, so one that fails is a bug in the compiler.
///
/// If there isn't the memory for its code, Rust ends the process, which isn't a panic.
#[must_use]
pub fn generate(program: &Program<'_>) -> Code {
    let returned = program.fitted().as_slice();
    assert!(
        returned.len() <= usize_from_u32(FUNCTIONS_MAX),
        "no source holds more than `FUNCTIONS_MAX` functions"
    );
    let main_function_index = usize_from_u32(program.main_function_index());
    assert!(main_function_index < returned.len(), "`main` is one of the functions");
    let main_returned = returned[main_function_index];
    // The switch, then a body for each function: room for every instruction, made once.
    let instructions_count_max = usize_from_u32(SWITCH_INSTRUCTIONS_MAX)
        + returned.len() * usize_from_u32(BODY_INSTRUCTIONS_MAX);
    let mut words = Vec::with_capacity(instructions_count_max);
    let capacity_before = words.capacity();
    // `main`'s body follows the switch and the bodies before it.
    let bodies_before: u32 = returned[..main_function_index].iter().copied().map(body_count).sum();
    let main_instruction_index = switch_count(main_returned) + bodies_before;
    words.extend(generate_switch(main_instruction_index, main_returned));
    for &each in returned {
        words.extend(generate_body(each));
    }
    assert!(words.len() <= instructions_count_max, "the switch, and a body for each function");
    assert_eq!(words.capacity(), capacity_before, "the room made for the code never grows");
    assert_generated(&words, returned, main_function_index);
    // `main` calls nothing, so the deepest frame along its calls is its own.
    Code { words, stack_bytes: FRAME_BYTES }
}

/// Returns how many instructions the switch takes when `main` gives back `main_returned`: one
/// fewer when `main` sets the exit status itself.
const fn switch_count(main_returned: Returned) -> u32 {
    match main_returned {
        Returned::Void => SWITCH_INSTRUCTIONS_MAX,
        Returned::U8(_) => SWITCH_INSTRUCTIONS_MIN,
    }
}

/// Returns how many instructions the body of a function that gives back `returned` takes.
const fn body_count(returned: Returned) -> u32 {
    match returned {
        Returned::Void => 1,
        Returned::U8(_) => 2,
    }
}

/// Returns the body of a function that gives back `returned`, as words: for `Void`, `ret`, and
/// for `U8`, `movz w0` of the value, which clears the rest of `x0`, then `ret`. A call's result
/// goes in `w0`.
fn generate_body(returned: Returned) -> impl Iterator<Item = u32> {
    let value = match returned {
        Returned::Void => None,
        Returned::U8(value) => {
            Some(Instruction::MoveWideWithZero { target: STATUS, immediate: u16::from(value) })
        }
    };
    let body = value.into_iter().chain([Instruction::Return]).map(Instruction::encode);
    let count = u32::try_from(body.clone().count()).expect("a body is a few instructions");
    assert_eq!(count, body_count(returned), "a body is as long as its count says");
    assert!(count <= BODY_INSTRUCTIONS_MAX, "a body takes no more than the most a body takes");
    body
}

/// Returns the switch, which calls `main` at `main_instruction_index` in the code, with
/// `UNPLACED` where `Code::placed` makes the two instructions that find the stack's top. When
/// `main` gives back `Void`, the switch sets the exit status to 0 itself, and otherwise leaves
/// `main`'s value where it is.
fn generate_switch(main_instruction_index: u32, main_returned: Returned) -> Vec<u32> {
    let count = switch_count(main_returned);
    assert!(main_instruction_index >= count, "`main` follows the switch");
    assert!(main_instruction_index < CODE_INSTRUCTIONS_MAX, "`main` is in the code");
    let call_distance = main_instruction_index - CALL_INDEX;
    let call_distance = i32::try_from(call_distance).expect("the code is far less than 2 GiB");
    let stack_pointer = StackOrRegister::StackPointer;
    let register = StackOrRegister::Register;
    let status = match main_returned {
        Returned::Void => Some(Instruction::MoveWideWithZero { target: STATUS, immediate: 0 }),
        Returned::U8(_) => None,
    };
    let switch: Vec<u32> = [
        // Keep the system's stack pointer and return address.
        Instruction::AddImmediate {
            target: register(SYSTEM_STACK),
            source: stack_pointer,
            immediate: 0,
        }
        .encode(),
        Instruction::MoveRegister { target: SYSTEM_RETURN, source: LINK }.encode(),
        // Find the top of Stella's stack, and point `sp` at it.
        UNPLACED,
        UNPLACED,
        Instruction::AddImmediate {
            target: stack_pointer,
            source: register(STACK_TOP),
            immediate: 0,
        }
        .encode(),
        Instruction::BranchWithLink { instructions: call_distance }.encode(),
        // Put the system's stack pointer and return address back.
        Instruction::AddImmediate {
            target: stack_pointer,
            source: register(SYSTEM_STACK),
            immediate: 0,
        }
        .encode(),
        Instruction::MoveRegister { target: LINK, source: SYSTEM_RETURN }.encode(),
    ]
    .into_iter()
    // Set the exit status when `main` didn't, and return it to the loader.
    .chain(status.map(Instruction::encode))
    .chain([Instruction::Return.encode()])
    .collect();
    assert_eq!(instructions_count(&switch), count, "the switch is as long as its count says");
    switch
}

/// Returns the two instructions that find the stack's top for `placement`, as words: `adrp x11`
/// to the top's page, then `add x11, x11` of its offset in the page.
fn placed_stack_top(placement: Placement) -> [u32; 2] {
    let Placement { code_address, stack_top_address } = placement;
    let on_instruction = code_address.is_multiple_of(u64::from(INSTRUCTION_BYTES));
    assert!(on_instruction, "code starts on an instruction");
    let is_aligned = stack_top_address.is_multiple_of(u64::from(STACK_ALIGNMENT_BYTES));
    assert!(is_aligned, "the stack's top keeps `sp` aligned");
    let page_bytes = u64::from(PAGE_ADDRESS_PAGE_BYTES);
    // `adrp` counts pages from the page its own address is in.
    let instruction_page = page_instruction_address(code_address) / page_bytes;
    let stack_top_page = stack_top_address / page_bytes;
    let instruction_page =
        i64::try_from(instruction_page).expect("a page's number fits in 63 bits");
    let stack_top_page = i64::try_from(stack_top_page).expect("a page's number fits in 63 bits");
    let pages = stack_top_page - instruction_page;
    assert!(pages >= i64::from(PAGE_ADDRESS_PAGES_MIN), "`adrp` reaches at most 4 GiB back");
    assert!(pages <= i64::from(PAGE_ADDRESS_PAGES_MAX), "`adrp` reaches less than 4 GiB on");
    let pages = i32::try_from(pages).expect("a count of pages `adrp` reaches fits in 32 bits");
    let offset = u16::try_from(stack_top_address % page_bytes);
    let offset = offset.expect("an offset in a 4 KiB page fits in 12 bits");
    let stack_top = StackOrRegister::Register(STACK_TOP);
    let words = [
        Instruction::PageAddress { target: STACK_TOP, pages }.encode(),
        Instruction::AddImmediate { target: stack_top, source: stack_top, immediate: offset }
            .encode(),
    ];
    // What the processor will work out from the two words is where the stack's top is.
    assert_eq!(read_stack_top(code_address, words), stack_top_address, "the words find the top");
    words
}

/// Returns the address `words`, the two instructions that find the stack's top, put in `x11`
/// when the code is at `code_address`: what the processor works out from them, read back from
/// their fields.
fn read_stack_top(code_address: u64, words: [u32; 2]) -> u64 {
    let [page_word, offset_word] = words.map(Instruction::decode);
    let Some(Instruction::PageAddress { target: STACK_TOP, pages }) = page_word else {
        unreachable!("the first word is `adrp x11`, not {page_word:?}");
    };
    let stack_top = StackOrRegister::Register(STACK_TOP);
    let Some(Instruction::AddImmediate { target, source, immediate }) = offset_word else {
        unreachable!("the second word is an `add`, not {offset_word:?}");
    };
    assert_eq!(target, stack_top, "the `add` leaves the stack's top in `x11`");
    assert_eq!(source, stack_top, "the `add` starts from the page `adrp` left in `x11`");
    let page_bytes = u64::from(PAGE_ADDRESS_PAGE_BYTES);
    let instruction_page = page_instruction_address(code_address) / page_bytes;
    let stack_top_page = instruction_page.checked_add_signed(i64::from(pages));
    let stack_top_page = stack_top_page.expect("the stack's top is at an address above 0");
    stack_top_page * page_bytes + u64::from(immediate)
}

/// Returns the address of the `adrp` that finds the stack's top, in code at `code_address`.
fn page_instruction_address(code_address: u64) -> u64 {
    code_address + u64::from(STACK_TOP_PAGE_INDEX * INSTRUCTION_BYTES)
}

/// Asserts that `words`, generated for functions that give back `returned`, with `main` the one
/// at `main_function_index`, hold together as `Code::placed` and the file format will trust: the
/// switch, then each function's body, read back from its words as giving back what `returned`
/// says, the stack's two words left to be placed, the call landing on `main`'s body, and the exit
/// status set by the switch exactly when `main` gives back `Void`.
fn assert_generated(words: &[u32], returned: &[Returned], main_function_index: usize) {
    let main_returned = returned[main_function_index];
    let switch_count = usize_from_u32(switch_count(main_returned));
    assert!(words.len() > switch_count, "the switch, and at least `main`'s body");
    let (switch, bodies) = words.split_at(switch_count);
    for (index, &word) in switch.iter().enumerate() {
        let is_left = index == usize_from_u32(STACK_TOP_PAGE_INDEX)
            || index == usize_from_u32(STACK_TOP_OFFSET_INDEX);
        assert_eq!(word == UNPLACED, is_left, "only the stack's two words are left to be placed");
    }
    // The call, read back from its word, lands on `main`'s body.
    let call = Instruction::decode(switch[usize_from_u32(CALL_INDEX)]);
    let Some(Instruction::BranchWithLink { instructions }) = call else {
        unreachable!("the switch calls `main` with `bl`, not {call:?}");
    };
    let called_index = CALL_INDEX.checked_add_signed(instructions);
    let called_index = usize_from_u32(called_index.expect("the switch calls forward"));
    assert!(called_index < words.len(), "`main` is one of the bodies");
    let returns = Instruction::Return.encode();
    assert_eq!(switch[switch_count - 1], returns, "the switch returns to the loader");
    let status = Instruction::decode(switch[switch_count - 2]);
    let sets_status =
        status == Some(Instruction::MoveWideWithZero { target: STATUS, immediate: 0 });
    assert_eq!(sets_status, main_returned == Returned::Void, "the switch sets 0 for `Void`");
    let mut rest = bodies;
    for (index, &each) in returned.iter().enumerate() {
        if index == main_function_index {
            assert_eq!(words.len() - rest.len(), called_index, "the switch calls `main`'s body");
        }
        let (body, after) = rest.split_at(usize_from_u32(body_count(each)));
        assert_eq!(read_body(body), each, "a body gives back what its function returns");
        rest = after;
    }
    assert!(rest.is_empty(), "the code is the switch and the bodies, and nothing more");
}

/// Returns what `body`, a function's body as words, gives back, read back from its words: a
/// lone `ret` gives back `Void`, and `movz w0` of a value then `ret` gives back that `U8`.
fn read_body(body: &[u32]) -> Returned {
    let decoded: Vec<Option<Instruction>> = body.iter().copied().map(Instruction::decode).collect();
    match decoded.as_slice() {
        [Some(Instruction::Return)] => Returned::Void,
        [
            Some(Instruction::MoveWideWithZero { target: STATUS, immediate }),
            Some(Instruction::Return),
        ] => Returned::U8(u8::try_from(*immediate).expect("a `U8` body sets a value a `U8` holds")),
        other => unreachable!("a body is `ret`, or `movz w0` then `ret`, not {other:?}"),
    }
}

/// Returns how many instructions `words` are, each word one instruction.
fn instructions_count(words: &[u32]) -> u32 {
    assert!(
        words.len() <= usize_from_u32(CODE_INSTRUCTIONS_MAX),
        "no program's code is more than `CODE_INSTRUCTIONS_MAX` instructions"
    );
    u32::try_from(words.len()).expect("a count of at most `CODE_INSTRUCTIONS_MAX` fits in a `u32`")
}
