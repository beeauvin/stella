// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for the sentences the meaning checks' problems print, and for where each problem
//! is. Each sentence is pinned exactly, because people and tools both read them.

#![forbid(unsafe_code)]

use stella_meaning::Kind::{
    AnotherFunction, DoesntFit, MissingReturn, NoSuchType, NotMain, ReturnInVoid,
};
use stella_meaning::{IntegerType, Problem};
use stella_source::{Position, SOURCE_BYTES_MAX};

#[test]
fn sentences() {
    // The problem with the file as a whole has a sentence of its own.
    assert_eq!(Problem::NoMain.to_string(), "a program needs a function named `main`.");
    // Every kind of problem at a token, each with its sentence written out whole. A problem
    // prints its kind's sentence, wherever it is.
    for (kind, sentence) in [
        (NoSuchType, "this type doesn't exist; for now, the types are `Void` and `U8`."),
        (NotMain, "a program's one function must be named `main`, for now."),
        (AnotherFunction, "a program holds only one function, for now."),
        (ReturnInVoid, "a function that returns `Void` ends without `return`."),
        (
            MissingReturn(IntegerType::U8),
            "a function that returns `U8` must end with `return` and a value.",
        ),
        (DoesntFit(IntegerType::U8), "this integer doesn't fit in `U8`, which holds 0 to 255."),
    ] {
        assert_eq!(kind.to_string(), sentence, "for {kind:?}");
        let problem = Problem::At { kind, position: Position::on_line(3).at_column(32) };
        assert_eq!(problem.to_string(), sentence, "for {problem:?}");
    }
}

#[test]
fn places() {
    // Only a missing `main` is with the file as a whole, so it has no position. Every other
    // problem is at a token, and its position is the one it was given: the first a position can
    // be, one in the middle, and the last.
    assert_eq!(Problem::NoMain.position(), None, "a missing `main` is nowhere in the file");
    let positions = [
        Position::on_line(1).at_column(1),
        Position::on_line(3).at_column(32),
        Position::on_line(SOURCE_BYTES_MAX).at_column(SOURCE_BYTES_MAX + 1),
    ];
    let kinds = [
        NoSuchType,
        NotMain,
        AnotherFunction,
        ReturnInVoid,
        MissingReturn(IntegerType::U8),
        DoesntFit(IntegerType::U8),
    ];
    for kind in kinds {
        for position in positions {
            let problem = Problem::At { kind, position };
            assert_eq!(problem.position(), Some(position), "for {problem:?}");
        }
    }
}
