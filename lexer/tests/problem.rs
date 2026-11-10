// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for the sentences the lexer's problems print. Each sentence is pinned exactly,
//! because people and tools both read them.

use stella_lexer::Kind::*;

#[test]
fn sentences() {
    for (kind, sentence) in [
        (Unexpected(';'), "`;` can't start a token."),
        (Unexpected('9'), "`9` can't start a token."),
        (Unexpected('_'), "`_` can't start a token."),
        (Unexpected('/'), "`/` can't start a token."),
        (Unexpected('"'), "`\"` can't start a token."),
        // A backtick can't sit between backticks, so it's written the way Markdown writes one.
        (Unexpected('`'), "`` ` `` can't start a token."),
        (
            NameNotSnakeCase,
            "a name must be lowercase letters and digits joined by single underscores, like \
             `my_function`.",
        ),
        (
            TypeNameNotPascalCase,
            "a type's name must be words that each start with one capital, then lowercase letters \
             and digits, like `Void` or `MessageQueue`.",
        ),
    ] {
        assert_eq!(kind.to_string(), sentence, "for {kind:?}");
    }
    // Every other character the lexer can refuse is shown the same way: printable ASCII other
    // than a space, which separates tokens.
    for character in '!'..='~' {
        if character == '`' {
            continue;
        }
        let sentence = format!("`{character}` can't start a token.");
        assert_eq!(Unexpected(character).to_string(), sentence, "for {character:?}");
    }
}
