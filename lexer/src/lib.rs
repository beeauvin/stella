// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Turns Stella source text into tokens.
//!
//! The lexer is the check after the text lint. `lex` takes the `Text` the text lint made, so it
//! can trust everything the lint guarantees, and returns the source's tokens, or the first
//! problem in it. The tokens go in a `TokenBuffer`, made once and lent to each source in turn. A
//! source that lexes comes back as `Tokens`, which only `lex` can make, so the tokens lint can
//! take them and trust them in turn.

#![forbid(unsafe_code)]

mod lex;
mod problem;

pub use lex::{TOKENS_MAX, Token, TokenBuffer, TokenKind, Tokens, lex};
pub use problem::{Kind, Problem};
