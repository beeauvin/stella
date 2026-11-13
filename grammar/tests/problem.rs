// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for the sentences the grammar's problems print. Each sentence is pinned exactly,
//! because people and tools both read them.

#![forbid(unsafe_code)]

use stella_grammar::Kind::{CodeInBody, Expected, NotAFunction};
use stella_grammar::Part::{
    BraceOpen, Name, ParenthesisClose, ParenthesisOpen, ReturnType, Returns,
};

#[test]
fn sentences() {
    // Every kind of problem the grammar has, and every part a function's first line can be
    // missing, each with its sentence written out whole.
    for (kind, sentence) in [
        (NotAFunction, "outside a block, a line of code must start with `function`."),
        (CodeInBody, "a function's body can hold only comments, for now."),
        (Expected(Name), "the function's name must come after `function`."),
        (Expected(ParenthesisOpen), "`(` must come after the function's name."),
        (
            Expected(ParenthesisClose),
            "`)` must come after `(`, since a function takes no parameters, for now.",
        ),
        (Expected(Returns), "`returns` and the function's return type must come after `()`."),
        (Expected(ReturnType), "the function's return type must come after `returns`."),
        (Expected(BraceOpen), "`{` must come after the function's return type."),
    ] {
        assert_eq!(kind.to_string(), sentence, "for {kind:?}");
    }
}
