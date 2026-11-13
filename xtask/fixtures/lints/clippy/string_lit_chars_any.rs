// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A character looked for among the characters of a string literal.

/// Says whether `letter` is a vowel.
#[must_use]
pub fn is_vowel(letter: char) -> bool {
    "aeiou".chars().any(|vowel| vowel == letter)
}
