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
tested by [`accepted_paths`](../../linter/path/tests/path.rs). `stella build`
itself isn't built yet.

The compiler refuses a path it can't build from: an absolute path, or one with
`..` as a part. It also refuses a file whose name doesn't end in `.stella`.
Every name in the path, each directory's and the file's before `.stella`, is
lowercase letters and digits joined by single hyphens, like `hello-world` or
`rung-0`. One leading `./` is fine, so `./programs/hello.stella` means the same
as `programs/hello.stella`. Anywhere else, `.` or an empty part, as in
`programs//hello.stella`, is refused. Tested by
[`every_code_point_in_a_files_name`](../../linter/path/tests/path.rs),
[`every_code_point_in_a_directorys_name`](../../linter/path/tests/path.rs),
[`accepted_paths`](../../linter/path/tests/path.rs),
[`refused_paths`](../../linter/path/tests/path.rs) and
[`fuzz_against_the_oracle`](../../linter/path/tests/path.rs).

Then the path is checked again, as it's stored. macOS ignores case, so
`hello.stella` can name a file stored as `Hello.stella`, and that file is
refused for its name, as `Hello.stella`. Before that, no part of the path may
be a symlink, just as linting every file never follows one. Tested by
[`names_are_checked_as_stored`](../../cli/tests/lint.rs),
[`named_symlinks_are_refused`](../../cli/tests/lint.rs),
[`a_name_stored_in_another_case_comes_back_as_stored`](../../cli/tests/stored.rs),
[`a_name_that_folds_past_ascii_comes_back_as_stored`](../../cli/tests/stored.rs)
and [`a_symlink_anywhere_in_a_path_is_refused`](../../cli/tests/stored.rs).

To find each name as it's stored, the compiler looks for it by name among the
entries of its directory, through no more than 10,000 entries in all. So a path
may cross onto another volume, and a file with more than one name, through hard
links, is checked by the name it was given. A path swapped for a symlink while
the compiler is checking it can still be followed. Tested by
[`a_named_path_is_bounded`](../../cli/tests/lint.rs),
[`a_path_through_a_mount_point_is_found`](../../cli/tests/stored.rs) and
[`a_file_with_two_names_comes_back_by_the_one_typed`](../../cli/tests/stored.rs).

Source can't be piped in, so `-` is refused like any option. Tested by
[`misuse`](../../cli/tests/lint.rs).

For now, the compiler builds for one target, macOS 27, and there's no target to
choose. Not built yet.

## Linting

`stella lint` checks programs without building them. It runs a build's checks,
in the same order, through the [layout](layout.md). It doesn't check the code
itself yet. It prints nothing for a program that passes.

```
$ stella lint programs/hello.stella
$ stella lint
```

Given a path, it checks that one program. A path through a symlink is refused,
as above. A path that isn't UTF-8 is refused, since a name that isn't UTF-8
can't be lowercase kebab. Tested by [`one_path`](../../cli/tests/lint.rs),
[`one_path_sizes`](../../cli/tests/lint.rs),
[`named_symlinks_are_refused`](../../cli/tests/lint.rs) and
[`a_path_that_isnt_utf8`](../../cli/tests/lint.rs).

Given no path, it checks every `.stella` file under the directory it runs in.
Each refused file gets its first problem, with a blank line between messages,
in the byte order of the files' paths. A directory it can't read gets a message
of its own, and every other file is still checked. Tested by
[`every_program_in_a_tree`](../../cli/tests/lint.rs),
[`every_program_passes`](../../cli/tests/lint.rs),
[`no_programs`](../../cli/tests/lint.rs) and
[`unreadable_directories`](../../cli/tests/lint.rs).

It skips anything whose name starts with `.`, such as `.git` and `.build`. It
also skips directories named `target`: the compiler is written in Rust, which
builds into `target`, and no Stella source goes there. It never goes into what
it skips, and it doesn't follow symlinks. It looks at no more than 10,000 files
and directories. Tested by
[`every_program_in_a_tree`](../../cli/tests/lint.rs),
[`symlinks_are_skipped`](../../cli/tests/lint.rs) and
[`the_walk_is_bounded`](../../cli/tests/lint.rs).

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

None of the report is built yet.

## Exit status

The compiler exits with:

- **0:** the program was built, or for `stella lint`, every program it checked
  passed.
- **1:** a program was refused, because of its file name or path, its size,
  its text or its code.
- **2:** something outside the program went wrong. It's one of these:
  - The command was misused. It also prints a short usage line.
  - A file doesn't exist, isn't a regular file, or can't be read, even because
    a directory on the way to it can't be searched or read.
  - A directory can't be read.
  - There was nothing to lint.
  - There were more than 10,000 entries to look through, under the directory
    or on the way to a file.
  - The binary couldn't be written, or the linker failed.
  - Stderr couldn't be written, as when a pipe closes early, stderr is open
    only for reading, or a pipe that doesn't wait for its reader fills up.

A closed stderr isn't one of those. Rust opens `/dev/null` in its place before
the compiler starts, so what's written there is lost, and the status stands.

When `stella lint` checks every program, it exits with the worst status any of
them gets. The statuses are tested by
[`each_status_has_its_exit_code`](../../cli/tests/status.rs) and
[`the_worse_of_two_statuses_is_the_greater`](../../cli/tests/status.rs). For
`stella lint`, this is tested by
[`one_path`](../../cli/tests/lint.rs), [`not_a_file`](../../cli/tests/lint.rs),
[`an_unreadable_file`](../../cli/tests/lint.rs),
[`unreadable_paths`](../../cli/tests/lint.rs),
[`unreadable_directories`](../../cli/tests/lint.rs),
[`the_worst_status_wins`](../../cli/tests/lint.rs),
[`code_that_cant_be_lexed`](../../cli/tests/lint.rs),
[`code_in_another_layout`](../../cli/tests/lint.rs),
[`no_programs`](../../cli/tests/lint.rs),
[`the_walk_is_bounded`](../../cli/tests/lint.rs),
[`a_named_path_is_bounded`](../../cli/tests/lint.rs),
[`stderr_that_cant_be_written`](../../cli/tests/lint.rs),
[`a_message_that_cant_be_written_is_an_error`](../../cli/tests/run.rs),
[`a_closed_stderr`](../../cli/tests/lint.rs),
[`misuse`](../../cli/tests/lint.rs) and
[`misuses`](../../cli/tests/arguments.rs). `stella build` isn't built yet.

## Refusals

The compiler checks a program in a fixed order, and each check leaves the next
one less to worry about:

1. The path: it's relative, and no part of it is `..`, empty, or a `.` other
   than one leading `./`. Then its directories are named the one way, and its
   file ends in `.stella`, with a name before it that's named the one way.
   Then no part of the path may be a symlink. Last come the same checks again,
   for the path as it's stored.
2. The size: at least one byte, and at most 16,384.
3. The characters: everything on the [source text](source-text.md) page but
   the ASCII rule, in one pass.
4. The lines: exactly one final newline with no blank line before it, no
   trailing spaces, and indentation in multiples of four.
5. [Comments](comments.md), including that code and a comment can't share a
   line.
6. ASCII: outside comments, every character is ASCII.
7. The [tokens](tokens.md): in code, every character is in a token or is a
   space, and every name has its one [shape](names.md).
8. The [layout](layout.md): where lines end and braces sit, whether the braces
   pair up, and then each line's indentation and spaces.
9. The code itself.

The order of the path's checks is tested by
[`refused_paths`](../../linter/path/tests/path.rs), and the order of steps 2 to 6
by [`the_steps_run_in_order`](../../linter/text/tests/text.rs). That the tokens
come after them is tested by
[`code_that_cant_be_lexed`](../../cli/tests/lint.rs), and that the layout comes
after the tokens by [`code_in_another_layout`](../../cli/tests/lint.rs). The
code's own checks aren't built yet.

For now, when the compiler refuses a program, it stops at the first check that
fails. Within that check, it reports the earliest problem in the file. The
layout is checked in [three steps](layout.md) of its own, so there it reports
the earliest problem in the first step that fails. That's tested by
[`characters`](../../linter/text/tests/text.rs),
[`invalid_utf8`](../../linter/text/tests/text.rs),
[`lines`](../../linter/text/tests/text.rs),
[`comments`](../../linter/text/tests/text.rs),
[`ascii`](../../linter/text/tests/text.rs), the lexer's
[`the_first_problem_wins`](../../lexer/tests/lex.rs), and the tokens lint's
[`the_first_problem_wins`](../../linter/tokens/tests/layout.rs) and
[`the_steps_run_in_order`](../../linter/tokens/tests/layout.rs). It writes one
message to stderr.
For example:

```
hello.stella:1:32: a comment can't share a line with code.
    function main() returns Void { // hi
                                   ^
```

The message starts with the file, line and column, so editors and terminals can
jump to it. Then one sentence says what's wrong. Last comes the line itself,
with a caret under the column. Lines and columns count from 1, and a column
counts code points, not bytes. A problem with the file as a whole, such as its
name or its size, is one line: the file, then the sentence.

```
Hello.stella: a source file's name, before `.stella`, must be lowercase letters and digits joined by single hyphens, like `hello-world`.
```

The path and the line are shown safely. Every character outside printable
ASCII (U+0020 to U+007E) is shown as an escape: `\t`, `\r`, or `\u{E9}` for any
other, and a byte that isn't UTF-8 is shown as `\xFF`. So nothing unsafe
reaches the terminal, and the caret stays exactly under the problem. Tested by
[`every_code_point_is_shown_safely`](../../cli/tests/message.rs),
[`every_byte_that_isnt_utf8_is_shown_safely`](../../cli/tests/message.rs)
and [`the_caret_lands_after_every_code_point`](../../cli/tests/message.rs).

Every sentence is tested by the path lint's
[`sentences`](../../linter/path/tests/problem.rs), the text lint's
[`sentences`](../../linter/text/tests/problem.rs), the lexer's
[`sentences`](../../lexer/tests/problem.rs) and the tokens lint's
[`sentences`](../../linter/tokens/tests/problem.rs), and every line and column
by the tests above. The message as a whole is tested by
[`whole_file_messages`](../../cli/tests/message.rs) and
[`messages`](../../cli/tests/message.rs).
