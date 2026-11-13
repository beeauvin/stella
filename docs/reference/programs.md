# Programs

For now, a program is one source file, and its name ends in `.stella`. Tested
by [`accepted_paths`](../../linter/path/tests/path.rs) and
[`refused_paths`](../../linter/path/tests/path.rs).

It holds exactly one [function](functions.md), named `main`, and nothing else
but [comments](comments.md) and blank lines. Outside every function, the
compiler refuses any line of code that doesn't start one. Tested by
[`function_lines`](../../grammar/tests/parse.rs).

A file with no function has no `main`, so it's refused as a whole, even when it
holds comments. Tested by [`no_main`](../../meaning/tests/program.rs). Once
every function's type exists, a first function with any other name is refused
at its name, and a function after `main` at its `function`. Tested by
[`not_main`](../../meaning/tests/program.rs),
[`another_function`](../../meaning/tests/program.rs) and
[`every_program`](../../meaning/tests/program.rs).

```stella
function main() returns Void {
}
```

Running the program runs `main`. When `main` returns, the program exits with
status 0. Not built yet.

[The compiler](compiler.md) says how a program is built.
