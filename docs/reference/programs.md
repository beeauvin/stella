# Programs

For now, a program is one source file, and its name ends in `.stella`. Tested
by [`accepted_paths`](../../linter/text/tests/path.rs) and
[`refused_paths`](../../linter/text/tests/path.rs).

It holds exactly one [function](functions.md), named `main`, and nothing else
but [comments](comments.md). The compiler refuses anything else. Not built
yet.

```stella
function main() returns Void {
}
```

Running the program runs `main`. When `main` returns, the program exits with
status 0. Not built yet.

[The compiler](compiler.md) says how a program is built.
