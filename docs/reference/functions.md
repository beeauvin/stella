# Functions

A function is written in this order: the [keyword](keywords.md) `function`, its
name, `()`, the keyword `returns`, a [type](types.md), and then a body in
braces. Its first line holds all of that up to the `{`, and the compiler refuses
any other order, at the first token out of place. So `function main() {` is
refused at its `{`, since `returns` and the type must come first. Tested by
[`function_lines`](../../grammar/tests/parse.rs) and
[`every_line`](../../grammar/tests/parse.rs).

A first line that ends before its `{` is refused just past its last token, for
the first part it lacks. So a first line can't be split, and this is refused
at the end of its first line, since the return type must come after `returns`:

```stella
function main() returns
Void {
}
```

```
hello.stella:1:24: the function's return type must come after `returns`.
    function main() returns
                           ^
```

Tested by [`lines_that_end_early`](../../grammar/tests/parse.rs) and
[`every_line`](../../grammar/tests/parse.rs).

```stella
function main() returns Void {
}
```

A function returns a value of exactly one type. Tested by
[`function_lines`](../../grammar/tests/parse.rs).

For now:

- It takes no parameters, so its parentheses are empty. Tested by
  [`function_lines`](../../grammar/tests/parse.rs).
- Its body holds [comments](comments.md), blank lines, and at most one line of
  code: `return` and an integer, the value the function gives back. Any other
  line of code in it is refused at its first token. Tested by
  [`bodies`](../../grammar/tests/parse.rs) and
  [`return_lines`](../../grammar/tests/parse.rs).
- Nothing comes after the value on a `return` line, and no line of code comes
  after the `return`, since nothing after it could run. Comments can. Tested
  by [`return_lines`](../../grammar/tests/parse.rs) and
  [`every_body_line`](../../grammar/tests/parse.rs).
- Whether a function has a `return`, and whether its integer fits, depends on
  the [type](types.md) it returns: a `Void` function has none, and a `U8`
  function ends with one whose integer is from 0 to 255. Tested by
  [`void`](../../meaning/tests/fit.rs) and
  [`missing_returns`](../../meaning/tests/fit.rs).
- The only function is `main`, as [programs](programs.md) says. Tested by
  [`not_main`](../../meaning/tests/program.rs) and
  [`another_function`](../../meaning/tests/program.rs).
- It returns `Void` or `U8`, since those are the only [types](types.md).
  Tested by [`unknown_types`](../../meaning/tests/resolve.rs).
