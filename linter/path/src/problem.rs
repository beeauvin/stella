// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! This module holds what the path lint reports when it refuses a path.

use std::fmt;

/// A problem is the one thing the path lint refused about a path.
///
/// It's always with the path as a whole, so it has no position. The problems are in the order
/// the checks run: absolute, parent, empty parts, `.` parts, directories' names, and then the
/// file's name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Problem {
    /// The path starts at the root, so there's no way to mirror it under `.build/`.
    Absolute,
    /// A part of the path is `..`, so it could leave the directory the compiler runs in.
    Parent,
    /// A part of the path is empty, as `//` or a trailing `/` makes.
    EmptyPart,
    /// A part of the path is `.`, other than one leading `./`.
    DotPart,
    /// A directory's name isn't lowercase letters and digits with single hyphens between words.
    DirectoryNotKebab,
    /// The file's name doesn't end in `.stella`. Case matters, so `hello.Stella` doesn't.
    NameNotStella,
    /// The file's name is `.stella` and nothing more.
    NameEmpty,
    /// The file's name before `.stella` isn't lowercase letters and digits with single hyphens
    /// between words.
    NameNotKebab,
}

/// A problem displays as the sentence that says what's wrong. A message prints it after the path.
impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match *self {
            Self::Absolute => "the build path can't be established from an absolute path.",
            Self::Parent => "the build path can't be established from a path that uses `..`.",
            Self::EmptyPart => {
                "a path can't have an empty part, like the one `//` or a trailing `/` makes."
            }
            Self::DotPart => "`.` can't be a part of a path, except as one leading `./`.",
            Self::DirectoryNotKebab => {
                "a directory's name must be lowercase letters and digits joined by single \
                 hyphens, like `hello-world`."
            }
            Self::NameNotStella => "a source file's name must end in `.stella`.",
            Self::NameEmpty => "a source file needs a name before `.stella`.",
            Self::NameNotKebab => {
                "a source file's name, before `.stella`, must be lowercase letters and digits \
                 joined by single hyphens, like `hello-world`."
            }
        })
    }
}
