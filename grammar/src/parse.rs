// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Parsing checks each node against the rule for the block it's in, and says what form it takes.
//!
//! A node is a line of the source that holds a comment, or code other than a `}` alone, as the
//! blocks made it. A block allows some forms of node and no others, and a comment is allowed in
//! every block and outside them. For now:
//!
//! - **Outside every block,** a node of code is a function's first line: `function`, the
//!   function's name, `(`, `)`, `returns`, the function's return type and `{`. The `{` opens the
//!   function's body. The parentheses are a small rule of their own, the parameters, which for
//!   now are empty.
//! - **In a function's body,** there's no code yet, so only comments are allowed.
//!
//! Nodes are checked in the order they're in the source, so a node's parent, which comes before
//! it, has always passed, and the block it opens is known. The first node that fails is the one
//! reported, at the first token at fault. How a line ends is the grammar's to check, not the
//! layout's, so a line can run out before its rule does. Then the part that's missing is reported
//! just past the line's last token, since there's no token in its place.
//!
//! There's one form for each node, in the same order.

use stella_blocks::{Blocks, NODES_MAX, Node};
use stella_lexer::Token;
use stella_lexer::TokenKind::{
    self, BraceClose, BraceOpen, Comment, Function, Name, ParenthesisClose, ParenthesisOpen,
    Returns, TypeName,
};
use stella_source::{Position, u32_from_usize, usize_from_u32};

use crate::problem::{Kind, Part, Problem};

/// A piece of a rule is one token: the kind it must be, and the part of the line it is, which a
/// problem names when some other token is in its place.
#[derive(Clone, Copy)]
struct Piece {
    kind: TokenKind,
    part: Part,
}

/// The parameters are `(` and `)`. For now a function takes none, so nothing goes between them.
const PARAMETERS: [Piece; 2] = [
    Piece { kind: ParenthesisOpen, part: Part::ParenthesisOpen },
    Piece { kind: ParenthesisClose, part: Part::ParenthesisClose },
];

/// A function's first line, after its `function`: the function's name, the parameters,
/// `returns`, the return type, and the `{` that opens its body. It's joined from the smaller
/// rules when the compiler is built, so checking it is one pass over its pieces.
const FUNCTION_LINE: [Piece; 6] = joined([
    &[Piece { kind: Name, part: Part::Name }],
    &PARAMETERS,
    &[
        Piece { kind: Returns, part: Part::Returns },
        Piece { kind: TypeName, part: Part::ReturnType },
        Piece { kind: BraceOpen, part: Part::BraceOpen },
    ],
]);

/// This is where a function's name is on its first line: the index of its token, counted from
/// the line's first, which is `function`.
///
/// It's read from the rule when the compiler is built, so whoever reads a name there reads the
/// token the rule checked.
pub const FUNCTION_NAME_TOKEN_INDEX: u32 = {
    // `function` comes before the rule's first piece.
    let (mut piece_index, mut token_index) = (0_usize, 1_u32);
    while !matches!(FUNCTION_LINE[piece_index].part, Part::Name) {
        piece_index += 1;
        token_index += 1;
    }
    token_index
};

/// This is where a function's return type is on its first line: the index of its token,
/// counted back from the line's last, which is `{`.
///
/// It's counted from the end, so it holds however many tokens the parameters take. Like the
/// name's, it's read from the rule.
pub const FUNCTION_RETURN_TYPE_TOKEN_INDEX_FROM_END: u32 = {
    let (mut piece_index, mut token_index_from_end) = (FUNCTION_LINE.len() - 1, 0_u32);
    while !matches!(FUNCTION_LINE[piece_index].part, Part::ReturnType) {
        piece_index -= 1;
        token_index_from_end += 1;
    }
    token_index_from_end
};

const _: () = assert!(
    FUNCTION_RETURN_TYPE_TOKEN_INDEX_FROM_END > 0,
    "a function's return type comes before the `{{` that ends its first line"
);

const _: () = assert!(
    ends_with_its_only_brace(&FUNCTION_LINE),
    "a function's first line ends with `{{` and holds no other, and a `{{` ends its line, so a \
     line that matches the rule's last piece has no token after it"
);

/// A form says what a node is, as the grammar proved it. Each node has exactly one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    /// The node is a comment.
    Comment,
    /// The node is a function's first line, which opens the function's body.
    Function,
}

/// The form of each of a source's nodes, whose tokens come in the order the grammar allows.
///
/// Only `parse` makes them, so holding `Forms` proves the source passed the text lint, the
/// lexer, the tokens lint and the grammar, and that its lines are nested in their blocks.
#[derive(Debug)]
pub struct Forms<'source> {
    blocks: Blocks<'source>,
    forms: Vec<Form>,
}

impl<'source> Forms<'source> {
    /// Returns the blocks, just as `nest` made them.
    #[must_use]
    pub const fn blocks(&self) -> &Blocks<'source> {
        &self.blocks
    }

    /// Returns the forms, one for each node, in the order the nodes are in. So the form of the
    /// node at an index is the form at that index.
    #[must_use]
    pub const fn as_slice(&self) -> &[Form] {
        self.forms.as_slice()
    }
}

const _: () = assert!(
    size_of::<Form>() == 1,
    "a form is one byte, so a source's forms take no more room than its nodes number"
);

/// Checks that `blocks`' nodes take the forms the grammar allows, and returns the form of each
/// as `Forms`.
///
/// # Errors
///
/// Returns the problem with the first node that takes no form its block allows, at its first
/// token out of place.
///
/// # Panics
///
/// Never, whatever the blocks. The assertions check what the blocks proved and the grammar's own
/// work, so one that fails is a bug in the compiler.
///
/// If there isn't the memory for their forms, Rust ends the process, which isn't a panic.
pub fn parse(blocks: Blocks<'_>) -> Result<Forms<'_>, Problem> {
    let nodes = blocks.nodes();
    assert!(nodes.len() <= usize_from_u32(NODES_MAX), "no source holds more than `NODES_MAX`");
    // Each node has one form, so this is room for every form, made once, before any is found.
    let mut forms = Vec::with_capacity(nodes.len());
    let capacity_before = forms.capacity();
    for node in nodes {
        let tokens = blocks.tokens_of(node);
        let form = match parse_node(node, tokens, &forms) {
            Ok(form) => form,
            Err(problem) => {
                let first = tokens[0].position();
                assert_eq!(problem.position.line(), first.line(), "a node's problem is on it");
                assert!(
                    problem.position.column() >= first.column(),
                    "a node's problem is at its first token, or after it"
                );
                return Err(problem);
            }
        };
        forms.push(form);
    }
    assert_eq!(forms.len(), nodes.len(), "each node has one form");
    assert_eq!(forms.capacity(), capacity_before, "the room made for the forms never grows");
    assert_parsed(&blocks, &forms);
    Ok(Forms { blocks, forms })
}

/// Checks `node`, whose tokens are `tokens`, against what the block it's in allows, and returns
/// its form, or its problem. `forms` holds the form of every node before it.
fn parse_node(node: &Node, tokens: &[Token], forms: &[Form]) -> Result<Form, Problem> {
    let first = &tokens[0];
    if first.kind() == Comment {
        assert_eq!(tokens.len(), 1, "a comment is alone on its line");
        return Ok(Form::Comment);
    }
    // A node of code is checked against what the block it's in allows, which its parent's form
    // says. The parent comes before it, so it has passed, and has its form.
    let Some(parent_index) = node.parent_index() else {
        return parse_function_line(tokens);
    };
    let parent_index = usize_from_u32(parent_index);
    assert!(parent_index < forms.len(), "a node's parent comes before it");
    match forms[parent_index] {
        Form::Function => Err(Problem { kind: Kind::CodeInBody, position: first.position() }),
        Form::Comment => unreachable!("a comment opens no block, so it's no node's parent"),
    }
}

/// Checks `tokens`, a node of code outside every block, against a function's first line, piece
/// by piece. Returns its form, or the problem at the first token out of place, or just past the
/// line's last token when the line ends before the rule does. A line that doesn't start with
/// `function` is refused for that, since outside every block nothing else is allowed.
fn parse_function_line(tokens: &[Token]) -> Result<Form, Problem> {
    let (first, rest) = tokens.split_first().expect("a node holds a token");
    let last = tokens.last().expect("a node holds a token");
    if first.kind() != Function {
        return Err(Problem { kind: Kind::NotAFunction, position: first.position() });
    }
    for (piece, token) in FUNCTION_LINE.iter().zip(rest) {
        if token.kind() != piece.kind {
            return Err(Problem { kind: Kind::Expected(piece.part), position: token.position() });
        }
    }
    // Every token matched its piece. A line with fewer tokens than the rule has pieces ended
    // early, and the first piece it lacks is the part that's missing.
    if let Some(missing) = FUNCTION_LINE.get(rest.len()) {
        return Err(Problem { kind: Kind::Expected(missing.part), position: just_past(last) });
    }
    // The rule's last piece is its only `{`, which ends its line, so the line has exactly the
    // rule's tokens.
    assert_eq!(rest.len(), FUNCTION_LINE.len(), "a function's first line is its rule's tokens");
    assert_eq!(last.kind(), BraceOpen, "a function's first line ends with `{{`");
    Ok(Form::Function)
}

/// Returns the position just past `token`, the last on its line, where a part that's missing
/// would have gone. A comment is alone on its line, so `token` is code, which is ASCII: it's as
/// many columns wide as it is bytes long.
fn just_past(token: &Token) -> Position {
    assert_ne!(token.kind(), Comment, "a comment is alone on its line, so it ends no line of code");
    let position = token.position();
    let width = u32_from_usize(token.span().len());
    Position::on_line(position.line()).at_column(position.column() + width)
}

/// Asserts that `forms`, made for `blocks`' nodes, hold together, as the AST will trust: a node
/// is a comment exactly when its first token is one, and a function exactly when it opens a
/// block. A function's first line is outside every block, starts with `function` and holds as
/// many tokens as its rule, and a function's body holds only comments.
fn assert_parsed(blocks: &Blocks<'_>, forms: &[Form]) {
    let nodes = blocks.nodes();
    assert_eq!(forms.len(), nodes.len(), "each node has one form");
    for (node, &form) in nodes.iter().zip(forms) {
        let tokens = blocks.tokens_of(node);
        let starts_with_comment = tokens[0].kind() == Comment;
        assert_eq!(form == Form::Comment, starts_with_comment, "a comment is a `Comment`");
        let opens_block = node.block().is_some();
        assert_eq!(form == Form::Function, opens_block, "only a function opens a block");
        if let Some(parent_index) = node.parent_index() {
            let parent = forms[usize_from_u32(parent_index)];
            assert_eq!(parent, Form::Function, "a node in a block is in a function's body");
            assert_eq!(form, Form::Comment, "a function's body holds only comments");
        }
        if form == Form::Function {
            assert!(node.parent_index().is_none(), "a function's first line is in no block");
            assert_eq!(tokens.len(), FUNCTION_LINE.len() + 1, "`function`, then its rule");
            assert_eq!(tokens[0].kind(), Function, "a function's first line starts `function`");
        }
    }
}

/// Says whether `pieces` end with `{`, and hold no other brace. It runs when the compiler is
/// built.
const fn ends_with_its_only_brace(pieces: &[Piece]) -> bool {
    let Some((last, before)) = pieces.split_last() else {
        return false;
    };
    let mut index = 0;
    while index < before.len() {
        if matches!(before[index].kind, BraceOpen | BraceClose) {
            return false;
        }
        index += 1;
    }
    matches!(last.kind, BraceOpen)
}

/// Returns `rules`, one after another, as one rule of `COUNT` pieces. It runs when the compiler
/// is built, and fails the build if the pieces don't number exactly `COUNT`.
const fn joined<const COUNT: usize, const RULES: usize>(
    rules: [&[Piece]; RULES],
) -> [Piece; COUNT] {
    // Every piece is written over before this returns, as the last assertion checks.
    let mut pieces = [Piece { kind: BraceOpen, part: Part::BraceOpen }; COUNT];
    let mut count = 0;
    let mut rule_index = 0;
    while rule_index < RULES {
        let rule = rules[rule_index];
        let mut piece_index = 0;
        while piece_index < rule.len() {
            assert!(count < COUNT, "the rules have no more pieces than the one they make");
            pieces[count] = rule[piece_index];
            count += 1;
            piece_index += 1;
        }
        rule_index += 1;
    }
    assert!(count == COUNT, "the rules have as many pieces as the one they make");
    pieces
}
