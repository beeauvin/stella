// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Nesting cuts a layout into nodes, and puts each node in the block it's in.
//!
//! A node is a line of the source that holds tokens, other than a `}`'s: a comment, or a line of
//! code. It isn't called a line, because not every line of a source is one, and a node's index
//! isn't its line's number. The tokens lint has proved how a line of code looks, so it's one of
//! two kinds:
//!
//! - **A line that opens a block** ends with `{`. It's a node, and the nodes after it are in its
//!   block, up to the `}` that closes it.
//! - **A `}`** is alone on its line, and closes the innermost block still open. Its line isn't a
//!   node: the block it closes records which token it is.
//!
//! A comment is a node that opens no block. The layout lets no line of code end without a
//! brace, so every other node opens one. A blank line holds no tokens, so it isn't a node.
//!
//! The nodes are kept flat, in the order they're in the source. Each node knows the node whose
//! block it's in, and a node that opens a block knows where its block ends. So a block's nodes
//! run from the one after the node that opens it to its end, and stepping over a block is a jump.
//! Nothing needs recursion, and nothing needs a stack.

use std::iter::FusedIterator;
use std::ops::Range;

use stella_lexer::TokenKind::{
    BraceClose, BraceOpen, Comment, Function, Name, ParenthesisClose, ParenthesisOpen, Returns,
    TypeName,
};
use stella_lexer::{TOKENS_MAX, Token, Tokens};
use stella_linter_text::LEVEL_SPACES;
use stella_linter_tokens::Layout;
use stella_source::{SOURCE_BYTES_MAX, is_own, u32_from_usize, usize_from_u32};

/// This is a bound on how many nodes a source can hold.
///
/// Every node is a line that holds a token and ends with a `\n`, so it's at least two bytes. No
/// source reaches it today: a comment is at least three bytes, and so is a line that opens a
/// block, so the most is 5,461. But it holds for any node there could be.
pub const NODES_MAX: u32 = SOURCE_BYTES_MAX / 2;

/// A node is one line of the source that holds tokens, other than a `}`'s, and where it sits
/// among the blocks.
///
/// It holds where its tokens are rather than the tokens, so it's plain data that borrows
/// nothing. `Blocks::tokens_of` gives the tokens back. Only `nest` makes one, so a node always
/// names tokens that are in some source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Node {
    /// The index of the node's first token, in the layout's tokens.
    first_token_index: u32,
    /// The index just past the node's last token, in the layout's tokens. When the node opens a
    /// block, its last token is its `{`.
    end_token_index: u32,
    /// The index, in the nodes, of the node whose block this node is directly in, or `None` for
    /// a node in no block.
    parent_index: Option<u32>,
    /// The block this node opens, when it ends with `{`.
    block: Option<Block>,
}

const _: () = assert!(
    size_of::<Node>() == 28,
    "a source's nodes are budgeted at 28 bytes each, about 224 KiB for `NODES_MAX` of them"
);

impl Node {
    /// Returns where the node's tokens are in the layout's tokens, as a range of indices. There's
    /// at least one, and when the node opens a block, the last of them is its `{`.
    ///
    /// # Panics
    ///
    /// Never, for a node `nest` made, and nothing else makes one. The assertion checks the
    /// nesting's own work.
    #[must_use]
    pub const fn token_span(&self) -> Range<usize> {
        assert!(self.first_token_index < self.end_token_index, "a node holds a token");
        usize_from_u32(self.first_token_index)..usize_from_u32(self.end_token_index)
    }

    /// Returns the index, in the nodes, of the node whose block this node is directly in, or
    /// `None` for a node in no block.
    #[must_use]
    pub const fn parent_index(&self) -> Option<u32> {
        self.parent_index
    }

    /// Returns the block this node opens, when it ends with `{`.
    #[must_use]
    pub const fn block(&self) -> Option<Block> {
        self.block
    }
}

/// A block is the nodes between a `{` and the `}` that closes it.
///
/// The node that ends with the `{` opens it, so its nodes start just after that one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Block {
    /// The index, in the nodes, just past the block's last node, counting the nodes of the
    /// blocks inside it. An empty block ends just past the node that opens it.
    end_node_index: u32,
    /// The index of the `}` that closes the block, in the layout's tokens.
    close_token_index: u32,
}

impl Block {
    /// Returns the index, in the nodes, just past the block's last node, counting the nodes of
    /// the blocks inside it. An empty block ends just past the node that opens it.
    #[must_use]
    pub const fn end_node_index(self) -> u32 {
        self.end_node_index
    }

    /// Returns the index of the `}` that closes the block, in the layout's tokens.
    #[must_use]
    pub const fn close_token_index(self) -> u32 {
        self.close_token_index
    }
}

/// The nodes of a source laid out the one way, each in the block it's in, and the layout they
/// were nested from.
///
/// Only `nest` makes them, so holding `Blocks` proves the source passed the text lint, the lexer
/// and the tokens lint.
#[derive(Debug)]
pub struct Blocks<'source> {
    layout: Layout<'source>,
    nodes: Vec<Node>,
}

impl<'source> Blocks<'source> {
    /// Returns the tokens, just as the lexer made them.
    #[must_use]
    pub const fn tokens(&self) -> &Tokens<'source> {
        self.layout.tokens()
    }

    /// Returns the nodes, in the order they're in the source.
    #[must_use]
    pub const fn nodes(&self) -> &[Node] {
        self.nodes.as_slice()
    }

    /// Returns `node`'s tokens, in order.
    ///
    /// # Panics
    ///
    /// Panics if `node` isn't one of these nodes: the very node `nodes` holds. A node only
    /// means something in the source it was nested from. So one from another source is a bug,
    /// even when it equals one of these, and so is a copy of one of these.
    #[must_use]
    pub fn tokens_of(&self, node: &Node) -> &[Token] {
        assert!(is_own(&self.nodes, node), "a node is one of its source's nodes");
        let tokens = self.tokens().as_slice();
        let span = node.token_span();
        assert!(span.end <= tokens.len(), "a node's tokens are among its source's");
        &tokens[span]
    }

    /// Returns the indices of the nodes in no block, in order. A node that opens a block comes
    /// without the nodes in it.
    #[must_use]
    pub fn top(&self) -> Children<'_> {
        let end_node_index = u32_from_usize(self.nodes.len());
        Children { nodes: &self.nodes, next_node_index: 0, end_node_index }
    }

    /// Returns the indices of the nodes directly in the block the node at `index` opens, in
    /// order. A node that opens a block of its own comes without the nodes in it.
    ///
    /// # Panics
    ///
    /// Panics if there's no node at `index`, or if the node there opens no block.
    #[must_use]
    pub fn children(&self, index: u32) -> Children<'_> {
        let nodes = self.nodes.as_slice();
        assert!(usize_from_u32(index) < nodes.len(), "node {index} is one of the nodes");
        let block = nodes[usize_from_u32(index)].block;
        assert!(block.is_some(), "node {index} opens no block, so it has no children");
        let block = block.expect("the node opens a block, as just asserted");
        assert!(block.end_node_index > index, "a block ends after the node that opens it");
        assert!(
            usize_from_u32(block.end_node_index) <= nodes.len(),
            "a block ends within the nodes"
        );
        Children { nodes, next_node_index: index + 1, end_node_index: block.end_node_index }
    }
}

/// Children are the nodes directly in one block, or in no block, as indices into the nodes.
///
/// A node that opens a block comes without the nodes in it, which are its own children. Once the
/// children are done, they stay done.
#[derive(Clone, Debug)]
pub struct Children<'blocks> {
    /// All the nodes, so the children can step over a block.
    nodes: &'blocks [Node],
    /// The index of the next child, when it's less than the end.
    next_node_index: u32,
    /// The index just past the last node the children can come from.
    end_node_index: u32,
}

impl Iterator for Children<'_> {
    type Item = u32;

    /// Returns the next child's index, and steps over its block, if it opens one.
    fn next(&mut self) -> Option<u32> {
        assert!(
            usize_from_u32(self.end_node_index) <= self.nodes.len(),
            "children end within the nodes"
        );
        if self.next_node_index >= self.end_node_index {
            return None;
        }
        let index = self.next_node_index;
        // A node that opens a block comes without the nodes in it, so the next child is the node
        // just past its block.
        self.next_node_index = match self.nodes[usize_from_u32(index)].block {
            Some(block) => block.end_node_index,
            None => index + 1,
        };
        assert!(self.next_node_index > index, "the children move forward");
        assert!(self.next_node_index <= self.end_node_index, "the children stay within their end");
        Some(index)
    }
}

impl FusedIterator for Children<'_> {}

/// Nests `layout`'s nodes in their blocks, and returns them. Nothing can be refused, since the
/// tokens lint has proved everything this relies on.
///
/// # Panics
///
/// Never, whatever the layout. The assertions check what the tokens lint proved and the nesting's
/// own work, so one that fails is a bug in the compiler.
///
/// If there isn't the memory for its nodes, Rust ends the process, which isn't a panic.
#[must_use]
pub fn nest(layout: Layout<'_>) -> Blocks<'_> {
    let tokens = layout.tokens().as_slice();
    assert!(
        tokens.len() <= usize_from_u32(TOKENS_MAX),
        "the lexer makes no more tokens than a source can hold"
    );
    // A node holds at least one token, and no source holds more than `NODES_MAX` nodes. So this
    // is room for every node the source can hold, made once, before any is found.
    let nodes_count_max = tokens.len().min(usize_from_u32(NODES_MAX));
    let mut nodes: Vec<Node> = Vec::with_capacity(nodes_count_max);
    let capacity_before = nodes.capacity();
    // The innermost node whose block is still open, if any, and how many blocks are open.
    let mut open_index: Option<u32> = None;
    let mut depth: u32 = 0;
    // The index of the first token on the line being nested.
    let mut line_token_index: usize = 0;
    while line_token_index < tokens.len() {
        let rest = &tokens[line_token_index..];
        let line = &rest[..count_on_first_line(rest)];
        if line[0].kind() == BraceClose {
            assert_eq!(line.len(), 1, "a `}}` is alone on its line");
            assert!(depth > 0, "the braces pair up, so a `}}` closes a block that's open");
            depth -= 1;
            let column = line[0].position().column();
            assert_eq!(column, column_at(depth), "a `}}` is indented like the line it closes");
            let opener_index =
                open_index.expect("a block is open, since the depth was more than none");
            let block = Block {
                end_node_index: u32_from_usize(nodes.len()),
                close_token_index: u32_from_usize(line_token_index),
            };
            open_index = nest_close_block(&mut nodes, opener_index, block);
        } else {
            assert_line(line, depth);
            let room = nodes.len() < nodes_count_max;
            assert!(room, "every node holds a token and is at least two bytes, so each has room");
            nodes.push(Node {
                first_token_index: u32_from_usize(line_token_index),
                end_token_index: u32_from_usize(line_token_index + line.len()),
                parent_index: open_index,
                block: None,
            });
            if line[line.len() - 1].kind() == BraceOpen {
                open_index = Some(u32_from_usize(nodes.len() - 1));
                depth += 1;
            }
        }
        assert_eq!(open_index.is_some(), depth > 0, "a node's block is open when any block is");
        assert!(depth <= NODES_MAX, "every block that's open was opened by a node");
        line_token_index += line.len();
    }
    assert_eq!(line_token_index, tokens.len(), "every token was nested");
    assert!(open_index.is_none(), "the braces pair up, so no block is left open");
    assert_eq!(depth, 0, "the braces pair up, so no block is left open");
    assert_eq!(nodes.capacity(), capacity_before, "the room made for the nodes never grows");
    assert_nested(&nodes, tokens);
    Blocks { layout, nodes }
}

/// Returns how many of `tokens` are on the first one's line, which is at least that one. The
/// tokens come in order, so they're the first few.
fn count_on_first_line(tokens: &[Token]) -> usize {
    assert!(!tokens.is_empty(), "a line is looked for where a token is");
    let line = tokens[0].position().line();
    let count = tokens.iter().take_while(|token| token.position().line() == line).count();
    assert!(count >= 1, "the first token is on its own line");
    if let Some(next) = tokens.get(count) {
        assert!(next.position().line() > line, "tokens come in the order they're in the source");
    }
    count
}

/// Asserts what the lexer and the tokens lint proved of `line`, the tokens of a line that isn't a
/// `}`'s, when `depth` blocks are open: it's indented a level for each, only its last token can
/// be a `{`, and a `{` doesn't start it, no `}` is on it, and a comment is alone on it.
fn assert_line(line: &[Token], depth: u32) {
    let (last, before) = line.split_last().expect("a line holds a token");
    let column = line[0].position().column();
    assert_eq!(column, column_at(depth), "a line is indented a level for each block it's in");
    for token in before {
        let goes_on = match token.kind() {
            Function | Returns | Name | TypeName | ParenthesisOpen | ParenthesisClose => true,
            BraceOpen | BraceClose | Comment => false,
        };
        assert!(goes_on, "a brace or a comment is the last token on its line");
    }
    assert_ne!(last.kind(), BraceClose, "a `}}` is alone on its line, and this line isn't one");
    if last.kind() == BraceOpen {
        assert!(!before.is_empty(), "a `{{` doesn't start its line");
    }
    if last.kind() == Comment {
        assert_eq!(line.len(), 1, "a comment is alone on its line");
    }
}

/// Closes the block the node at `opener_index` opens, as `block`, now that every node in it is in
/// `nodes`. Returns the innermost node whose block is still open, which is the opener's parent.
fn nest_close_block(nodes: &mut [Node], opener_index: u32, block: Block) -> Option<u32> {
    assert_eq!(
        usize_from_u32(block.end_node_index),
        nodes.len(),
        "a block is closed once its last node is in"
    );
    assert!(
        opener_index < block.end_node_index,
        "a block's nodes come after the node that opens it"
    );
    let node = &mut nodes[usize_from_u32(opener_index)];
    assert!(node.block.is_none(), "a block is closed once");
    assert!(block.close_token_index >= node.end_token_index, "a block closes after it opens");
    node.block = Some(block);
    if let Some(parent_index) = node.parent_index {
        assert!(parent_index < opener_index, "a node comes after its parent");
    }
    node.parent_index
}

/// Asserts that `nodes`, nested from `tokens`, hold together. Each node comes after its parent,
/// within its parent's block, after the node before it in the source, and opens a block exactly
/// when its last token is `{`. A block ends after the node that opens it and within the nodes,
/// and closes on a `}`.
fn assert_nested(nodes: &[Node], tokens: &[Token]) {
    assert!(nodes.len() <= usize_from_u32(NODES_MAX), "no source holds more than `NODES_MAX`");
    // The index of the token just past the node before, so the nodes come in order.
    let mut end_token_index_before: usize = 0;
    for (index, node) in nodes.iter().enumerate() {
        let index = u32_from_usize(index);
        let span = node.token_span();
        let in_order = span.start >= end_token_index_before;
        assert!(in_order, "the nodes come in the order they're in the source");
        end_token_index_before = span.end;
        assert!(span.end <= tokens.len(), "a node's tokens are among its source's");
        let ends_with_open = tokens[span.end - 1].kind() == BraceOpen;
        assert_eq!(node.block.is_some(), ends_with_open, "a node opens a block with a `{{`");
        if let Some(parent_index) = node.parent_index {
            assert!(parent_index < index, "a node comes after its parent");
            let enclosing = nodes[usize_from_u32(parent_index)].block;
            let enclosing = enclosing.expect("a parent opens a block");
            assert!(index < enclosing.end_node_index, "a node is within its parent's block");
        }
        if let Some(block) = node.block {
            assert!(block.end_node_index > index, "a block ends after the node that opens it");
            assert!(
                usize_from_u32(block.end_node_index) <= nodes.len(),
                "a block ends within the nodes"
            );
            let close = tokens[usize_from_u32(block.close_token_index)].kind();
            assert_eq!(close, BraceClose, "a block closes on a `}}`");
        }
    }
}

/// Returns the column of the first token on a line in `depth` blocks: one level, `LEVEL_SPACES`
/// spaces, for each.
fn column_at(depth: u32) -> u32 {
    assert!(depth <= NODES_MAX, "every block that's open was opened by a node");
    depth * LEVEL_SPACES + 1
}
