// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for the sentences the tokens lint's problems print. Each sentence is pinned exactly,
//! because people and tools both read them.

#![forbid(unsafe_code)]

use stella_lexer::TokenKind;
use stella_lexer::TokenKind::{
    BraceClose, BraceOpen, Comment, Function, Name, ParenthesisClose, ParenthesisOpen, Returns,
    TypeName,
};
use stella_linter_tokens::Kind::{
    BraceCloseNotAlone, BraceClosesNothing, BraceNeverClosed, BraceOpenNotLast,
    BraceOpenStartsLine, IndentationNotDepth, LineEndsWithoutBrace, SpaceMissing, SpaceUnwanted,
    SpacesExtra,
};
use stella_linter_tokens::Neighbors;

/// Returns the two kinds as neighbors, `before` first.
// TS-DISTINCT-ARGS: the two kinds are in the order the source has them, and an expectation with
// them swapped fails its test.
const fn between(before: TokenKind, after: TokenKind) -> Neighbors {
    Neighbors { before, after }
}

#[test]
fn sentences() {
    // Every kind of problem the tokens lint has, each with its sentence written out whole.
    for (kind, sentence) in [
        (BraceOpenNotLast, "`{` must end its line."),
        (BraceOpenStartsLine, "`{` can't start a line."),
        (BraceCloseNotAlone, "`}` must be on a line of its own."),
        (LineEndsWithoutBrace, "a line of code must end with `{` or `}`."),
        (BraceClosesNothing, "this `}` has no `{` to close."),
        (BraceNeverClosed, "this `{` is never closed."),
        (IndentationNotDepth(0), "this line can't be indented, since no `{` is still open on it."),
        (
            IndentationNotDepth(1),
            "this line must be indented 4 spaces, one level of four for each `{` still open on \
             it.",
        ),
        (
            IndentationNotDepth(2),
            "this line must be indented 8 spaces, one level of four for each `{` still open on \
             it.",
        ),
        (
            IndentationNotDepth(89),
            "this line must be indented 356 spaces, one level of four for each `{` still open on \
             it.",
        ),
        (
            SpaceMissing(between(TypeName, BraceOpen)),
            "one space must go between a type's name and `{`.",
        ),
        (
            SpaceMissing(between(ParenthesisClose, Returns)),
            "one space must go between `)` and `returns`.",
        ),
        (SpaceUnwanted(between(Name, ParenthesisOpen)), "no space can go between a name and `(`."),
        (
            SpaceUnwanted(between(ParenthesisOpen, ParenthesisClose)),
            "no space can go between `(` and `)`.",
        ),
        (
            SpacesExtra(between(Function, Name)),
            "only one space can go between `function` and a name.",
        ),
    ] {
        assert_eq!(kind.to_string(), sentence, "for {kind:?}");
    }
    // Every kind of token is named the same way in each sentence about the space between two,
    // in either place, even the pairs that can't share a line.
    let named = [
        (Function, "`function`"),
        (Returns, "`returns`"),
        (Name, "a name"),
        (TypeName, "a type's name"),
        (ParenthesisOpen, "`(`"),
        (ParenthesisClose, "`)`"),
        (BraceOpen, "`{`"),
        (BraceClose, "`}`"),
        (Comment, "a comment"),
    ];
    for (first, first_named) in named {
        for (second, second_named) in named {
            let neighbors = between(first, second);
            let ending = format!("between {first_named} and {second_named}.");
            for (kind, sentence) in [
                (SpaceMissing(neighbors), format!("one space must go {ending}")),
                (SpaceUnwanted(neighbors), format!("no space can go {ending}")),
                (SpacesExtra(neighbors), format!("only one space can go {ending}")),
            ] {
                assert_eq!(kind.to_string(), sentence, "for {kind:?}");
            }
        }
    }
}
