// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for the sentences the grammar's problems print. Each sentence is pinned exactly,
//! because people and tools both read them.

#![forbid(unsafe_code)]

use stella_grammar::Kind::{AfterReturn, AfterReturnValue, Expected, NotAFunction, NotAReturn};
use stella_grammar::Part::{
    BraceOpen, Name, ParenthesisClose, ParenthesisOpen, ReturnType, ReturnValue, Returns,
};

#[test]
fn sentences() {
    // Every kind of problem the grammar has, and every part a line can be missing, each with its
    // sentence written out whole.
    for (kind, sentence) in [
        (NotAFunction, "outside a block, a line of code must start with `function`."),
        (NotAReturn, "in a function's body, a line of code must start with `return`, for now."),
        (AfterReturn, "nothing can come after `return` in a body."),
        (AfterReturnValue, "a line with `return` ends with its value."),
        (Expected(Name), "the function's name must come after `function`."),
        (Expected(ParenthesisOpen), "`(` must come after the function's name."),
        (
            Expected(ParenthesisClose),
            "`)` must come after `(`, since a function takes no parameters, for now.",
        ),
        (Expected(Returns), "`returns` and the function's return type must come after `()`."),
        (Expected(ReturnType), "the function's return type must come after `returns`."),
        (Expected(BraceOpen), "`{` must come after the function's return type."),
        (Expected(ReturnValue), "an integer must come after `return`."),
    ] {
        assert_eq!(kind.to_string(), sentence, "for {kind:?}");
    }
}
