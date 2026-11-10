# Layout

Stella code has one layout, and the compiler refuses any other.

- A line is indented one level for each `{` still open on it. A line that
  starts with `}` counts its own brace as already closed. This holds for code
  and comments alike. Not built yet.
- For now, a level is four spaces, and every line's indentation is a multiple
  of four. Tested by [`lines`](../../linter/text/tests/text.rs).
- A tab is refused anywhere in a source file. Tested by
  [`characters`](../../linter/text/tests/text.rs) and
  [`every_code_point_in_a_comment`](../../linter/text/tests/text.rs).
- A function's first line reads exactly `function main() returns Void {`, with
  single spaces as shown. So `function main ()` and `returns Void{` are
  refused. Not built yet.
- `{` ends the line it's on, and `}` sits on a line of its own. That holds even
  for an empty body, so this is refused:

  ```stella
  function main() returns Void {}
  ```

  Not built yet.
- No line ends in a space. Tested by
  [`lines`](../../linter/text/tests/text.rs).
- Blank lines may go anywhere, except at the end of the file. A line of only
  spaces isn't blank: it ends in a space, so it's refused. Tested by
  [`accepted_sources`](../../linter/text/tests/text.rs) and
  [`lines`](../../linter/text/tests/text.rs).
- The file ends with exactly one newline. Tested by
  [`lines`](../../linter/text/tests/text.rs).
