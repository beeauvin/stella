# Stella

![license](https://img.shields.io/github/license/voxstella/stella)

Stella is a small, correct systems language for software that knows its limits.

Programs are built from components that speak only in messages, wired into
constellations. Every byte of memory, every queue and every loop has a bound the
compiler proves before anything runs. If it can't fit, it won't build. If it
builds, it tells you exactly what it needs. Period.

## Where it stands

Stella is at its very start. The compiler can't build a program yet. What it
can do is lint one: `stella lint` checks a source file's path, its text, its
tokens, its layout, its grammar and what its code means, and says exactly
where the first thing wrong is.

```console
$ stella lint app/bad-indent.stella
app/bad-indent.stella:2:1: indentation must be a multiple of four spaces.
      }
    ^
```

The [reference](docs/reference/readme.md) describes the language as it stands
today. Each rule in it links to the tests that check it, or says it isn't built
yet.

## Trying it

Stella is written in Rust, for macOS. [mise](https://mise.jdx.dev) installs the
Rust it's pinned to:

```sh
mise install
cargo build --release
```

With mise activated in your shell, `cargo` is that Rust. If it isn't activated,
put `mise exec --` before each `cargo` command here.

Then lint one program, or every program under the directory you're in:

```sh
target/release/stella lint hello.stella
target/release/stella lint
```

It exits with 0 when every program passes, 1 when one is refused, and 2 when
something outside the program went wrong, like a file that doesn't exist.

## Working on it

One command runs everything that must pass before anything merges, and CI runs
the same one:

```sh
cargo xtask ci
```

It checks the formatting, runs Clippy with warnings denied, runs the tests,
builds the documentation, and then proves that the lints bite: it compiles a
small fixture for each lint the workspace claims to enforce, and fails unless
that lint is what's reported.

The lints are strict on purpose, and they're declared once, in the root
`Cargo.toml` and `clippy.toml`. Every crate inherits them. Each enforces a rule
with a short code, like `KS-BOUNDED`, and
[the rules the code is held to](docs/rules.md) says what each one means.

## How it's laid out

A program goes through a line of checks. Each check hands the next a type that
only it can make, so nothing downstream can be given text that wasn't checked.

| Directory       | What it holds                                                 |
| --------------- | ------------------------------------------------------------- |
| `source`        | What every check shares: the size limit and positions.        |
| `linter/path`   | The check of a source file's path.                            |
| `linter/text`   | The check of a source file's text.                            |
| `lexer`         | The lexer, which turns checked text into tokens.              |
| `linter/tokens` | The check of how tokens are laid out.                         |
| `blocks`        | The nesting of laid-out lines into blocks.                    |
| `grammar`       | The check of each line against the grammar.                   |
| `ast`           | The tree of a program's functions.                            |
| `meaning`       | The checks of what code means: its types, and its program.    |
| `arm64`         | The generator of a program's machine code.                    |
| `macho`         | The writer of the signed executable macOS runs.               |
| `cli`           | The `stella` command, which reads files and runs the checks.  |
| `repository`    | Tests of the repository itself, like the reference's links.   |
| `test-support`  | What the tests share.                                         |
| `xtask`         | The workspace's own tools, run as `cargo xtask <command>`.    |
| `docs`          | The documentation, with the reference in `docs/reference`.    |

## License

Stella is provided under the [Mozilla Public License 2.0](https://mozilla.org/MPL/2.0/).

A copy of the MPLv2 is included in the [license.md](/license.md) file for convenience.
