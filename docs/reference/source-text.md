# Source text

A source file is UTF-8 text, at most 16,384 bytes long, and never empty. Outside
[comments](comments.md), it's ASCII. The size is tested by
[`size`](../../linter/text/src/text_tests.rs) and
[`accepted_sources`](../../linter/text/src/text_tests.rs).

Every code point is tried against the rules below, once in a comment and once
in code, by
[`every_code_point_in_a_comment`](../../linter/text/src/text_tests.rs) and
[`every_code_point_in_code`](../../linter/text/src/text_tests.rs).

- The compiler refuses a file that isn't valid UTF-8. Tested by
  [`invalid_utf8`](../../linter/text/src/text_tests.rs).
- `\n` ends a line, and nothing else does. `\r` is refused anywhere, and so are
  U+2028 and U+2029, the line and paragraph separators. Tested by
  [`characters`](../../linter/text/src/text_tests.rs) too.
- No control character may appear except `\n`. These are U+0000 to U+001F and
  U+007F to U+009F, so a tab is refused too. Tested by
  [`characters`](../../linter/text/src/text_tests.rs) too.
- No bidirectional control character may appear. These are Unicode's
  `Bidi_Control` characters: U+061C, U+200E, U+200F, U+202A to U+202E, and
  U+2066 to U+2069. They can make text display in a different order from the
  way the compiler reads it.
- U+FEFF, the byte-order mark, is refused anywhere, including at the start of
  the file. Tested by [`characters`](../../linter/text/src/text_tests.rs) too.
- Outside comments, every character is ASCII. Tested by
  [`ascii`](../../linter/text/src/text_tests.rs) too.
