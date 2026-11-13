// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! An instruction is one arm64 instruction Stella's code uses, and the 32-bit word the processor
//! reads it as.
//!
//! These are the only instructions the code uses so far. Each names its parts, so two registers
//! can't be given the wrong way round, and takes exactly what its form holds. `encode` makes an
//! instruction's word, and `decode` reads a word back as the instruction it is. `encode` checks
//! itself against `decode`: every word that's made is read back before it's returned.
//!
//! A register's field is 5 bits, and what 31 means in one depends on the instruction. In `add`
//! it's the stack pointer, `sp`, and elsewhere it's the zero register, which reads as 0 and
//! throws away what's written to it. So a `Register` is only ever `x0` to `x30`, and the one
//! instruction that reaches `sp` takes a `StackOrRegister`.

/// This is the largest immediate `add` takes, unshifted: 12 bits.
pub const ADD_IMMEDIATE_MAX: u16 = (1 << 12) - 1;

/// This is the size of the pages `adrp` counts in: 4 KiB, whatever size the system's pages are.
pub const PAGE_ADDRESS_PAGE_BYTES: u32 = 1 << 12;

/// This is the furthest back `adrp` reaches, in pages: its count is 21 bits, signed, so 4 GiB.
pub const PAGE_ADDRESS_PAGES_MIN: i32 = -(1 << 20);

/// This is the furthest on `adrp` reaches, in pages: one page short of 4 GiB.
pub const PAGE_ADDRESS_PAGES_MAX: i32 = (1 << 20) - 1;

/// This is the furthest back `bl` reaches, in instructions: its count is 26 bits, signed, so
/// 128 MiB.
pub const BRANCH_INSTRUCTIONS_MIN: i32 = -(1 << 25);

/// This is the furthest on `bl` reaches, in instructions: one instruction short of 128 MiB.
pub const BRANCH_INSTRUCTIONS_MAX: i32 = (1 << 25) - 1;

/// This is a register field's 5 bits.
const REGISTER_FIELD_MASK: u32 = 0b1_1111;

/// This is what a register field holds for `sp` or the zero register, by instruction.
const REGISTER_FIELD_31: u32 = 31;

/// These are `add`'s fixed bits, with an immediate: 64 bits, no flags set, the immediate
/// unshifted. The mask says which bits are fixed.
const ADD_IMMEDIATE_BITS: u32 = 0x9100_0000;
const ADD_IMMEDIATE_MASK: u32 = 0xFFC0_0000;

/// These are `mov`'s fixed bits, between two registers: a 64-bit `orr` of the zero register and
/// a register that isn't shifted. The mask says which bits are fixed.
const MOVE_REGISTER_BITS: u32 = 0xAA00_03E0;
const MOVE_REGISTER_MASK: u32 = 0xFFE0_FFE0;

/// These are `adrp`'s fixed bits. The mask says which bits are fixed.
const PAGE_ADDRESS_BITS: u32 = 0x9000_0000;
const PAGE_ADDRESS_MASK: u32 = 0x9F00_0000;

/// These are `bl`'s fixed bits. The mask says which bits are fixed.
const BRANCH_WITH_LINK_BITS: u32 = 0x9400_0000;
const BRANCH_WITH_LINK_MASK: u32 = 0xFC00_0000;

/// These are `movz`'s fixed bits, on 32 bits, with no shift. The mask says which bits are fixed.
const MOVE_WIDE_WITH_ZERO_BITS: u32 = 0x5280_0000;
const MOVE_WIDE_WITH_ZERO_MASK: u32 = 0xFFE0_0000;

/// This is `ret`, which returns to the address in `x30`, whole.
const RETURN_BITS: u32 = 0xD65F_03C0;

/// These are how many bits `adrp`'s page count and `bl`'s instruction count take.
const PAGE_ADDRESS_PAGES_BITS: u32 = 21;
const BRANCH_INSTRUCTIONS_BITS: u32 = 26;

/// This is where the field of the register an instruction reads starts, in bits from the
/// word's lowest. The field of the register it writes starts at 0.
const SOURCE_FIELD_SHIFT: u32 = 5;

/// This is where the field of the register `mov` reads starts. Its `orr` reads the zero
/// register where other instructions read their source.
const MOVE_SOURCE_FIELD_SHIFT: u32 = 16;

/// This is where `add`'s immediate starts.
const ADD_IMMEDIATE_SHIFT: u32 = 10;

/// This is where `movz`'s immediate starts.
const MOVE_WIDE_IMMEDIATE_SHIFT: u32 = 5;

/// `adrp`'s page count is split in two: its low 2 bits sit high in the word, and the other 19
/// sit low. These are where each part starts, and the bits each takes once it's shifted down.
const PAGE_ADDRESS_LOW_SHIFT: u32 = 29;
const PAGE_ADDRESS_LOW_BITS: u32 = 2;
const PAGE_ADDRESS_LOW_MASK: u32 = 0b11;
const PAGE_ADDRESS_HIGH_SHIFT: u32 = 5;
const PAGE_ADDRESS_HIGH_MASK: u32 = 0x7_FFFF;

const _: () = assert!(
    ADD_IMMEDIATE_BITS & !ADD_IMMEDIATE_MASK == 0,
    "`add`'s fixed bits are among the bits its mask fixes"
);
const _: () = assert!(
    MOVE_REGISTER_BITS & !MOVE_REGISTER_MASK == 0,
    "`mov`'s fixed bits are among the bits its mask fixes"
);
const _: () = assert!(
    PAGE_ADDRESS_BITS & !PAGE_ADDRESS_MASK == 0,
    "`adrp`'s fixed bits are among the bits its mask fixes"
);
const _: () = assert!(
    BRANCH_WITH_LINK_BITS & !BRANCH_WITH_LINK_MASK == 0,
    "`bl`'s fixed bits are among the bits its mask fixes"
);
const _: () = assert!(
    MOVE_WIDE_WITH_ZERO_BITS & !MOVE_WIDE_WITH_ZERO_MASK == 0,
    "`movz`'s fixed bits are among the bits its mask fixes"
);
const _: () = assert!(
    (RETURN_BITS >> SOURCE_FIELD_SHIFT) & REGISTER_FIELD_MASK == 30,
    "`ret` returns to `x30`, where `bl` leaves the address to return to"
);
const _: () = assert!(
    PAGE_ADDRESS_PAGES_MAX == (1 << (PAGE_ADDRESS_PAGES_BITS - 1)) - 1,
    "`adrp`'s reach on is what its count's bits hold"
);
const _: () = assert!(
    PAGE_ADDRESS_PAGES_MIN == -(1 << (PAGE_ADDRESS_PAGES_BITS - 1)),
    "`adrp`'s reach back is what its count's bits hold"
);
const _: () = assert!(
    BRANCH_INSTRUCTIONS_MAX == (1 << (BRANCH_INSTRUCTIONS_BITS - 1)) - 1,
    "`bl`'s reach on is what its count's bits hold"
);
const _: () = assert!(
    BRANCH_INSTRUCTIONS_MIN == -(1 << (BRANCH_INSTRUCTIONS_BITS - 1)),
    "`bl`'s reach back is what its count's bits hold"
);
const _: () = assert!(
    PAGE_ADDRESS_LOW_MASK.count_ones() + PAGE_ADDRESS_HIGH_MASK.count_ones()
        == PAGE_ADDRESS_PAGES_BITS,
    "the two parts of `adrp`'s page count are all of its bits"
);
const _: () = assert!(
    PAGE_ADDRESS_LOW_MASK.count_ones() == PAGE_ADDRESS_LOW_BITS,
    "the low part of `adrp`'s page count is as many bits as it's shifted by"
);

/// A register is one of the 31 general registers, by its number: `x0` to `x30`, or `w0` to
/// `w30` where an instruction works on 32 bits.
///
/// `Register::new` and the decoder are all that make one, and each refuses 31, which no
/// instruction reads as a general register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Register(u8);

impl Register {
    /// This is the highest register's number: `x30`, which holds the address a call returns to.
    pub const NUMBER_MAX: u8 = 30;

    /// Returns the register numbered `number`.
    ///
    /// # Panics
    ///
    /// Panics if `number` is more than `Register::NUMBER_MAX`.
    #[must_use]
    pub const fn new(number: u8) -> Self {
        assert!(number <= Self::NUMBER_MAX, "a general register is numbered 0 to 30");
        Self(number)
    }

    /// Returns the register's number, 0 to 30.
    #[must_use]
    pub const fn number(self) -> u8 {
        self.0
    }

    /// Returns the register's number as an instruction's field holds it.
    fn field(self) -> u32 {
        assert!(self.0 <= Self::NUMBER_MAX, "`Register::new` checked the number");
        u32::from(self.0)
    }

    /// Returns the register `field` holds, an instruction's 5 bits, or `None` when it holds 31,
    /// which is no general register.
    fn from_field(field: u32) -> Option<Self> {
        assert!(field <= REGISTER_FIELD_MASK, "a register's field is 5 bits");
        let number = u8::try_from(field).expect("5 bits fit in a byte");
        (number <= Self::NUMBER_MAX).then_some(Self(number))
    }
}

/// The stack pointer or a register: what `add` reads and writes, where a field's 31 means `sp`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StackOrRegister {
    /// The stack pointer, `sp`.
    StackPointer,
    /// A general register.
    Register(Register),
}

impl StackOrRegister {
    /// Returns what an `add`'s register field holds for this.
    fn field(self) -> u32 {
        match self {
            Self::StackPointer => REGISTER_FIELD_31,
            Self::Register(register) => register.field(),
        }
    }

    /// Returns what `field` holds, an `add`'s 5 bits for a register.
    fn from_field(field: u32) -> Self {
        Register::from_field(field).map_or(Self::StackPointer, Self::Register)
    }
}

/// An instruction is one arm64 instruction Stella's code uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Instruction {
    /// `add target, source, #immediate`, on 64 bits: puts `source` plus `immediate` in `target`.
    /// It's also how a value moves to or from `sp`: `add x9, sp, #0` is `mov x9, sp`.
    AddImmediate {
        /// Where the sum goes.
        target: StackOrRegister,
        /// What the immediate is added to.
        source: StackOrRegister,
        /// The number added, at most `ADD_IMMEDIATE_MAX`.
        immediate: u16,
    },
    /// `mov target, source`, on 64 bits, between two registers. It's `orr target, xzr, source`.
    MoveRegister {
        /// Where the value goes.
        target: Register,
        /// Where the value comes from.
        source: Register,
    },
    /// `adrp target, #pages`: puts in `target` the address of the 4 KiB page that's `pages`
    /// pages on from this instruction's own.
    PageAddress {
        /// Where the page's address goes.
        target: Register,
        /// How many pages on the page is, from `PAGE_ADDRESS_PAGES_MIN` to
        /// `PAGE_ADDRESS_PAGES_MAX`.
        pages: i32,
    },
    /// `bl #instructions`: calls the instruction that's `instructions` instructions on from this
    /// one, and leaves the address of the one after this in `x30`.
    BranchWithLink {
        /// How many instructions on the one called is, from `BRANCH_INSTRUCTIONS_MIN` to
        /// `BRANCH_INSTRUCTIONS_MAX`.
        instructions: i32,
    },
    /// `movz target, #immediate`, on 32 bits: sets `target` to `immediate`, and clears the rest
    /// of the register.
    MoveWideWithZero {
        /// Where the immediate goes.
        target: Register,
        /// The number the register is set to.
        immediate: u16,
    },
    /// `ret`: returns to the address in `x30`.
    Return,
}

impl Instruction {
    /// Returns the word the processor reads this instruction as.
    ///
    /// # Panics
    ///
    /// Panics if a number is more than its form holds: an `add`'s immediate past
    /// `ADD_IMMEDIATE_MAX`, an `adrp`'s pages or a `bl`'s instructions past their reach.
    #[must_use]
    pub fn encode(self) -> u32 {
        let word = match self {
            Self::AddImmediate { target, source, immediate } => {
                assert!(immediate <= ADD_IMMEDIATE_MAX, "`add`'s immediate fits in 12 bits");
                let immediate = u32::from(immediate) << ADD_IMMEDIATE_SHIFT;
                let source = source.field() << SOURCE_FIELD_SHIFT;
                ADD_IMMEDIATE_BITS | immediate | source | target.field()
            }
            Self::MoveRegister { target, source } => {
                MOVE_REGISTER_BITS | (source.field() << MOVE_SOURCE_FIELD_SHIFT) | target.field()
            }
            Self::PageAddress { target, pages } => {
                assert!(pages >= PAGE_ADDRESS_PAGES_MIN, "`adrp` reaches at most 4 GiB back");
                assert!(pages <= PAGE_ADDRESS_PAGES_MAX, "`adrp` reaches less than 4 GiB on");
                let count = field_from_signed(pages, PAGE_ADDRESS_PAGES_BITS);
                let low = (count & PAGE_ADDRESS_LOW_MASK) << PAGE_ADDRESS_LOW_SHIFT;
                let high = (count >> PAGE_ADDRESS_LOW_BITS) << PAGE_ADDRESS_HIGH_SHIFT;
                PAGE_ADDRESS_BITS | low | high | target.field()
            }
            Self::BranchWithLink { instructions } => {
                let near = instructions >= BRANCH_INSTRUCTIONS_MIN;
                assert!(near, "`bl` reaches at most 128 MiB back");
                let near = instructions <= BRANCH_INSTRUCTIONS_MAX;
                assert!(near, "`bl` reaches less than 128 MiB on");
                BRANCH_WITH_LINK_BITS | field_from_signed(instructions, BRANCH_INSTRUCTIONS_BITS)
            }
            Self::MoveWideWithZero { target, immediate } => {
                let immediate = u32::from(immediate) << MOVE_WIDE_IMMEDIATE_SHIFT;
                MOVE_WIDE_WITH_ZERO_BITS | immediate | target.field()
            }
            Self::Return => RETURN_BITS,
        };
        assert_eq!(Self::decode(word), Some(self), "an instruction's word reads back as it");
        word
    }

    /// Returns the instruction `word` is, or `None` when it's none of the instructions Stella's
    /// code uses.
    ///
    /// # Panics
    ///
    /// Never, whatever the word. Each field is masked to the bits it holds before it's read.
    #[must_use]
    pub fn decode(word: u32) -> Option<Self> {
        if word == RETURN_BITS {
            return Some(Self::Return);
        }
        let target_field = word & REGISTER_FIELD_MASK;
        if word & ADD_IMMEDIATE_MASK == ADD_IMMEDIATE_BITS {
            let immediate = (word >> ADD_IMMEDIATE_SHIFT) & u32::from(ADD_IMMEDIATE_MAX);
            let source_field = (word >> SOURCE_FIELD_SHIFT) & REGISTER_FIELD_MASK;
            return Some(Self::AddImmediate {
                target: StackOrRegister::from_field(target_field),
                source: StackOrRegister::from_field(source_field),
                immediate: u16::try_from(immediate).expect("12 bits fit in 16"),
            });
        }
        if word & BRANCH_WITH_LINK_MASK == BRANCH_WITH_LINK_BITS {
            let count = word & !BRANCH_WITH_LINK_MASK;
            let instructions = signed_from_field(count, BRANCH_INSTRUCTIONS_BITS);
            return Some(Self::BranchWithLink { instructions });
        }
        // Every other instruction's target is a general register, so a field of 31 is none of
        // them.
        let target = Register::from_field(target_field)?;
        if word & MOVE_REGISTER_MASK == MOVE_REGISTER_BITS {
            let source_field = (word >> MOVE_SOURCE_FIELD_SHIFT) & REGISTER_FIELD_MASK;
            let source = Register::from_field(source_field)?;
            return Some(Self::MoveRegister { target, source });
        }
        if word & PAGE_ADDRESS_MASK == PAGE_ADDRESS_BITS {
            let high = (word >> PAGE_ADDRESS_HIGH_SHIFT) & PAGE_ADDRESS_HIGH_MASK;
            let low = (word >> PAGE_ADDRESS_LOW_SHIFT) & PAGE_ADDRESS_LOW_MASK;
            let count = (high << PAGE_ADDRESS_LOW_BITS) | low;
            let pages = signed_from_field(count, PAGE_ADDRESS_PAGES_BITS);
            return Some(Self::PageAddress { target, pages });
        }
        if word & MOVE_WIDE_WITH_ZERO_MASK == MOVE_WIDE_WITH_ZERO_BITS {
            let immediate = (word >> MOVE_WIDE_IMMEDIATE_SHIFT) & u32::from(u16::MAX);
            let immediate = u16::try_from(immediate).expect("16 bits fit in 16");
            return Some(Self::MoveWideWithZero { target, immediate });
        }
        None
    }
}

/// Returns `value` as the low `bits` of a field, its top bit the sign.
fn field_from_signed(value: i32, bits: u32) -> u32 {
    assert!(bits >= 1, "a field is at least one bit");
    assert!(bits < u32::BITS, "a field is narrower than a word");
    let field = value.cast_unsigned() & ((1 << bits) - 1);
    assert_eq!(signed_from_field(field, bits), value, "the value fits in its field");
    field
}

/// Returns `field`, the low `bits` of a word, as a signed number, its top bit the sign.
fn signed_from_field(field: u32, bits: u32) -> i32 {
    assert!(bits >= 1, "a field is at least one bit");
    assert!(bits < u32::BITS, "a field is narrower than a word");
    assert!(field < 1 << bits, "the field holds no more than its bits");
    // Shifting the field's top bit to the word's makes it the sign, and shifting back, as a
    // signed number, copies it down.
    let unused = u32::BITS - bits;
    (field << unused).cast_signed() >> unused
}
