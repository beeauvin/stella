# The compiler

## Building

There is one way to build a program: from the directory you're working in,
point the compiler at the program's `.stella` file.

```
$ stella build programs/hello.stella
hello needs 0 bytes of running memory and 44 bytes of code, on top of what macOS needs to run any program.
```

The binary goes in `.build/`, at the same path as its source, without
`.stella`. So `programs/hello.stella` builds to `.build/programs/hello`, and
`.build/` and the directories in it are made as they're needed. The path and
the name it's built from are tested by
[`accepted_paths`](../../linter/path/tests/path.rs), and the build by
[`builds_a_program`](../../cli/tests/build.rs),
[`the_longest_name`](../../cli/tests/build.rs) and
[`a_binary_is_saved_at_its_sources_path_without_the_suffix`](../../cli/tests/save.rs).

If something other than a regular file is already there, such as a directory,
a symlink or a FIFO, the build stops without touching it. A symlink as `.build`
or as a directory in it is never gone through, and neither is a file in the way
of a directory. So a binary and a directory can want the same place:
`programs/hello.stella` builds to `.build/programs/hello`, and
`programs/hello/world.stella` needs that path as a directory. Whichever is
built second stops, and what the first built is left as it was. Tested by
[`something_else_at_the_binarys_path`](../../cli/tests/build.rs),
[`symlinks_on_the_way`](../../cli/tests/build.rs),
[`a_binary_and_a_directory_in_one_place`](../../cli/tests/build.rs),
[`something_on_the_way_that_isnt_a_directory`](../../cli/tests/save.rs) and
[`something_at_the_binarys_path_that_isnt_a_file`](../../cli/tests/save.rs).

A program that's refused gets the same message `stella lint` gives it, and
nothing is written: an old binary stays exactly as it was. Tested by
[`a_refused_program_touches_nothing`](../../cli/tests/build.rs) and
[`sources_that_cant_be_built`](../../cli/tests/build.rs).

The compiler refuses a path it can't build from: an absolute path, or one with
`..` as a part. It also refuses a file whose name doesn't end in `.stella`.
Every name in the path, each directory's and the file's before `.stella`, is
lowercase letters and digits joined by single hyphens, like `hello-world` or
`step-0`. One leading `./` is fine, so `./programs/hello.stella` means the same
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
links, is checked by the name it was given. The one exception is a name that
isn't ASCII: if the file has another such name, the message may show either,
and the file is refused both ways. A path swapped for a symlink while the
compiler is checking it can still be followed. Tested by
[`a_named_path_is_bounded`](../../cli/tests/lint.rs),
[`a_path_through_a_mount_point_is_found`](../../cli/tests/stored.rs) and
[`a_file_with_two_names_comes_back_by_the_one_typed`](../../cli/tests/stored.rs).

Source can't be piped in, so `-` is refused like any option. `stella build`
takes exactly one path. Tested by [`misuse`](../../cli/tests/lint.rs),
[`misuse`](../../cli/tests/build.rs) and
[`misuses`](../../cli/tests/arguments.rs).

For now, the compiler builds for one target, macOS 15.7.5 or later on arm64,
and there's no target to choose. 15.7.5 is the oldest macOS the binaries have
been seen to run on, and CI runs them on it, on 26 and on 27. Tested by
[`otool_reads_what_the_oracle_states`](../../macho/tests/write.rs), which reads
macOS 15.7.5 back from the binary,
[`the_smallest_program`](../../macho/tests/write.rs), which pins the header
that says arm64, and
[`macos_checks_the_signature_and_runs_it`](../../macho/tests/write.rs), which
runs it.

## The binary

The compiler writes the binary itself, every byte of it, with no assembler and
no linker: the machine code, and the Mach-O file around it that macOS runs.

The code starts with a switch to Stella's own stack. macOS gives the program a
stack of its own, where the loader, the program's arguments and its
environment live. Stella's code runs on a stack in the binary instead, sized
exactly by the compiler, so what the program needs doesn't depend on who
starts it. The switch keeps macOS's stack pointer and return address in two
registers Stella's code never uses, points the stack pointer at Stella's
stack, and calls `main`. When `main` returns, the switch puts both back and
returns the program's exit status. When `main` returns `Void`, the switch sets
it to 0. When `main` returns `U8`, `main` has already put its value where a
result goes, and the switch leaves it there, so the program exits with that
value. Nothing is written to memory to do any of it. Tested by
[`the_smallest_program`](../../arm64/tests/generate.rs) and
[`main_returns_a_u8`](../../arm64/tests/generate.rs), against the system's
assembler, and
[`the_linker_finds_the_stack_the_same_way`](../../arm64/tests/generate.rs),
against its linker. Every instruction's word is tested against the assembler
too, by
[`the_assembler_makes_the_same_word_of_every_edge`](../../arm64/tests/instruction.rs)
and
[`the_assembler_makes_the_same_word_of_random_instructions`](../../arm64/tests/instruction.rs).

Stella's stack sits at the bottom of its segment, just above the segment that
holds the code, which can't be written. So a push past the stack's end would
land there and stop the program, and never write over anything. Tested by
[`the_code_finds_the_stack`](../../macho/tests/write.rs),
[`every_stack_edge`](../../macho/tests/layout.rs) and
[`otool_reads_what_the_oracle_states`](../../macho/tests/write.rs), which reads
back that the code's segment may be read and run, never written.

The binary is signed, as macOS requires, with a hash of each 4 KiB page before
the signature and no certificate, in the program's name: its file's, without
`.stella`. `codesign` accepts the signature, and can
sign the binary again, which is how you'd sign it with a certificate of your
own. The same program with the same name always builds to the same bytes.
Tested by
[`the_smallest_program`](../../macho/tests/write.rs),
[`the_same_bytes_every_time`](../../macho/tests/write.rs),
[`the_binary_is_signed_in_its_sources_name`](../../cli/tests/build.rs),
[`macos_checks_the_signature_and_runs_it`](../../macho/tests/write.rs),
[`codesign_signs_it_again`](../../macho/tests/write.rs),
[`otool_reads_what_the_oracle_states`](../../macho/tests/write.rs) and
[`shasum_gives_every_hash_and_the_uuid`](../../macho/tests/write.rs).

The binary is written beside its place first, to a part of the build's own,
and then renamed over it. So a binary is never rewritten in place, which macOS
can refuse to run, and nobody ever sees half of one. It's for anyone to read
and run, and for its owner to write, less what the umask takes away. Tested by
[`rebuilding`](../../cli/tests/build.rs),
[`builds_at_once`](../../cli/tests/build.rs),
[`the_mode_follows_the_umask`](../../cli/tests/build.rs) and
[`saving_again_puts_a_new_file_in_place`](../../cli/tests/save.rs).

A part is named `.<process>.<count>.part`: the number of the process that's
building, and a count from 0, as in `.build/programs/.4321.0.part`. No binary's
name starts with a dot, so a part is never taken for one. A build makes its
part only where nothing is, at the first count that's free, and never reads,
writes or removes one it didn't make. So builds at once never share a part,
even builds of one program: every one of them builds. Tested by
[`builds_at_once`](../../cli/tests/build.rs),
[`saves_at_once_in_one_process_each_keep_their_own_part`](../../cli/tests/save.rs),
[`what_is_where_a_part_would_go`](../../cli/tests/build.rs),
[`whatever_is_where_a_part_would_go_is_left_alone`](../../cli/tests/save.rs)
and [`another_builds_part_is_left_alone`](../../cli/tests/save.rs).

If the binary can't be written whole, or can't be put in place, the build
removes its part, and the binary that was there stays as it was. Once a
directory holds 64 parts with one process's number, a build by a process with
that number stops there, and they can be removed when no build is running.
Tested by
[`a_binary_that_cant_be_written_whole`](../../cli/tests/build.rs),
[`a_binary_that_cant_be_replaced`](../../cli/tests/build.rs),
[`too_many_parts`](../../cli/tests/build.rs),
[`too_many_parts_stop_a_save`](../../cli/tests/save.rs) and
[`a_failure_leaves_the_binary_that_was_there`](../../cli/tests/save.rs).

A build that's killed part-way leaves its part behind, and nothing reads or
removes it: the next build makes its own beside it. No test kills a build, so
this one isn't tested.

Each thing in `.build/` is looked at just before it's used. Something swapped
in between the look and the use, by whoever else can write in `.build/`, can
still be followed or replaced.

## Linting

`stella lint` checks programs without building them. It runs every check a
build runs, in the same order, so a program that passes is one a build would
build. It prints nothing for a program that passes. Tested by
[`a_refused_program_touches_nothing`](../../cli/tests/build.rs), which gives
both commands one program for each check a source goes through once it's read:
its text, its tokens, its layout, its grammar, its types and its program. The
path and the size are given to both by
[`sources_that_cant_be_built`](../../cli/tests/build.rs), and a program that
passes by [`builds_a_program`](../../cli/tests/build.rs).

```
$ stella lint programs/hello.stella
$ stella lint
```

Given a path, it checks that one program. A path through a symlink is refused,
as above. A path that isn't UTF-8 is always refused, by whichever check meets
the bad bytes first, since no name that isn't UTF-8 is lowercase kebab. Tested
by [`one_path`](../../cli/tests/lint.rs),
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
builds into `target`, and no Stella source goes there. A source named by its
path under `target` is still checked. It never goes into what it skips, and it
doesn't follow symlinks. It looks at no more than 10,000 files
and directories. Tested by
[`every_program_in_a_tree`](../../cli/tests/lint.rs),
[`symlinks_are_skipped`](../../cli/tests/lint.rs) and
[`the_walk_is_bounded`](../../cli/tests/lint.rs).

## The memory report

When a build succeeds, the compiler prints what the program needs, on stdout.
It gives two numbers:

- **Running memory** is the memory the program writes while it runs. For now,
  that's 0 bytes, because `main` keeps nothing: what it gives back goes in a
  register.
- **Code** is the machine code Stella generates. For now, that's 11
  instructions, 44 bytes, whatever `main` returns. When it returns `Void`,
  ten are the switch to Stella's stack and back, and one is `main`, which
  returns. When it returns `U8`, the switch is nine, since it doesn't set the
  exit status, and `main` is two: one to set its value, and one to return.
  Tested by [`the_smallest_program`](../../arm64/tests/generate.rs) and
  [`main_returns_a_u8`](../../arm64/tests/generate.rs).

Neither counts what macOS needs to run any program: the loader, libSystem,
the program's arguments and environment, and the file format that wraps the
code.

The report names the program after its file, without `.stella`. It goes to
stdout, last, once the binary is in place, and only when the build succeeds.
Nothing else is ever written to stdout. Tested by
[`builds_a_program`](../../cli/tests/build.rs),
[`the_report_names_the_program`](../../cli/tests/build.rs),
[`a_report_that_cant_be_written`](../../cli/tests/build.rs),
[`a_build_reports_on_stdout_and_says_the_rest_on_stderr`](../../cli/tests/run.rs)
and [`a_report_is_flushed_before_the_run_ends`](../../cli/tests/run.rs).

## Exit status

The compiler exits with:

- **0:** the program was built, or for `stella lint`, every program it checked
  passed.
- **1:** a program was refused, because of its file name or path, its size,
  its text or its code.
- **2:** something outside the program went wrong. It's one of these:
  - The command was misused. It also prints how each command is used.
  - A file doesn't exist, isn't a regular file, or can't be read, even because
    a directory on the way to it can't be searched or read.
  - A directory can't be read.
  - There was nothing to lint.
  - There were more than 10,000 entries to look through, under the directory
    or on the way to a file.
  - The binary couldn't be written, or something in `.build/` was in its way.
  - The report couldn't be written to stdout. The binary is built by then, and
    stderr says why.
  - Stderr couldn't be written, as when a pipe closes early, stderr is open
    only for reading, or a stderr that doesn't wait for its reader fills up.

A closed stderr isn't one of those, and neither is a closed stdout. Rust opens
`/dev/null` in the place of each before the compiler starts, so what's written
there is lost, and the status stands.

When `stella lint` checks every program, it exits with the worst status any of
them gets. The statuses are tested by
[`each_status_has_its_exit_code`](../../cli/tests/status.rs) and
[`the_worse_of_two_statuses_is_the_greater`](../../cli/tests/status.rs). For
`stella lint`, this is tested by
[`one_path`](../../cli/tests/lint.rs),
[`one_path_sizes`](../../cli/tests/lint.rs),
[`named_symlinks_are_refused`](../../cli/tests/lint.rs),
[`not_a_file`](../../cli/tests/lint.rs),
[`an_unreadable_file`](../../cli/tests/lint.rs),
[`unreadable_paths`](../../cli/tests/lint.rs),
[`unreadable_directories`](../../cli/tests/lint.rs),
[`the_worst_status_wins`](../../cli/tests/lint.rs),
[`code_that_cant_be_lexed`](../../cli/tests/lint.rs),
[`code_in_another_layout`](../../cli/tests/lint.rs),
[`code_that_breaks_the_grammar`](../../cli/tests/lint.rs),
[`code_that_isnt_a_program`](../../cli/tests/lint.rs),
[`no_programs`](../../cli/tests/lint.rs),
[`the_walk_is_bounded`](../../cli/tests/lint.rs),
[`a_named_path_is_bounded`](../../cli/tests/lint.rs),
[`stderr_that_cant_be_written`](../../cli/tests/lint.rs),
[`a_message_that_cant_be_written_is_an_error`](../../cli/tests/run.rs),
[`a_closed_stderr`](../../cli/tests/lint.rs) and
[`misuse`](../../cli/tests/lint.rs). Which arguments are a misuse is tested by
[`misuses`](../../cli/tests/arguments.rs).

For `stella build`, it's tested by
[`builds_a_program`](../../cli/tests/build.rs),
[`a_refused_program_touches_nothing`](../../cli/tests/build.rs),
[`sources_that_cant_be_built`](../../cli/tests/build.rs),
[`a_binary_that_cant_be_written`](../../cli/tests/build.rs),
[`a_binary_that_cant_be_written_whole`](../../cli/tests/build.rs),
[`a_report_that_cant_be_written`](../../cli/tests/build.rs),
[`a_report_that_cant_be_written_is_a_failure_that_says_so`](../../cli/tests/run.rs),
[`a_message_that_cant_be_written_is_an_error`](../../cli/tests/run.rs),
[`a_binary_that_cant_be_replaced`](../../cli/tests/build.rs),
[`something_else_at_the_binarys_path`](../../cli/tests/build.rs),
[`too_many_parts`](../../cli/tests/build.rs),
[`symlinks_on_the_way`](../../cli/tests/build.rs) and
[`misuse`](../../cli/tests/build.rs).

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
8. The [layout](layout.md): where braces sit, whether the braces pair up, and
   then each line's indentation and spaces.
9. The grammar: each line's tokens come in the order the block it's in
   allows, and the line ends where its rule does, as
   [functions](functions.md) and [programs](programs.md) say.
10. The [types](types.md): every type a function returns exists.
11. The fit: what each function's body gives back fits the type it returns,
    as [types](types.md) says.
12. The [program](programs.md): there's exactly one function, and it's named
    `main`.

The order of the checks on the path as it's given is tested by
[`refused_paths`](../../linter/path/tests/path.rs), and that a symlink is
refused before a stored name by
[`named_symlinks_are_refused`](../../cli/tests/lint.rs). The order of steps 2
to 6 is tested by [`the_steps_run_in_order`](../../linter/text/tests/text.rs).
That the tokens come after the text is tested by
[`code_that_cant_be_lexed`](../../cli/tests/lint.rs), with a problem from step
4, and that the layout comes
after the tokens by [`code_in_another_layout`](../../cli/tests/lint.rs). That
the grammar comes after the layout is tested by
[`code_that_breaks_the_grammar`](../../cli/tests/lint.rs), that the types
come after the grammar, and the program after the types, by
[`code_that_isnt_a_program`](../../cli/tests/lint.rs), and that the fit comes
after the types and before the program by
[`code_that_doesnt_fit_its_type`](../../cli/tests/lint.rs).

For now, when the compiler refuses a program, it stops at the first check that
fails. Within that check, it reports the earliest problem in the file. The
layout is checked in [three steps](layout.md) of its own, so there it reports
the earliest problem in the first step that fails. The grammar reports the
earliest line at fault, at its first token out of place, and the types and
the program report the earliest function at fault. A file with no function is
refused as a whole, since there's no function in it to point at. That's tested
by
[`characters`](../../linter/text/tests/text.rs),
[`invalid_utf8`](../../linter/text/tests/text.rs),
[`lines`](../../linter/text/tests/text.rs),
[`comments`](../../linter/text/tests/text.rs),
[`ascii`](../../linter/text/tests/text.rs), the lexer's
[`the_first_problem_wins`](../../lexer/tests/lex.rs), and the tokens lint's
[`the_first_problem_wins`](../../linter/tokens/tests/layout.rs) and
[`the_steps_run_in_order`](../../linter/tokens/tests/layout.rs), the
grammar's [`the_first_problem_wins`](../../grammar/tests/parse.rs), the types'
[`the_first_problem_wins`](../../meaning/tests/resolve.rs) and the program's
[`the_first_problem_wins`](../../meaning/tests/program.rs) and
[`no_main`](../../meaning/tests/program.rs). It writes one message to stderr.
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
name, its size or a missing `main`, is one line: the file, then the sentence.

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
[`sentences`](../../lexer/tests/problem.rs), the tokens lint's
[`sentences`](../../linter/tokens/tests/problem.rs), the grammar's
[`sentences`](../../grammar/tests/problem.rs) and the meaning checks'
[`sentences`](../../meaning/tests/problem.rs), and every line and column
by the tests above. The message as a whole is tested by
[`whole_file_messages`](../../cli/tests/message.rs) and
[`messages`](../../cli/tests/message.rs).
