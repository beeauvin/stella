# Layout

Stella code has one layout, and the compiler refuses any other.

- A line is indented one level for each `{` still open on it. A line that
  starts with `}` counts its own brace as already closed. This holds for code
  and comments alike. Not tested yet.
- For now, a level is four spaces, and every line's indentation is a multiple
  of four. Tested by [`lines`](../../linter/text/src/text_tests.rs).
- A tab is refused anywhere in a source file. Tested by
  [`characters`](../../linter/text/src/text_tests.rs) and
  [`every_code_point_in_a_comment`](../../linter/text/src/text_tests.rs).
- A function's first line reads exactly `function main() returns Void {`, with
  single spaces as shown. So `function main ()` and `returns Void{` are
  refused. Not tested yet.
- `{` ends the line it's on, and `}` sits on a line of its own. That holds even
  for an empty body, so this is refused:

  ```stella
  function main() returns Void {}
  ```

  Not tested yet.
- No line ends in a space. Tested by
  [`lines`](../../linter/text/src/text_tests.rs).
- Blank lines may go anywhere, except at the end of the file. Tested by
  [`accepted_sources`](../../linter/text/src/text_tests.rs) and
  [`lines`](../../linter/text/src/text_tests.rs).
- The file ends with exactly one newline. Tested by
  [`lines`](../../linter/text/src/text_tests.rs).
