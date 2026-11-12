// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for the generator. Every fuzz test's floors were measured on its sequence, so the
//! sequence is pinned: the same seed must give the same numbers for good.

use std::iter::repeat_with;

use stella_test_support::Rng;

/// The first three numbers below 1,000 from a seed of 1.
const PINNED: [usize; 3] = [165, 517, 103];

#[test]
fn a_seed_gives_the_same_numbers_every_time() {
    // Two generators from one seed agree, draw for draw, and another seed gives other numbers.
    let (mut one, mut other, mut another) = (Rng::new(7), Rng::new(7), Rng::new(8));
    let draws: Vec<usize> = repeat_with(|| one.below(1_000_000)).take(100).collect();
    let again: Vec<usize> = repeat_with(|| other.below(1_000_000)).take(100).collect();
    let differently: Vec<usize> = repeat_with(|| another.below(1_000_000)).take(100).collect();
    assert_eq!(draws, again);
    assert_ne!(draws, differently);
    // The sequence itself is pinned, since the fuzz tests' floors were measured on it. These are
    // xorshift64*'s first three numbers from a seed of 1, as the algorithm's shifts and its
    // multiplier give them, each taken below 1,000.
    let mut pinned = Rng::new(1);
    assert_eq!([pinned.below(1_000), pinned.below(1_000), pinned.below(1_000)], PINNED);
}

#[test]
fn below_stays_below_and_reaches_every_number() {
    // Every draw is below its count, and over many draws a small count's every number comes up.
    let mut rng = Rng::new(0x5745_4C4C_4100_0001);
    let mut seen = [false; 6];
    for _ in 0_u32..1_000 {
        let draw = rng.below(seen.len());
        assert!(draw < seen.len());
        seen[draw] = true;
    }
    assert_eq!(seen, [true; 6]);
    // Below 1, the only number is 0.
    assert_eq!(rng.below(1), 0);
}

#[test]
fn pick_returns_one_of_the_choices() {
    // Each pick is one of the choices, and over many picks every choice comes up.
    let mut rng = Rng::new(0x5745_4C4C_4100_0001);
    let choices = ["a", "b", "c"];
    let mut picked = [false; 3];
    for _ in 0_u32..200 {
        let pick = rng.pick(&choices);
        let index = choices.iter().position(|&choice| choice == pick);
        picked[index.expect("a pick is one of the choices")] = true;
    }
    assert_eq!(picked, [true; 3]);
}

#[test]
#[should_panic(expected = "a seed of 0 would only ever give 0")]
fn a_seed_of_zero_is_a_bug() {
    _ = Rng::new(0);
}

#[test]
#[should_panic(expected = "there's no number below 0 to return")]
fn below_zero_is_a_bug() {
    _ = Rng::new(1).below(0);
}

#[test]
#[should_panic(expected = "there's nothing to pick from")]
fn picking_from_nothing_is_a_bug() {
    _ = Rng::new(1).pick(&[]);
}
