// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for the blocks. They're a crate of their own, so the compiler lets them reach only
//! what the crates make public: every source goes through the text lint's `lint`, the lexer's
//! `lex` and the tokens lint's `lint_tokens`, and then `nest`.
//!
//! `nest` can't refuse, so every source here is laid out the one way, and each row says what
//! nesting should make of it, as an outline: written by hand, or for the largest, by a formula
//! from how they're built. An oracle works out the whole table of lines from each line's text,
//! and every row checks the oracle against its outline before `nest` sees any. An outline names
//! lines by number, so `children` also states one whole table by hand, with its tokens' indices
//! and where its blocks end. A seeded fuzz test checks random laid-out programs against the
//! oracle. The largest sources sit at each limit a source of 16,384 bytes has, so nothing can be
//! counted in anything too small.

use std::cell::RefCell;
use std::panic::Location;
use std::ptr;

use stella_blocks::{Block, Blocks, Children, LINES_MAX, Line, LineBuffer, nest};
use stella_lexer::TokenKind::*;
use stella_lexer::{TokenBuffer, lex};
use stella_linter_text::{LEVEL_SPACES, lint};
use stella_linter_tokens::lint_tokens;
use stella_source::{Position, SOURCE_BYTES_MAX};

/// One line of an outline: the line's number in the source, how many tokens it holds, the number
/// of the line whose block it's in, or 0 for none, and the number of the line whose `}` closes
/// the block it opens, or 0 for none.
type Outlined = (u32, u32, u32, u32);

/// This is the smallest program, laid out the one way.
const SMALLEST: &str = "function main() returns Void {\n}\n";

/// Blocks nested three deep, then closed back out two levels at once to a block beside one of
/// them, then out to the top, with comments in blocks.
const CLOSES_BACK_OUT: &str = "a {\n    b {\n        c {\n            d {\n            }\n        \
                               }\n    }\n    e {\n    }\n    // f\n}\ng {\n    // h\n}\n";

thread_local! {
    /// Each test lexes and nests every source it tries into these buffers, as the command will.
    static BUFFERS: RefCell<(TokenBuffer, LineBuffer)> =
        RefCell::new((TokenBuffer::new(), LineBuffer::new()));
}

/// Says whether `source` passes the text lint, the lexer and the tokens lint, so it can be nested.
fn lays_out(source: &str) -> bool {
    let Ok(text) = lint(source.as_bytes()) else {
        return false;
    };
    BUFFERS.with_borrow_mut(|(tokens, _)| {
        lex(text, tokens).is_ok_and(|tokens| lint_tokens(tokens).is_ok())
    })
}

/// Nests `source`, which must be laid out the one way, and hands its blocks to `then`. The blocks
/// must hold the very tokens the lexer made.
fn with_blocks<T>(source: &str, then: impl FnOnce(Blocks<'_, '_>) -> T) -> T {
    let text = lint(source.as_bytes()).expect("source should pass the text lint");
    BUFFERS.with_borrow_mut(|(token_buffer, line_buffer)| {
        let tokens = lex(text, token_buffer);
        let tokens = tokens.unwrap_or_else(|problem| panic!("should lex: {problem:?}"));
        let layout = lint_tokens(tokens);
        let layout = layout.unwrap_or_else(|problem| panic!("should lay out: {problem:?}"));
        let blocks = nest(layout, line_buffer);
        let (given, back) = (tokens.as_slice(), blocks.tokens().as_slice());
        assert!(ptr::eq(given, back), "the blocks hold the tokens the lexer made");
        then(blocks)
    })
}

/// What nesting makes of a source, as the tests compare it: its lines, the lines `top` yields,
/// and for each line, the lines `children` yields when it opens a block.
#[derive(Debug, PartialEq, Eq)]
struct Nested {
    lines: Vec<Line>,
    top: Vec<u32>,
    children: Vec<Option<Vec<u32>>>,
}

/// Returns `index`, an index into a source's tokens or lines, as a `u32`.
fn index_of(index: usize) -> u32 {
    u32::try_from(index).expect("an index within a source")
}

/// Nests `source`, which must be laid out the one way, and returns what `nest` made of it. On the
/// way, it checks that each line's tokens are its own, and that every token is in exactly one
/// line, or is exactly one block's `}`.
fn checked(source: &str) -> Nested {
    with_blocks(source, |blocks| {
        let (tokens, lines) = (blocks.tokens().as_slice(), blocks.lines());
        let mut seen = vec![0_u32; tokens.len()];
        for line in lines {
            let (first, count) = (line.first as usize, line.count as usize);
            let own = &tokens[first..first + count];
            assert!(ptr::eq(blocks.tokens_of(line), own), "a line's tokens are its own");
            seen[first..first + count].iter_mut().for_each(|each| *each += 1);
            if let Some(block) = line.block {
                seen[block.close as usize] += 1;
            }
        }
        let once = seen.iter().all(|&each| each == 1);
        assert!(once, "every token is in exactly one line, or is exactly one block's `}}`");
        let children = (0..lines.len())
            .map(|index| {
                let opens = lines[index].block.is_some();
                opens.then(|| yielded(blocks.children(index_of(index)), lines.len()))
            })
            .collect();
        Nested { lines: lines.to_vec(), top: yielded(blocks.top(), lines.len()), children }
    })
}

/// Returns what `children` yield, but no more than one more than the `lines` there are, so
/// children that never stop make a test fail rather than hang. Children that are done must stay
/// done.
fn yielded(mut children: Children<'_>, lines: usize) -> Vec<u32> {
    let found: Vec<u32> = children.by_ref().take(lines + 1).collect();
    if found.len() <= lines {
        assert_eq!(children.next(), None, "children that are done stay done");
    }
    found
}

/// The examples a test tries. `check` tries the oracle on every one before `nest` sees any, so
/// every example checks the oracle, even before `nest` can run.
#[derive(Default)]
struct Examples {
    examples: Vec<Example>,
}

/// An example is a source, its outline, and the line of the test that added it, so a failure
/// points at the example rather than at `check`.
struct Example {
    source: String,
    outline: Vec<Outlined>,
    at: &'static Location<'static>,
}

impl Examples {
    /// Adds `source`, which must be laid out the one way, and nest as `outline` says.
    #[track_caller]
    fn nests(&mut self, source: &str, outline: &[Outlined]) {
        let (source, outline) = (String::from(source), outline.to_vec());
        self.examples.push(Example { source, outline, at: Location::caller() });
    }

    /// Checks that every example is laid out the one way, as `nest` needs, and that the oracle
    /// agrees with its outline. Then checks every one against `nest`.
    fn check(mut self) {
        let examples = std::mem::take(&mut self.examples);
        assert!(!examples.is_empty(), "a test tries at least one example");
        let mut expected = Vec::new();
        for Example { source, outline, at } in &examples {
            let shown = shown(source);
            assert!(lays_out(source), "{at}: {shown} should be laid out the one way");
            let oracle = oracle(source);
            assert_eq!(oracle.outline, *outline, "{at}: the oracle's outline, for {shown}");
            expected.push(oracle.nested);
        }
        for (Example { source, at, .. }, nested) in examples.iter().zip(expected) {
            assert_eq!(checked(source), nested, "{at}: for {}", shown(source));
        }
    }
}

/// Returns `source` as a failure shows it: whole when it's short, or else its start and its
/// length, since the largest sources are 16,384 bytes.
fn shown(source: &str) -> String {
    if source.len() <= 120 {
        format!("{source:?}")
    } else {
        let start: String = source.chars().take(60).collect();
        format!("{start:?}, and on to {} bytes", source.len())
    }
}

impl Drop for Examples {
    /// Every example added is checked, so a test can't pass by forgetting to. A test that has
    /// already failed is left to report its own failure.
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(self.examples.is_empty(), "examples were added but never checked");
        }
    }
}

/// Returns the spaces that indent a line `levels` levels.
fn indent(levels: u32) -> String {
    " ".repeat((levels * LEVEL_SPACES) as usize)
}

/// Returns `lines` as a source, each ending in `\n`.
fn source_of(lines: &[String]) -> String {
    lines.iter().map(|line| format!("{line}\n")).collect()
}

/// Returns the lines of blocks nested `depth` deep, each opened by `({` and closed by its own `}`,
/// all laid out the one way. A `{` can't start a line, so `({` is the shortest line that opens a
/// block.
fn nested_blocks(depth: u32) -> Vec<String> {
    let mut lines: Vec<String> = (0..depth).map(|level| format!("{}({{", indent(level))).collect();
    lines.extend((0..depth).rev().map(|level| format!("{}}}", indent(level))));
    lines
}

/// Returns the outline of `nested_blocks(depth)`: each line that opens a block is in the one
/// before it, and is closed by the `}` as far from the end as it is from the start.
fn nested_outline(depth: u32) -> Vec<Outlined> {
    (1..=depth).map(|line| (line, 2, line - 1, 2 * depth + 1 - line)).collect()
}

/// Returns the outline of `count` comments, one on each line from line 1, in no block.
fn comments_outline(count: u32) -> Vec<Outlined> {
    (1..=count).map(|line| (line, 1, 0, 0)).collect()
}

/// Returns the outline of `count` empty blocks, one after another from line 1, each opened by a
/// line of two tokens and closed on the next line.
fn blocks_outline(count: u32) -> Vec<Outlined> {
    (0..count).map(|block| (2 * block + 1, 2, 0, 2 * block + 2)).collect()
}

#[test]
fn the_smallest_program() {
    let mut examples = Examples::default();
    examples.nests(SMALLEST, &[(1, 7, 0, 2)]);
    // Comments are lines, in the block they're in. Blank lines hold no tokens, so they aren't.
    examples.nests(
        "// above\n\nfunction main() returns Void {\n    // inside\n\n    //\n}\n\n// below\n",
        &[(1, 1, 0, 0), (3, 7, 0, 7), (4, 1, 3, 0), (6, 1, 3, 0), (9, 1, 0, 0)],
    );
    examples.check();
}

#[test]
fn comments() {
    let mut examples = Examples::default();
    // A source of only comments is lines in no block.
    examples.nests("// only a comment\n", &[(1, 1, 0, 0)]);
    examples.nests("//\n\n// and another\n", &[(1, 1, 0, 0), (3, 1, 0, 0)]);
    // A comment at every depth is in the block it's in, before, between and after the blocks
    // there.
    let every_depth = [
        "// zero",
        "a {",
        "    // one",
        "    b {",
        "        // two",
        "        c {",
        "            // three",
        "        }",
        "        // two again",
        "    }",
        "    // one again",
        "}",
        "// zero again",
    ];
    let every_depth: Vec<String> = every_depth.iter().map(|line| String::from(*line)).collect();
    examples.nests(
        &source_of(&every_depth),
        &[
            (1, 1, 0, 0),
            (2, 2, 0, 12),
            (3, 1, 2, 0),
            (4, 2, 2, 10),
            (5, 1, 4, 0),
            (6, 2, 4, 8),
            (7, 1, 6, 0),
            (9, 1, 4, 0),
            (11, 1, 2, 0),
            (13, 1, 0, 0),
        ],
    );
    // A comment that reads like a brace opens and closes nothing.
    examples.nests(
        "a {\n    // }\n    // b {\n}\n// {\n",
        &[(1, 2, 0, 4), (2, 1, 1, 0), (3, 1, 1, 0), (5, 1, 0, 0)],
    );
    examples.check();
}

#[test]
fn siblings() {
    let mut examples = Examples::default();
    // Blocks one after another at the top, with and without blank lines between them.
    examples.nests("a {\n}\nb {\n}\nc {\n}\n", &blocks_outline(3));
    examples.nests("a {\n}\n\n\nb {\n}\n", &[(1, 2, 0, 2), (5, 2, 0, 6)]);
    // Blocks one after another in a block.
    examples.nests(
        "a {\n    b {\n    }\n    c {\n    }\n    d {\n    }\n}\n",
        &[(1, 2, 0, 8), (2, 2, 1, 3), (4, 2, 1, 5), (6, 2, 1, 7)],
    );
    // Blocks in two blocks side by side stay in their own.
    examples.nests(
        "a {\n    b {\n    }\n}\nc {\n    d {\n    }\n}\n",
        &[(1, 2, 0, 4), (2, 2, 1, 3), (5, 2, 0, 8), (6, 2, 5, 7)],
    );
    examples.check();
}

#[test]
fn nesting() {
    let mut examples = Examples::default();
    examples.nests(&source_of(&nested_blocks(1)), &nested_outline(1));
    examples.nests(&source_of(&nested_blocks(5)), &nested_outline(5));
    // Out of a deep block two levels at once, to a block beside one of them, then to the top.
    examples.nests(
        CLOSES_BACK_OUT,
        &[
            (1, 2, 0, 11),
            (2, 2, 1, 7),
            (3, 2, 2, 6),
            (4, 2, 3, 5),
            (8, 2, 1, 9),
            (10, 1, 1, 0),
            (12, 2, 0, 14),
            (13, 1, 12, 0),
        ],
    );
    // Out of the deepest block straight to the top.
    examples.nests(
        "a {\n    b {\n        c {\n        }\n    }\n}\nd {\n}\n",
        &[(1, 2, 0, 6), (2, 2, 1, 5), (3, 2, 2, 4), (7, 2, 0, 8)],
    );
    examples.check();
}

#[test]
fn heads() {
    let mut examples = Examples::default();
    // Every kind of token that can come before a `{`, in one line, and long lines in blocks.
    let every_kind = "x Y function returns(z)() {\n}\n";
    examples.nests(every_kind, &[(1, 10, 0, 2)]);
    examples.nests("a b c d e f g h {\n    ((((({\n    }\n}\n", &[(1, 9, 0, 4), (2, 6, 1, 3)]);
    examples.check();
    // A line's tokens end with its `{`, and its block's close is the `}` that closes it.
    with_blocks(every_kind, |blocks| {
        let line = blocks.lines()[0];
        let kinds: Vec<_> = blocks.tokens_of(&line).iter().map(|token| token.kind).collect();
        let every = [Name, TypeName, Function, Returns, ParenOpen, Name, ParenClose, ParenOpen];
        assert_eq!(kinds, [&every[..], &[ParenClose, BraceOpen]].concat());
        let close = blocks.tokens().as_slice()[line.block.expect("a block").close as usize];
        assert_eq!((close.kind, close.position), (BraceClose, Position::new(2, 1)));
    });
}

#[test]
fn children() {
    // The table of lines and their children, worked out by hand. The tokens are numbered in
    // order: `a` is 0 and its `{` is 1, up to the last `}`, which is 19.
    let line = |first, count, parent, block| Line { first, count, parent, block };
    let block = |end, close| Some(Block { end, close });
    let table = [
        line(0, 2, None, block(6, 15)),
        line(2, 2, Some(0), block(4, 10)),
        line(4, 2, Some(1), block(4, 9)),
        line(6, 2, Some(2), block(4, 8)),
        line(11, 2, Some(0), block(5, 13)),
        line(14, 1, Some(0), None),
        line(16, 2, None, block(8, 19)),
        line(18, 1, Some(6), None),
    ];
    assert_eq!(oracle(CLOSES_BACK_OUT).nested.lines, table, "the oracle's table");
    with_blocks(CLOSES_BACK_OUT, |blocks| {
        assert_eq!(blocks.lines(), table);
        // The children step over the blocks inside them, and an empty block has none.
        let children = |index| yielded(blocks.children(index), table.len());
        assert_eq!(yielded(blocks.top(), table.len()), [0, 6]);
        assert_eq!(children(0), [1, 4, 5]);
        assert_eq!(children(1), [2]);
        assert_eq!(children(2), [3]);
        assert_eq!(children(3), []);
        assert_eq!(children(4), []);
        assert_eq!(children(6), [7]);
    });
}

#[test]
#[should_panic(expected = "opens no block")]
fn children_of_a_line_that_opens_no_block() {
    // Asking for the children of a line that opens no block is a bug, caught when it's asked.
    with_blocks("// a\n", |blocks| {
        let _ = blocks.children(0);
    });
}

#[test]
fn the_largest_sources() {
    // Each limit goes as far as the largest source lets it, and one more doesn't fit. So nothing
    // nesting counts can be counted in anything smaller.
    let mut examples = Examples::default();
    let size = SOURCE_BYTES_MAX as usize;
    // The deepest nesting, and the deepest line: a comment in its innermost block, 63 deep.
    assert!(source_of(&nested_blocks(63)).len() <= size);
    assert!(source_of(&nested_blocks(64)).len() > size);
    examples.nests(&source_of(&nested_blocks(63)), &nested_outline(63));
    let mut deepest = nested_blocks(63);
    deepest.insert(63, format!("{}//", indent(63)));
    assert!(source_of(&deepest).len() <= size);
    let mut outline: Vec<Outlined> = (1..=63).map(|line| (line, 2, line - 1, 128 - line)).collect();
    outline.push((64, 1, 63, 0));
    examples.nests(&source_of(&deepest), &outline);
    // The most lines: a comment on every line. The buffer has room for a line in every two
    // bytes, so lines of code that open no block, once there are any, will fit too.
    assert!("//\n".repeat(5_461).len() <= size);
    assert!("//\n".repeat(5_462).len() > size);
    const { assert!(LINES_MAX == SOURCE_BYTES_MAX / 2, "room for a line in every two bytes") };
    examples.nests(&"//\n".repeat(5_461), &comments_outline(5_461));
    // Lines 256 apart, which a byte can't tell from lines side by side.
    let apart = format!("//\n{}//\n", "\n".repeat(255));
    examples.nests(&apart, &[(1, 1, 0, 0), (257, 1, 0, 0)]);
    examples.nests(&format!("({{\n{}}}\n", "\n".repeat(255)), &[(1, 2, 0, 257)]);
    // The most blocks, one after another.
    assert!("({\n}\n".repeat(3_276).len() <= size);
    assert!("({\n}\n".repeat(3_277).len() > size);
    examples.nests(&"({\n}\n".repeat(3_276), &blocks_outline(3_276));
    // The most lines that hold tokens, `}` lines too: the most blocks, and a comment.
    let most_with_tokens = format!("{}//\n", "({\n}\n".repeat(3_276));
    assert!(most_with_tokens.len() <= size);
    assert!(format!("{most_with_tokens}//\n").len() > size);
    let mut outline = blocks_outline(3_276);
    outline.push((6_553, 1, 0, 0));
    examples.nests(&most_with_tokens, &outline);
    // The most lines in one block.
    let in_one_block = |count: usize| format!("({{\n{}}}\n", "    //\n".repeat(count));
    assert!(in_one_block(2_339).len() <= size);
    assert!(in_one_block(2_340).len() > size);
    let mut outline = vec![(1, 2, 0, 2_341)];
    outline.extend((2..=2_340).map(|line| (line, 1, 1, 0)));
    examples.nests(&in_one_block(2_339), &outline);
    // The most tokens on one line: a `(` in every byte but the last four.
    let widest = |opens: usize| format!("{}{{\n}}\n", "(".repeat(opens));
    assert!(widest(size - 4).len() <= size);
    assert!(widest(size - 3).len() > size);
    examples.nests(&widest(size - 4), &[(1, 16_381, 0, 2)]);
    // The last lines, after the most blank lines: a comment, and a block.
    let last_comment = |blanks: usize| format!("{}//\n", "\n".repeat(blanks));
    assert!(last_comment(size - 3).len() <= size);
    assert!(last_comment(size - 2).len() > size);
    examples.nests(&last_comment(size - 3), &[(16_382, 1, 0, 0)]);
    let last_block = |blanks: usize| format!("{}({{\n}}\n", "\n".repeat(blanks));
    assert!(last_block(size - 5).len() <= size);
    assert!(last_block(size - 4).len() > size);
    examples.nests(&last_block(size - 5), &[(16_380, 2, 0, 16_381)]);
    examples.check();
}

#[test]
fn one_buffer_serves_every_source() {
    // The same buffers serve every source in turn, as they will in the command, so each source's
    // lines replace the last one's, however many more or fewer there were.
    let mut examples = Examples::default();
    let most_lines = "//\n".repeat(5_461);
    examples.nests(&most_lines, &comments_outline(5_461));
    examples.nests(SMALLEST, &[(1, 7, 0, 2)]);
    examples.nests(&"({\n}\n".repeat(3_276), &blocks_outline(3_276));
    examples.nests("// x\n", &[(1, 1, 0, 0)]);
    examples.nests(&most_lines, &comments_outline(5_461));
    examples.check();
    // And they're always in the same room, since a buffer is made once and never grows. These
    // buffers are new, so nothing has had a chance to grow them before the smallest source.
    let (mut token_buffer, mut line_buffer) = (TokenBuffer::new(), LineBuffer::new());
    let most_blocks = "({\n}\n".repeat(3_276);
    let mut room = None;
    for source in [SMALLEST, &most_lines, "// x\n", &most_blocks, SMALLEST, &most_lines] {
        let text = lint(source.as_bytes()).expect("source should pass the text lint");
        let tokens = lex(text, &mut token_buffer).expect("source should lex");
        let layout = lint_tokens(tokens).expect("source should lay out");
        let here = nest(layout, &mut line_buffer).lines().as_ptr().addr();
        assert_eq!(*room.get_or_insert(here), here, "the lines are in the same room");
    }
}

// The oracle, which works from each line's text.

/// What the oracle makes of a source: what nesting should make of it, and its outline.
struct Expected {
    nested: Nested,
    outline: Vec<Outlined>,
}

/// Returns how many tokens are in `code`, a line of code without its indentation: each of `(`,
/// `)`, `{` and `}` is one, a run of letters, digits and `_` is one, and spaces are between them.
fn tokens_in(code: &str) -> u32 {
    let mut count = 0;
    let mut in_word = false;
    for byte in code.bytes() {
        let word = byte.is_ascii_alphanumeric() || byte == b'_';
        if (word && !in_word) || matches!(byte, b'(' | b')' | b'{' | b'}') {
            count += 1;
        }
        in_word = word;
    }
    count
}

/// Returns what nesting should make of `source`, which is laid out the one way, worked out from
/// each line's text. A blank line isn't a line. A comment is one token, and a line of code is as
/// many as `tokens_in` counts. A line's depth is its indentation, a level for each block it's
/// in, and it's in the block opened by the latest line that ends with `{` one level out. A `}`
/// isn't a line: it's at the depth of the line whose block it closes, and that block ends with
/// the lines before it.
fn oracle(source: &str) -> Expected {
    let (mut lines, mut outline) = (Vec::<Line>::new(), Vec::<Outlined>::new());
    // The lines that open the blocks still open, outermost first, so one for each level.
    let mut open: Vec<usize> = Vec::new();
    let mut token: u32 = 0;
    for (content, number) in source.split_terminator('\n').zip(1_u32..) {
        let code = content.trim_start_matches(' ');
        if code.is_empty() {
            continue;
        }
        let depth = (content.len() - code.len()) / LEVEL_SPACES as usize;
        let is_comment = code.starts_with("//");
        let count = if is_comment { 1 } else { tokens_in(code) };
        if code == "}" {
            assert_eq!(open.len(), depth + 1, "a `}}` is at the depth of the block it closes");
            let opener = open.pop().expect("a block is open");
            lines[opener].block = Some(Block { end: index_of(lines.len()), close: token });
            outline[opener].3 = number;
        } else {
            assert_eq!(open.len(), depth, "a line is in every block still open");
            let parent = open.last().copied();
            lines.push(Line { first: token, count, parent: parent.map(index_of), block: None });
            outline.push((number, count, parent.map_or(0, |parent| outline[parent].0), 0));
            if code.ends_with('{') && !is_comment {
                open.push(lines.len() - 1);
            }
        }
        token += count;
    }
    assert!(open.is_empty(), "every block is closed");
    let top = children_by_parent(&lines, None);
    let children = (0..lines.len())
        .map(|index| {
            let opens = lines[index].block.is_some();
            opens.then(|| children_by_parent(&lines, Some(index_of(index))))
        })
        .collect();
    Expected { nested: Nested { lines, top, children }, outline }
}

/// Returns the indices of the lines in `lines` whose parent is `parent`, in order.
fn children_by_parent(lines: &[Line], parent: Option<u32>) -> Vec<u32> {
    (0..lines.len()).filter(|&index| lines[index].parent == parent).map(index_of).collect()
}

// Fuzzing against the oracle, seeded so a failing case replays.

/// This is a small deterministic generator, xorshift64*, as the lexer's tests use.
struct Rng {
    state: u64,
}

impl Rng {
    /// Returns the next number in the sequence.
    fn next(&mut self) -> u64 {
        self.state ^= self.state >> 12;
        self.state ^= self.state << 25;
        self.state ^= self.state >> 27;
        self.state.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Returns a number from 0 up to, but not including, `count`.
    fn below(&mut self, count: usize) -> usize {
        assert!(count > 0);
        usize::try_from(self.next() % count as u64).expect("below a usize")
    }

    /// Returns one of `choices`.
    fn pick<'a>(&mut self, choices: &[&'a str]) -> &'a str {
        choices[self.below(choices.len())]
    }
}

/// These are what a random line that opens a block is made of, before its `{`.
const PIECES: &[&str] = &["function", "returns", "main", "Void", "x", "MyType", "(", ")"];

/// These follow a comment's `//`. Some read like braces, which open and close nothing.
const COMMENT_TEXTS: &[&str] = &["", " a", " café", " { } ( )", " }", " function main() {"];

/// Returns `texts`, the tokens of a line of code, written the one way: one space between two
/// tokens, except none after `(`, and none before `(` or `)`.
fn joined(texts: &[&str]) -> String {
    let mut line = String::new();
    for (index, text) in texts.iter().enumerate() {
        if index > 0 && texts[index - 1] != "(" && !matches!(*text, "(" | ")") {
            line.push(' ');
        }
        line.push_str(text);
    }
    line
}

/// Returns a random program laid out the one way, as its lines: blank lines, comments, and blocks
/// nested up to eight deep, each opened by a few random tokens and `{`, and closed by `}`.
fn random_program(rng: &mut Rng) -> Vec<String> {
    let mut lines = Vec::new();
    let mut depth: u32 = 0;
    for _ in 0..=rng.below(40) {
        match rng.below(8) {
            0 => lines.push(String::new()),
            1 | 2 => lines.push(format!("{}//{}", indent(depth), rng.pick(COMMENT_TEXTS))),
            3 | 4 if depth > 0 => {
                depth -= 1;
                lines.push(format!("{}}}", indent(depth)));
            }
            _ if depth < 8 => {
                let heads = 1 + rng.below(4);
                let mut texts: Vec<&str> = (0..heads).map(|_| rng.pick(PIECES)).collect();
                texts.push("{");
                lines.push(format!("{}{}", indent(depth), joined(&texts)));
                depth += 1;
            }
            _ => lines.push(format!("{}// deep", indent(depth))),
        }
    }
    lines.extend((0..depth).rev().map(|level| format!("{}}}", indent(level))));
    // A source can't end with a blank line, or be empty.
    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    if lines.is_empty() {
        lines.push(String::from("// empty"));
    }
    lines
}

/// The shapes the fuzzing must reach often, as counts of the sources that reach each.
#[derive(Debug, Default)]
struct Reached {
    deep: u32,
    many_at_top: u32,
    empty_blocks: u32,
    comments_in_blocks: u32,
    blank_lines: u32,
}

impl Reached {
    /// Counts the shapes `source`, which nests as `lines`, reaches.
    fn count(&mut self, source: &str, lines: &[Line]) {
        let mut depths: Vec<u32> = Vec::new();
        for line in lines {
            depths.push(line.parent.map_or(0, |parent| depths[parent as usize] + 1));
        }
        let opens_at_top =
            lines.iter().filter(|line| line.parent.is_none() && line.block.is_some());
        let is_empty = |(index, line): (usize, &Line)| {
            line.block.is_some_and(|block| block.end == index_of(index) + 1)
        };
        self.deep += u32::from(depths.iter().any(|&depth| depth >= 6));
        self.many_at_top += u32::from(opens_at_top.count() >= 3);
        self.empty_blocks += u32::from(lines.iter().enumerate().any(is_empty));
        let in_block = |line: &Line| line.parent.is_some() && line.block.is_none();
        self.comments_in_blocks += u32::from(lines.iter().any(in_block));
        self.blank_lines += u32::from(source.starts_with('\n') || source.contains("\n\n"));
    }
}

#[test]
fn fuzz_against_the_oracle() {
    let mut rng = Rng { state: 0x424C_4F43_4B53_0001 };
    let mut reached = Reached::default();
    let mut checks = Vec::new();
    for _ in 0..20_000 {
        let source = source_of(&random_program(&mut rng));
        assert!(lays_out(&source), "the generator lays out {}", shown(&source));
        let expected = oracle(&source).nested;
        reached.count(&source, &expected.lines);
        checks.push((source, expected));
    }
    // The fuzzing only means something if the sources reach the shapes that matter, often. This
    // is checked before anything is nested, so it holds whatever `nest` does. Each floor is about
    // half of what the seed reaches.
    let Reached { deep, many_at_top, empty_blocks, comments_in_blocks, blank_lines } = reached;
    for (shape, count, floor) in [
        ("nesting six deep", deep, 3_000),
        ("three blocks at the top", many_at_top, 2_500),
        ("an empty block", empty_blocks, 9_000),
        ("a comment in a block", comments_in_blocks, 8_500),
        ("a blank line", blank_lines, 8_000),
    ] {
        assert!(count > floor, "only {count} reached {shape}");
    }
    // The oracle made these answers, so each goes straight to `nest`.
    for (source, expected) in checks {
        assert_eq!(checked(&source), expected, "for {}", shown(&source));
    }
}
