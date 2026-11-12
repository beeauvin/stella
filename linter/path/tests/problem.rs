// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for the sentences the path lint's problems print. Each sentence is pinned exactly,
//! because people and tools both read them.

#![forbid(unsafe_code)]

use stella_linter_path::Problem;

#[test]
fn sentences() {
    // Every problem the path lint has, each with its sentence written out whole.
    for (problem, sentence) in [
        (Problem::Absolute, "the build path can't be established from an absolute path."),
        (Problem::Parent, "the build path can't be established from a path that uses `..`."),
        (
            Problem::EmptyPart,
            "a path can't have an empty part, like the one `//` or a trailing `/` makes.",
        ),
        (Problem::DotPart, "`.` can't be a part of a path, except as one leading `./`."),
        (
            Problem::DirectoryNotKebab,
            "a directory's name must be lowercase letters and digits joined by single hyphens, \
             like `hello-world`.",
        ),
        (Problem::NameNotStella, "a source file's name must end in `.stella`."),
        (Problem::NameEmpty, "a source file needs a name before `.stella`."),
        (
            Problem::NameNotKebab,
            "a source file's name, before `.stella`, must be lowercase letters and digits joined \
             by single hyphens, like `hello-world`.",
        ),
    ] {
        assert_eq!(problem.to_string(), sentence, "for {problem:?}");
    }
}
