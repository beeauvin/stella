// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A small deterministic generator, so a fuzz test that fails replays from its seed.

/// A generator of random numbers, xorshift64*. The same seed gives the same numbers on every run
/// and every machine.
#[derive(Debug)]
pub struct Rng {
    state: u64,
}

impl Rng {
    /// Returns a generator that starts from `seed`.
    ///
    /// # Panics
    ///
    /// Panics if `seed` is 0, since xorshift never leaves 0.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        assert!(seed != 0, "a seed of 0 would only ever give 0");
        Self { state: seed }
    }

    /// Returns a number from 0 up to, but not including, `count`.
    ///
    /// # Panics
    ///
    /// Panics if `count` is 0, since no number is below it.
    pub fn below(&mut self, count: usize) -> usize {
        assert!(count > 0, "there's no number below 0 to return");
        let count = u64::try_from(count).expect("a `usize` fits in a `u64`");
        usize::try_from(self.next() % count).expect("a number below a `usize` is a `usize`")
    }

    /// Returns one of `choices`.
    ///
    /// # Panics
    ///
    /// Panics if there are no choices.
    pub fn pick<'choice>(&mut self, choices: &[&'choice str]) -> &'choice str {
        assert!(!choices.is_empty(), "there's nothing to pick from");
        choices[self.below(choices.len())]
    }

    /// Returns the next number in the sequence.
    const fn next(&mut self) -> u64 {
        self.state ^= self.state >> 12_u32;
        self.state ^= self.state << 25_u32;
        self.state ^= self.state >> 27_u32;
        self.state.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}
