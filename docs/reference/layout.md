# Layout

Stella code has one layout, and the compiler refuses any other.

- A line is indented one level for each `{` still open on it. A line that
  starts with `}` counts its own brace as already closed. This holds for code
  and comments alike. Tested by
  [`indentation`](../../linter/tokens/tests/layout.rs) and
  [`every_indentation`](../../linter/tokens/tests/layout.rs).
- For now, a level is four spaces, and every line's indentation is a multiple
  of four. Tested by [`lines`](../../linter/text/tests/text.rs).
- A tab is refused anywhere in a source file. Tested by
  [`characters`](../../linter/text/tests/text.rs) and
  [`every_code_point_in_a_comment`](../../linter/text/tests/text.rs).
- On a line, exactly one space goes between two tokens, except that none goes
  after `(`, and none goes before `(` or `)`. So `main`'s first line has single
  spaces where they're shown, `function main() returns Void {`, and
  `function main ()` and `returns Void{` are refused. Every pair of tokens that
  can share a line is tried with one space and two, and with none unless that
  makes them one word, by [`every_gap`](../../linter/tokens/tests/layout.rs),
  and more by [`spaces`](../../linter/tokens/tests/layout.rs).
- A line of code ends with `{` or `}`. For now, nothing else can end one, so a
  function's first line is never split, and can't leave its `{` to the next
  line. So this is refused:

  ```stella
  function main() returns Void
  {
  }
  ```

  Tested by [`lines`](../../linter/tokens/tests/layout.rs).
- `{` can't start a line: some token comes before it on its line. Tested by
  [`lines`](../../linter/tokens/tests/layout.rs). Which tokens can come before
  it is the grammar's to say, once the layout has passed. For now, a line of
  code that ends with `{` is a [function](functions.md)'s first line, outside
  every block, and a function's body holds no code. Tested by
  [`every_line`](../../grammar/tests/parse.rs).
- `{` ends the line it's on, and `}` sits on a line of its own. That holds even
  for an empty body, so this is refused:

  ```stella
  function main() returns Void {}
  ```

  Tested by [`lines`](../../linter/tokens/tests/layout.rs).
- Every `}` closes a `{` before it, and every `{` is closed. Tested by
  [`braces`](../../linter/tokens/tests/layout.rs).
- No line ends in a space. Tested by
  [`lines`](../../linter/text/tests/text.rs).
- Blank lines may go anywhere, except at the end of the file. A line of only
  spaces isn't blank: it ends in a space, so it's refused. Tested by
  [`accepted_sources`](../../linter/text/tests/text.rs) and
  [`lines`](../../linter/text/tests/text.rs).
- The file ends with exactly one newline. Tested by
  [`lines`](../../linter/text/tests/text.rs).

Tabs, trailing spaces, blank lines, the final newline and four-space levels
are checked with the [source text](source-text.md). The rest is checked once
the [tokens](tokens.md) are, in three steps, and the first step that fails is
the one reported:

1. Where lines end and braces sit: `{` ends its line and doesn't start it, `}`
   has a line of its own, and a line of code ends with `{` or `}`.
2. Whether the braces pair up: every `}` closes a `{`, and every `{` is closed.
3. Each line's indentation and spaces, together.

Within a step, the earliest problem in the file is reported. A brace out of
place is reported at the brace, so `{}` is refused at its `{`, and a `{` that
starts its line without ending it is refused for not ending it. A line that
ends some other way is reported just past its last token. A brace never closed
is reported at the outermost one left open. Within a line, the problem is at
the first character out of place: the first space too many, a token that needs
a space before it, or for a line indented too little, its first token. For
example:

```
hello.stella:1:29: one space must go between a type's name and `{`.
    function main() returns Void{
                                ^
```

The steps' order is tested by
[`the_steps_run_in_order`](../../linter/tokens/tests/layout.rs), the earliest
problem by [`the_first_problem_wins`](../../linter/tokens/tests/layout.rs), the
largest sources by [`the_largest_sources`](../../linter/tokens/tests/layout.rs),
the widest lines by [`the_widest_lines`](../../linter/tokens/tests/layout.rs),
random programs by
[`fuzz_against_the_oracle`](../../linter/tokens/tests/layout.rs), the sentences
by [`sentences`](../../linter/tokens/tests/problem.rs), and the command by
[`code_in_another_layout`](../../cli/tests/lint.rs).
