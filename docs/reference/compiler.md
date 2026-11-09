# The compiler

## Building

There is one way to build a program: from the directory you're working in,
point the compiler at the program's `.stella` file.

```
$ stella build programs/hello.stella
hello needs 0 bytes of running memory and 8 bytes of code, on top of what macOS needs to run any program.
```

The binary goes in `.build/`, at the same path as its source, without
`.stella`. So `programs/hello.stella` builds to `.build/programs/hello`. If
something other than a regular file is already there, such as a directory, the
build stops without touching it. The path and the name it's built from are
tested by [`accepted_paths`](../../linter/text/src/path_tests.rs). The building itself isn't tested yet.

The compiler refuses a path it can't build from: an absolute path, or one with
`..` as a part. It also refuses a file whose name doesn't end in `.stella`.
Every name in the path, each directory's and the file's before `.stella`, is
lowercase letters and digits joined by single hyphens, like `hello-world` or
`rung-0`. One leading `./` is fine, so `./programs/hello.stella` means the same
as `programs/hello.stella`. Anywhere else, `.` or an empty part, as in
`programs//hello.stella`, is refused. Tested by
[`every_code_point_in_a_files_name`](../../linter/text/src/path_tests.rs),
[`every_code_point_in_a_directorys_name`](../../linter/text/src/path_tests.rs),
[`accepted_paths`](../../linter/text/src/path_tests.rs), [`refused_paths`](../../linter/text/src/path_tests.rs) and
[`fuzz_against_the_oracle`](../../linter/text/src/path_tests.rs).

Source can't be piped in. Not tested yet.

For now, the compiler builds for one target, macOS 27, and there's no target to
choose. Not tested yet.

## The memory report

When a build succeeds, the compiler prints what the program needs, on stdout.
It gives two numbers:

- **Running memory** is the memory the program writes while it runs. For now,
  that's 0 bytes, because `main` does nothing.
- **Code** is the machine code Stella generates. For now, that's two
  instructions: 8 bytes.

Neither counts what macOS needs to run any program: the loader, libSystem,
the program's arguments and environment, and the file format that wraps the
code.

None of the report is tested yet.

## Exit status

The compiler exits with:

- **0:** the program was built.
- **1:** the program was refused, because of its file name or path, its size,
  its text or its code.
- **2:** something outside the program went wrong. The command was misused, the
  file doesn't exist or can't be read, the binary couldn't be written, or the
  linker failed. A misused command also prints a short usage line.

None of this is tested yet.

## Refusals

The compiler checks a program in a fixed order, and each check leaves the next
one less to worry about:

1. The path: it's relative, and no part of it is `..`, empty, or a `.` other
   than one leading `./`. Then its directories are named the one way. Last, its
   file ends in `.stella`, with a name before it that's named the one way.
2. The size: at least one byte, and at most 16,384.
3. The characters: everything on the [source text](source-text.md) page but
   the ASCII rule, in one pass.
4. The lines: exactly one final newline with no blank line before it, no
   trailing spaces, and indentation in multiples of four.
5. [Comments](comments.md), including that code and a comment can't share a
   line.
6. ASCII: outside comments, every character is ASCII.
7. The code itself.

The order of the path's checks is tested by [`refused_paths`](../../linter/text/src/path_tests.rs), and the
order of steps 2 to 6 by [`the_steps_run_in_order`](../../linter/text/src/text_tests.rs). The code's own checks
aren't tested yet.

For now, when the compiler refuses a program, it stops at the first check that
fails. Within that check, it reports the earliest problem in the file. That's
tested by [`characters`](../../linter/text/src/text_tests.rs), [`invalid_utf8`](../../linter/text/src/text_tests.rs), [`lines`](../../linter/text/src/text_tests.rs),
[`comments`](../../linter/text/src/text_tests.rs) and [`ascii`](../../linter/text/src/text_tests.rs). It writes one message to stderr. For example:

```
hello.stella:1:29: a space must come before `{`.
    function main() returns Void{
                                ^
```

The message starts with the file, line and column, so editors and terminals can
jump to it. Then one sentence says what's wrong. Last comes the line itself,
with a caret under the column. Lines and columns count from 1, and a column
counts code points, not bytes.

Every sentence is tested by [`sentences`](../../linter/text/src/problem_tests.rs), and every line and column by the
text tests above. The message as a whole isn't tested yet.
