// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Building reads each function out of the nodes the grammar proved, into a flat table.
//!
//! For now a source holds functions and comments, and a function's body holds comments and at
//! most one `return` line, its last line of code, so:
//!
//! - **Outside every block,** each function's first line becomes a `Function`, in the order
//!   they're in the source. Its name and its return type are read from where the grammar's rule
//!   puts them on the line, which the grammar says: the name counted from the line's first
//!   token, and the return type counted back from its last. The grammar proved the line is
//!   exactly its rule, so the order isn't checked again: only the kinds of the tokens read are
//!   asserted.
//! - **In a function's body,** each `return` line becomes a `Statement::Return`, holding where its
//!   value is, read from where the grammar's rule puts it. A function's statements are a run of a
//!   flat table that holds every function's, in the order they're in the source, so a function
//!   holds where its run starts and how long it is.
//! - **A comment** means nothing to the checks after this one, so it isn't in the tree.
//!
//! The nodes outside every block are walked with `Blocks::top`, so stepping over a function's
//! body is a jump, and nothing needs recursion.

use std::ops::Range;

use stella_blocks::NODES_MAX;
use stella_grammar::{
    FUNCTION_NAME_TOKEN_INDEX, FUNCTION_RETURN_TYPE_TOKEN_INDEX_FROM_END, Form, Forms,
    RETURN_VALUE_TOKEN_INDEX,
};
use stella_lexer::TokenKind::{BraceClose, BraceOpen, Integer, Name, Return, TypeName};
use stella_lexer::{Token, TokenKind};
use stella_source::{SOURCE_BYTES_MAX, is_own, u32_from_usize, usize_from_u32};

/// This is a bound on how many functions a source can hold.
///
/// A function's first line opens a block, and the `}` that closes it is alone on a later line,
/// so a function takes at least two lines of at least two bytes each. No source reaches it
/// today: the shortest function is 27 bytes, so the most is 606. But it holds for any function
/// there could be.
pub const FUNCTIONS_MAX: u32 = SOURCE_BYTES_MAX / 4;

const _: () = assert!(
    FUNCTIONS_MAX <= NODES_MAX,
    "a function's first line is a node, so there are never more functions than nodes"
);

/// This is a bound on how many statements a source can hold. Each is a node, so there are never
/// more statements than nodes.
pub const STATEMENTS_MAX: u32 = NODES_MAX;

/// A function is one function in the source.
///
/// It holds where its parts are rather than the parts, so it's plain data that borrows nothing.
/// `Ast::keyword`, `Ast::name` and `Ast::return_type` give its tokens back, and `Ast::text` a
/// token's text. Only `build` makes one, so a function always names parts that are in some
/// source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Function {
    /// The index, in the nodes, of the function's first line. That node opens the function's
    /// body, so it reaches the body's nodes and the `}` that closes it.
    node_index: u32,
    /// The index, in the tokens, of the function's name.
    name_token_index: u32,
    /// The index, in the tokens, of the name of the type the function returns.
    return_type_token_index: u32,
    /// The index, in the AST's statements, of the function's first statement. When it has none,
    /// it's where its first would be.
    first_statement_index: u32,
    /// How many statements the function's body holds, in a run from its first.
    statements_count: u32,
}

const _: () = assert!(
    size_of::<Function>() == 20,
    "a source's functions are budgeted at 20 bytes each, about 80 KiB for `FUNCTIONS_MAX` of them"
);

/// A statement is one line of code in a function's body that does something.
///
/// It holds where its parts are rather than the parts, so it's plain data that borrows nothing.
/// Only `build` makes one, so a statement always names parts that are in some source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Statement {
    /// A `return` line, which gives back the function's value.
    Return {
        /// The index, in the nodes, of the `return` line.
        node_index: u32,
        /// The index, in the tokens, of the value it gives back, an integer.
        value_token_index: u32,
    },
}

const _: () = assert!(
    size_of::<Statement>() == 8,
    "a source's statements are budgeted at 8 bytes each, about 64 KiB for `STATEMENTS_MAX` of them"
);

impl Function {
    /// Returns the index, in the nodes, of the function's first line, which opens its body.
    #[must_use]
    pub const fn node_index(&self) -> u32 {
        self.node_index
    }
}

/// The AST of a source whose tokens come in the order the grammar allows: its functions, in the
/// order they're in the source.
///
/// Only `build` makes it, so holding an `Ast` proves the source passed the text lint, the lexer,
/// the tokens lint and the grammar, and that the AST was built from what they proved.
#[derive(Debug)]
pub struct Ast<'source> {
    forms: Forms<'source>,
    functions: Vec<Function>,
    statements: Vec<Statement>,
}

impl<'source> Ast<'source> {
    /// Returns what the grammar proved, just as `parse` made it.
    #[must_use]
    pub const fn forms(&self) -> &Forms<'source> {
        &self.forms
    }

    /// Returns the functions, in the order they're in the source.
    #[must_use]
    pub const fn functions(&self) -> &[Function] {
        self.functions.as_slice()
    }

    /// Returns the `function` that starts `function`'s first line.
    ///
    /// # Panics
    ///
    /// Panics if `function` isn't one of this AST's: the very function `functions` holds. A
    /// function only means something in the source it was built from. So one from another
    /// source is a bug, even when it equals one of these, and so is a copy of one of these.
    #[must_use]
    pub fn keyword(&self, function: &Function) -> &Token {
        self.assert_own(function);
        let node = &self.forms.blocks().nodes()[usize_from_u32(function.node_index)];
        let token = &self.tokens()[node.token_span().start];
        assert_eq!(token.kind(), TokenKind::Function, "a function's first line starts `function`");
        token
    }

    /// Returns `function`'s name.
    ///
    /// # Panics
    ///
    /// Panics if `function` isn't one of this AST's.
    #[must_use]
    pub fn name(&self, function: &Function) -> &Token {
        self.assert_own(function);
        let token = &self.tokens()[usize_from_u32(function.name_token_index)];
        assert_eq!(token.kind(), Name, "a function's name is a name");
        token
    }

    /// Returns the name of the type `function` returns.
    ///
    /// # Panics
    ///
    /// Panics if `function` isn't one of this AST's.
    #[must_use]
    pub fn return_type(&self, function: &Function) -> &Token {
        self.assert_own(function);
        let token = &self.tokens()[usize_from_u32(function.return_type_token_index)];
        assert_eq!(token.kind(), TypeName, "a function's return type is a type's name");
        token
    }

    /// Returns the statements in `function`'s body, in the order they're in the source.
    ///
    /// # Panics
    ///
    /// Panics if `function` isn't one of this AST's.
    #[must_use]
    pub fn statements(&self, function: &Function) -> &[Statement] {
        self.assert_own(function);
        let start = usize_from_u32(function.first_statement_index);
        let end = start + usize_from_u32(function.statements_count);
        assert!(end <= self.statements.len(), "a function's statements are the AST's");
        &self.statements[start..end]
    }

    /// Returns the `}` that closes `function`'s body.
    ///
    /// # Panics
    ///
    /// Panics if `function` isn't one of this AST's.
    #[must_use]
    pub fn close(&self, function: &Function) -> &Token {
        self.assert_own(function);
        let node = &self.forms.blocks().nodes()[usize_from_u32(function.node_index)];
        let block = node.block().expect("a function's first line opens its body");
        let token = &self.tokens()[usize_from_u32(block.close_token_index())];
        assert_eq!(token.kind(), BraceClose, "a function's body is closed by a `}}`");
        token
    }

    /// Returns the `return` that starts `statement`'s line.
    ///
    /// # Panics
    ///
    /// Panics if `statement` isn't one of this AST's: the very statement `statements` holds.
    #[must_use]
    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "KS-ASSERT-ARGS: a statement is checked to be this AST's own by where it is, \
                  which a copy would lose"
    )]
    pub fn keyword_of(&self, statement: &Statement) -> &Token {
        assert!(is_own(&self.statements, statement), "a statement is one of its source's");
        let Statement::Return { node_index, .. } = *statement;
        let node = &self.forms.blocks().nodes()[usize_from_u32(node_index)];
        let token = &self.tokens()[node.token_span().start];
        assert_eq!(token.kind(), Return, "a `return` line starts with `return`");
        token
    }

    /// Returns the value `statement` gives back, an integer.
    ///
    /// # Panics
    ///
    /// Panics if `statement` isn't one of this AST's.
    #[must_use]
    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "KS-ASSERT-ARGS: a statement is checked to be this AST's own by where it is, \
                  which a copy would lose"
    )]
    pub fn value(&self, statement: &Statement) -> &Token {
        assert!(is_own(&self.statements, statement), "a statement is one of its source's");
        let Statement::Return { value_token_index, .. } = *statement;
        let token = &self.tokens()[usize_from_u32(value_token_index)];
        assert_eq!(token.kind(), Integer, "a `return` line's value is an integer");
        token
    }

    /// Returns `token`'s text, as it is in the source.
    ///
    /// # Panics
    ///
    /// Panics if `token` isn't one of this source's tokens.
    #[must_use]
    pub fn text(&self, token: &Token) -> &'source str {
        self.forms.blocks().tokens().text(token)
    }

    /// Returns the source's tokens, just as the lexer made them.
    const fn tokens(&self) -> &[Token] {
        self.forms.blocks().tokens().as_slice()
    }

    /// Asserts that `function` is one of this AST's: the very function `functions` holds.
    fn assert_own(&self, function: &Function) {
        assert!(is_own(&self.functions, function), "a function is one of its source's functions");
    }
}

/// Builds the AST of what `forms` holds, and returns it. Nothing can be refused, since the
/// grammar has proved everything this relies on.
///
/// # Panics
///
/// Never, whatever was parsed. The assertions check what the grammar proved and the building's own
/// work, so one that fails is a bug in the compiler.
///
/// If there isn't the memory for its functions, Rust ends the process, which isn't a panic.
#[must_use]
pub fn build(forms: Forms<'_>) -> Ast<'_> {
    let blocks = forms.blocks();
    let nodes_count = blocks.nodes().len();
    assert_eq!(forms.as_slice().len(), nodes_count, "each node has one form");
    // A function's first line is a node, and no source holds more than `FUNCTIONS_MAX`
    // functions. So this is room for every function the source can hold, made once.
    let functions_count_max = nodes_count.min(usize_from_u32(FUNCTIONS_MAX));
    let mut functions = Vec::with_capacity(functions_count_max);
    // A statement is a node too, so this is room for every statement, made once.
    let mut statements = Vec::with_capacity(nodes_count.min(usize_from_u32(STATEMENTS_MAX)));
    let capacities_before = (functions.capacity(), statements.capacity());
    for node_index in blocks.top() {
        match forms.as_slice()[usize_from_u32(node_index)] {
            // A comment means nothing to the checks after this one, so it isn't in the tree.
            Form::Comment => {}
            Form::Function => {
                let room = functions.len() < functions_count_max;
                assert!(room, "a source has no more functions than nodes, or than the most");
                functions.push(build_function(&forms, node_index, &mut statements));
            }
            Form::Return => unreachable!("a `return` line is in a function's body"),
        }
    }
    let capacities = (functions.capacity(), statements.capacity());
    assert_eq!(capacities, capacities_before, "the room for the tree never grows");
    assert_built(&forms, &functions, &statements);
    Ast { forms, functions, statements }
}

/// Returns the function whose first line is the node at `node_index`, which the grammar proved
/// is a function's first line, outside every block, and adds its body's statements to
/// `statements`.
fn build_function(forms: &Forms<'_>, node_index: u32, statements: &mut Vec<Statement>) -> Function {
    let blocks = forms.blocks();
    let node = &blocks.nodes()[usize_from_u32(node_index)];
    assert!(node.parent_index().is_none(), "a function's first line is outside every block");
    assert!(node.block().is_some(), "a function's first line opens its body");
    // The grammar's rule says where the two are: the name counted from the line's first token,
    // and the return type counted back from its last.
    let (name_token_index, return_type_token_index) = function_part_indices(&node.token_span());
    let first_statement_index = statements.len();
    for child_index in blocks.children(node_index) {
        match forms.as_slice()[usize_from_u32(child_index)] {
            Form::Comment => {}
            Form::Return => {
                let span = blocks.nodes()[usize_from_u32(child_index)].token_span();
                let value_token_index = span.start + usize_from_u32(RETURN_VALUE_TOKEN_INDEX);
                assert!(value_token_index < span.end, "a `return` line holds its value");
                let room = statements.len() < statements.capacity();
                assert!(room, "a source has no more statements than nodes");
                statements.push(Statement::Return {
                    node_index: child_index,
                    value_token_index: u32_from_usize(value_token_index),
                });
            }
            Form::Function => unreachable!("a function's body holds no function"),
        }
    }
    Function {
        node_index,
        name_token_index: u32_from_usize(name_token_index),
        return_type_token_index: u32_from_usize(return_type_token_index),
        first_statement_index: u32_from_usize(first_statement_index),
        statements_count: u32_from_usize(statements.len() - first_statement_index),
    }
}

/// Returns where a function's name and its return type are among the tokens, for a function
/// whose first line's tokens are `span`: where the grammar's rule puts each on the line.
fn function_part_indices(span: &Range<usize>) -> (usize, usize) {
    let name_index_on_line = usize_from_u32(FUNCTION_NAME_TOKEN_INDEX);
    let return_type_index_from_end = usize_from_u32(FUNCTION_RETURN_TYPE_TOKEN_INDEX_FROM_END);
    // The line holds the tokens up to its name, those from its return type on, and the last.
    assert!(
        span.len() > name_index_on_line + return_type_index_from_end + 1,
        "a function's first line has room for its name before its return type"
    );
    (span.start + name_index_on_line, span.end - 1 - return_type_index_from_end)
}

/// Asserts that `functions`, built from `forms`, hold together, as the checks after the AST
/// will trust: one function for each function's first line, in the order they're in the source,
/// each with a name and a return type that are a name and a type's name, on its first line,
/// between `function` and `{`. Its statements are a run of the table, the runs in the order of
/// the functions and covering it, and are exactly its body's `return` lines, in order, each
/// holding its integer.
fn assert_built(forms: &Forms<'_>, functions: &[Function], statements: &[Statement]) {
    let blocks = forms.blocks();
    let tokens = blocks.tokens().as_slice();
    assert!(functions.len() <= usize_from_u32(FUNCTIONS_MAX), "no source holds more functions");
    let lines_count = forms.as_slice().iter().filter(|&&form| form == Form::Function).count();
    assert_eq!(functions.len(), lines_count, "each function's first line is one function");
    let in_order = functions.is_sorted_by(|first, second| first.node_index < second.node_index);
    assert!(in_order, "the functions are in the order of their first lines");
    let mut covered_count: usize = 0;
    for function in functions {
        let form = forms.as_slice()[usize_from_u32(function.node_index)];
        assert_eq!(form, Form::Function, "it's a function's node");
        let span = blocks.nodes()[usize_from_u32(function.node_index)].token_span();
        let name_index = usize_from_u32(function.name_token_index);
        let return_type_index = usize_from_u32(function.return_type_token_index);
        assert_eq!(tokens[span.start].kind(), TokenKind::Function, "`function` comes first");
        assert!(span.start < name_index, "a function's name comes after `function`");
        assert!(name_index < return_type_index, "a function's name comes before its type");
        assert!(return_type_index < span.end - 1, "a return type comes before `{{`");
        assert_eq!(tokens[name_index].kind(), Name, "a function's name is a name");
        assert_eq!(tokens[return_type_index].kind(), TypeName, "a return type is a type's name");
        assert_eq!(tokens[span.end - 1].kind(), BraceOpen, "a function's first line ends `{{`");
        assert_eq!(
            usize_from_u32(function.first_statement_index),
            covered_count,
            "each function's statements follow the one before's"
        );
        let returns = blocks.children(function.node_index).filter(|&child_index| {
            let form = forms.as_slice()[usize_from_u32(child_index)];
            assert_ne!(form, Form::Function, "a function's body holds no function");
            form == Form::Return
        });
        let start = usize_from_u32(function.first_statement_index);
        let count = usize_from_u32(function.statements_count);
        let run = &statements[start..start + count];
        let run_nodes = run.iter().map(|&Statement::Return { node_index, .. }| node_index);
        assert!(run_nodes.eq(returns), "a function's statements are its body's `return` lines");
        for &Statement::Return { value_token_index, .. } in run {
            let value = tokens[usize_from_u32(value_token_index)].kind();
            assert_eq!(value, Integer, "a `return` line's value is an integer");
        }
        covered_count += count;
    }
    assert_eq!(covered_count, statements.len(), "every statement is a function's");
}
