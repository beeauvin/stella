# Functions

A function is written in this order: the [keyword](keywords.md) `function`, its
name, `()`, the keyword `returns`, a [type](types.md), and then a body in
braces. Its first line holds all of that up to the `{`, and the compiler refuses
any other order, at the first token out of place. So `function main() {` is
refused at its `{`, since `returns` and the type must come first. Tested by
[`function_lines`](../../grammar/tests/parse.rs) and
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
- Its body is empty, apart from any [comments](comments.md) and blank lines: a
  line of code in it is refused at its first token. Tested by
  [`bodies`](../../grammar/tests/parse.rs).
- The only function is `main`, as [programs](programs.md) says. Tested by
  [`not_main`](../../meaning/tests/program.rs) and
  [`another_function`](../../meaning/tests/program.rs).
- It returns `Void`, since that's the only [type](types.md). Tested by
  [`unknown_types`](../../meaning/tests/resolve.rs).
