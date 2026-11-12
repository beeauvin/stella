// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Nesting cuts a layout into its lines, and puts each line in the block it's in.
//!
//! A line here is a line of the source that holds tokens: a comment, or a line of code other
//! than a `}`. The tokens lint has proved how a line of code looks, so it's one of two kinds:
//!
//! - **A line that opens a block** ends with `{`. The lines after it are in its block, up to the
//!   `}` that closes it.
//! - **A `}`** is alone on its line, and closes the innermost block still open. Its line isn't a
//!   line here: the block it closes records which token it is.
//!
//! A comment is a line that opens no block, and so, later, is any line of code that doesn't end
//! with a brace, once the layout lets one. A blank line holds no tokens, so it isn't a line here.
//!
//! The lines are kept flat, in the order they're in the source, in a `LineBuffer` made once.
//! Each line knows the line whose block it's in, and a line that opens a block knows where its
//! block ends. So a block's lines run from the one after the line that opens it to its end, and
//! stepping over a block is a jump. Nothing needs recursion, and nothing needs a stack.

use std::iter::FusedIterator;

use stella_lexer::TokenKind::{BraceClose, BraceOpen, Comment};
use stella_lexer::{TOKENS_MAX, Token, Tokens};
use stella_linter_text::LEVEL_SPACES;
use stella_linter_tokens::Layout;
use stella_source::SOURCE_BYTES_MAX;

/// This is a bound on how many lines a source can hold. Every line holds a token and ends with a
/// `\n`, so it's at least two bytes. No source reaches it today: a comment is at least three bytes,
/// and so is a line that opens a block, so the most is 5,461. But it holds for any line there
/// could be.
pub const LINES_MAX: u32 = SOURCE_BYTES_MAX / 2;

/// A line is one line of the source that holds tokens, other than a `}`'s, and where it sits among
/// the blocks. It holds where its tokens are rather than the tokens, so a buffer of lines can
/// serve one source after another. `Blocks::tokens_of` gives the tokens back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Line {
    /// The first is the index of the line's first token, in the layout's tokens.
    pub first: u32,
    /// The count is how many tokens the line holds, at least one. When the line opens a block,
    /// the last of them is its `{`.
    pub count: u32,
    /// The parent is the index, in the lines, of the line whose block this line is directly in, or
    /// `None` for a line in no block.
    pub parent: Option<u32>,
    /// The block is the one this line opens, when it ends with `{`.
    pub block: Option<Block>,
}

// A buffer holds `LINES_MAX` lines of 28 bytes each, about 224 KiB, and it's made once.
const _: () = assert!(size_of::<Line>() == 28);

/// A block is the lines between a `{` and the `}` that closes it. The line that ends with the `{`
/// opens it, so its lines start just after that one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Block {
    /// The end is the index, in the lines, just past the block's last line, counting the lines of
    /// the blocks inside it. An empty block ends just past the line that opens it.
    pub end: u32,
    /// The close is the index of the `}` that closes the block, in the layout's tokens.
    pub close: u32,
}

/// A line buffer is where `nest` puts the lines it finds. It's made once, with room for
/// `LINES_MAX` lines, and it never grows.
#[derive(Debug)]
pub struct LineBuffer {
    lines: Vec<Line>,
}

impl LineBuffer {
    /// Returns a buffer with room for `LINES_MAX` lines.
    pub fn new() -> LineBuffer {
        let lines = Vec::with_capacity(LINES_MAX as usize);
        assert!(lines.capacity() >= LINES_MAX as usize);
        LineBuffer { lines }
    }
}

impl Default for LineBuffer {
    fn default() -> LineBuffer {
        LineBuffer::new()
    }
}

/// The lines of a source laid out the one way, each in the block it's in, and the source's
/// tokens. Only `nest` makes them, so holding `Blocks` proves the source passed the text lint, the
/// lexer and the tokens lint.
#[derive(Clone, Copy, Debug)]
pub struct Blocks<'source, 'buffer> {
    tokens: Tokens<'source, 'buffer>,
    lines: &'buffer [Line],
}

impl<'source, 'buffer> Blocks<'source, 'buffer> {
    /// Returns the tokens, just as the lexer made them.
    pub fn tokens(&self) -> Tokens<'source, 'buffer> {
        self.tokens
    }

    /// Returns the lines, in the order they're in the source.
    pub fn lines(&self) -> &'buffer [Line] {
        self.lines
    }

    /// Returns `line`'s tokens, in order. The line must be one of these lines: one from another
    /// source, or made by hand, gives some other tokens, or panics.
    pub fn tokens_of(&self, line: &Line) -> &'buffer [Token] {
        let tokens = self.tokens.as_slice();
        let (first, count) = (line.first as usize, line.count as usize);
        assert!(count >= 1, "a line holds a token");
        assert!(first + count <= tokens.len(), "a line's tokens are among its source's");
        &tokens[first..first + count]
    }

    /// Returns the indices of the lines in no block, in order. A line that opens a block comes
    /// without the lines in it.
    pub fn top(&self) -> Children<'buffer> {
        Children { lines: self.lines, next: 0, end: index_of(self.lines.len()) }
    }

    /// Returns the indices of the lines directly in the block the line at `index` opens, in
    /// order. A line that opens a block of its own comes without the lines in it. Panics if there's
    /// no line at `index`, or if the line there opens no block.
    pub fn children(&self, index: u32) -> Children<'buffer> {
        assert!((index as usize) < self.lines.len(), "line {index} is one of the lines");
        let Some(block) = self.lines[index as usize].block else {
            panic!("line {index} opens no block, so it has no children");
        };
        assert!(block.end > index, "a block ends after the line that opens it");
        assert!(block.end as usize <= self.lines.len(), "a block ends within the lines");
        Children { lines: self.lines, next: index + 1, end: block.end }
    }
}

/// Children are the lines directly in one block, or in no block, as indices into the lines. A
/// line that opens a block comes without the lines in it, which are its own children. Once the
/// children are done, they stay done.
#[derive(Clone, Debug)]
pub struct Children<'buffer> {
    /// The lines are all of them, so the children can step over a block.
    lines: &'buffer [Line],
    /// The next is the index of the next child, when it's less than the end.
    next: u32,
    /// The end is the index just past the last line the children can come from.
    end: u32,
}

impl Iterator for Children<'_> {
    type Item = u32;

    /// Returns the next child's index, and steps over its block, if it opens one.
    fn next(&mut self) -> Option<u32> {
        assert!(self.end as usize <= self.lines.len());
        if self.next >= self.end {
            return None;
        }
        let index = self.next;
        // A line that opens a block comes without the lines in it, so the next child is the line
        // just past its block.
        self.next = self.lines[index as usize].block.map_or(index + 1, |block| block.end);
        assert!(self.next > index, "the children move forward");
        assert!(self.next <= self.end, "the children stay within their end");
        Some(index)
    }
}

impl FusedIterator for Children<'_> {}

/// Nests `layout`'s lines in their blocks, in `buffer`, and returns them. Whatever the buffer held
/// before is replaced. Nothing can be refused, since the tokens lint has proved everything this
/// relies on.
pub fn nest<'source, 'buffer>(
    layout: Layout<'source, 'buffer>,
    buffer: &'buffer mut LineBuffer,
) -> Blocks<'source, 'buffer> {
    let tokens = layout.tokens();
    let slice = tokens.as_slice();
    assert!(slice.len() <= TOKENS_MAX as usize);
    let capacity = buffer.lines.capacity();
    assert!(capacity >= LINES_MAX as usize);
    buffer.lines.clear();
    // The innermost line whose block is still open, if any, and how many blocks are open.
    let mut open: Option<u32> = None;
    let mut depth: u32 = 0;
    // The index of the first token on the line being nested.
    let mut start: usize = 0;
    while start < slice.len() {
        let line = &slice[start..start + on_its_line(&slice[start..])];
        if line[0].kind == BraceClose {
            assert_eq!(line.len(), 1, "a `}}` is alone on its line");
            assert!(depth > 0, "the braces pair up");
            depth -= 1;
            let indented = line[0].position.column() == column_at(depth);
            assert!(indented, "a `}}` is indented like the line whose block it closes");
            let opener = open.expect("a block is open");
            open = close(&mut buffer.lines, opener, index_of(start));
        } else {
            assert_line(line, depth);
            assert!(buffer.lines.len() < LINES_MAX as usize, "there's room for every line");
            let (first, count) = (index_of(start), index_of(line.len()));
            buffer.lines.push(Line { first, count, parent: open, block: None });
            if line[line.len() - 1].kind == BraceOpen {
                open = Some(index_of(buffer.lines.len() - 1));
                depth += 1;
            }
        }
        assert_eq!(open.is_some(), depth > 0);
        assert!(depth <= LINES_MAX);
        start += line.len();
    }
    assert_eq!(start, slice.len(), "every token was nested");
    assert!(open.is_none(), "the braces pair up");
    assert_eq!(depth, 0, "every block is closed");
    assert_eq!(buffer.lines.capacity(), capacity, "the buffer never grows");
    assert_nested(&buffer.lines, slice);
    Blocks { tokens, lines: &buffer.lines }
}

/// Returns how many of `tokens` are on the first one's line, which is at least that one. The
/// tokens come in order, so they're the first few.
fn on_its_line(tokens: &[Token]) -> usize {
    let line = tokens[0].position.line();
    let count = tokens.iter().take_while(|token| token.position.line() == line).count();
    assert!(count >= 1);
    assert!(
        tokens.get(count).is_none_or(|next| next.position.line() > line),
        "tokens come in order"
    );
    count
}

/// Asserts what the lexer and the tokens lint proved of `line`, the tokens of a line that isn't a
/// `}`'s, when `depth` blocks are open: it's indented a level for each, only its last token can
/// be a `{`, and a `{` doesn't start it, no `}` is on it, and a comment is alone on it.
fn assert_line(line: &[Token], depth: u32) {
    let (last, before) = line.split_last().expect("a line holds a token");
    let indented = line[0].position.column() == column_at(depth);
    assert!(indented, "a line is indented a level for each block it's in");
    for token in before {
        assert!(!matches!(token.kind, BraceOpen | BraceClose | Comment));
    }
    assert_ne!(last.kind, BraceClose);
    if last.kind == BraceOpen {
        assert!(!before.is_empty(), "a `{{` doesn't start its line");
    }
    if last.kind == Comment {
        assert_eq!(line.len(), 1, "a comment is alone on its line");
    }
}

/// Closes the block the line at `opener` opens, with the `}` at `close` in the tokens, now that
/// every line in the block is in `lines`. Returns the innermost line whose block is still open,
/// which is the opener's parent.
fn close(lines: &mut [Line], opener: u32, close: u32) -> Option<u32> {
    let end = index_of(lines.len());
    assert!(opener < end, "a block's lines come after the line that opens it");
    let line = &mut lines[opener as usize];
    assert!(line.block.is_none(), "a block is closed once");
    assert!(close >= line.first + line.count, "a block closes after it opens");
    line.block = Some(Block { end, close });
    assert!(line.parent.is_none_or(|parent| parent < opener), "a line comes after its parent");
    line.parent
}

/// Asserts that `lines`, nested from `tokens`, hold together. Each line comes after its parent,
/// within its parent's block, after the line before it in the source, and opens a block exactly
/// when its last token is `{`. A block ends
/// after the line that opens it and within the lines, and closes on a `}`.
fn assert_nested(lines: &[Line], tokens: &[Token]) {
    assert!(lines.len() <= LINES_MAX as usize);
    // The index of the token just past the line before, so the lines come in order.
    let mut after: usize = 0;
    for (index, line) in lines.iter().enumerate() {
        let index = index_of(index);
        let (first, count) = (line.first as usize, line.count as usize);
        assert!(count >= 1, "a line holds a token");
        assert!(first >= after, "the lines come in the order they're in the source");
        after = first + count;
        assert!(first + count <= tokens.len(), "a line's tokens are among its source's");
        assert_eq!(line.block.is_some(), tokens[first + count - 1].kind == BraceOpen);
        if let Some(parent) = line.parent {
            assert!(parent < index, "a line comes after its parent");
            let enclosing = lines[parent as usize].block.expect("a parent opens a block");
            assert!(index < enclosing.end, "a line is within its parent's block");
        }
        if let Some(block) = line.block {
            assert!(block.end > index, "a block ends after the line that opens it");
            assert!(block.end as usize <= lines.len(), "a block ends within the lines");
            assert_eq!(tokens[block.close as usize].kind, BraceClose);
        }
    }
}

/// Returns the column of the first token on a line in `depth` blocks: one level, `LEVEL_SPACES`
/// spaces, for each.
fn column_at(depth: u32) -> u32 {
    assert!(depth <= LINES_MAX);
    depth * LEVEL_SPACES + 1
}

/// Returns `index`, an index into a source's tokens or lines, or a count of them, which is within
/// the source, as a `u32`.
fn index_of(index: usize) -> u32 {
    assert!(index <= SOURCE_BYTES_MAX as usize);
    u32::try_from(index).expect("an index within a source fits in a `u32`")
}
