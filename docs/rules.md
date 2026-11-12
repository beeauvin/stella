# The rules the code is held to

Stella's own code follows a small set of rules, each with a short code. The
codes turn up in three places: beside each line of the lint table in the root
`Cargo.toml` and `clippy.toml`, in the reason of every `#[expect]`, and in a
comment wherever the code departs from a rule on purpose. This page says what
each one means.

The rules come in an order: safety, then performance, then how pleasant the
code is to work in. When two pull against each other, the earlier one wins.

A lint marked `hygiene` in the lint table enforces no rule in particular. It
keeps the code tidy, or refuses something that's nearly always a mistake.

## Departing from a rule

A rule is its reason. Where the reason doesn't hold, the code may depart from
the rule, and it says so at that very place: the rule's code, then why its
reason doesn't apply there.

Where a lint enforces the rule, the departure is an `#[expect]` on the
smallest item it can go on:

```rust
#[expect(clippy::as_conversions, reason = "TS-SIZED-TYPES: `try_from` can't be called here")]
```

An `#[expect]` that no longer has anything to expect fails the build, so a
departure can't outlive its cause. Where no lint enforces the rule, it's a
comment directly above the place:

```rust
// TS-BOUNDED: the operating system bounds a process's arguments.
```

A departure without its reason is a mistake, not a judgement call.

## Safety

### TS-SIMPLE-FLOW

Control flow is simple and explicit. Nothing recurses: something that walks a
tree loops over a stack with a bound. A branch names every case. So a reader
can see every path the code can take, and the limit on each.

### TS-BOUNDED

Everything has a limit that's written down and enforced: a loop, a buffer, a
collection, a count of retries. A limit that's written fails early, where the
cause is plain, instead of late, as memory that ran out.

### TS-COMPOUND-CONDITIONS

A reader can see that every combination of a condition's inputs is handled. A
condition that hides a case is split into nested branches, or a match on the
pair, and a chain of `else if` ends with an `else`.

### TS-ASSERT-TRUST

An assertion is a claim about what our own code produced, and when it fails,
the code is wrong and the program stops. Anything that came from outside, such
as a file, an argument or a name the filesystem listed, is input. A check of
input returns an error that's handled, and is never an assertion.

### TS-ASSERT-ARGS

A function checks what it's given and what it gives back. Assertions stay on
in a release build, since that's where the rare case turns up, so
`debug_assert!` isn't used.

### TS-ASSERT-SPLIT

One assertion checks one thing, and carries a message that says why it holds.
When it fails, it says exactly which thing broke.

### TS-HANDLE-ERRORS

Every error is passed on with what the caller needs to act on it, recovered
from, or thrown away at a line that says what's thrown away and why that's
right. Each outside system, such as the filesystem or Cargo, has one module
that turns its failures into the program's own.

### TS-SIZED-TYPES

A number that's stored, or that means something in the language, has a width
that's written out. Turning one into an index into memory, or back, goes
through a named function that asserts nothing is lost. It's never an `as`.

### TS-OFF-BY-ONE

An index, a count and a size are different things, and a name says which it
holds. A span is a range, not a start and a length. Arithmetic that overflows
stops the program in a release build too.

### TS-DISTINCT-ARGS

Arguments can't be mixed up. Two values of one type that mean different things
are given as a struct that names each, or as types of their own, and a
function takes three parameters at most, counting `self`.

### TS-NO-UNSAFE

There's no `unsafe`. Every crate's root forbids it, since a crate can't be
made to inherit that from the workspace.

### TS-STRICT-LINTS

The lints are the strictest set that doesn't contradict itself. They're
declared once, every crate inherits them, and a warning fails the build. A lint
is never switched off for a crate or a module. `cargo xtask lints` proves the
set bites: it builds a small fixture for each lint and fails unless that lint
is what's reported.

### TS-MACROS

What a macro expands to depends only on what the repository says. No macro
reads a file or the environment while the code is built, so a test reads the
variables Cargo gives it when it runs, with `std::env`.

## Performance

### TS-DESIGN-PERF

Performance is designed in, not found later. The sizes of the types that are
stored in number and passed around are known, kept small, and asserted where
a buffer's budget leans on them.

## Working in the code

### TS-NO-ABBREVIATIONS

Names are spelled out: `Parenthesis`, not `Paren`. An acronym in a name is
written as a word, as in `HttpReply`.

### TS-UNITS-LAST

A name puts its unit or its qualifier last, as in `SOURCE_BYTES_MAX`, so that
names for related things line up and say what they count.

### TS-CLOSE-USE

A value is checked where it's used. Looking at a path and then using it
later, as if nothing could change in between, is two questions asked of the
filesystem at two times.

### TS-SAY-HOW

A test opens with a comment that says what it checks and how, so a failure can
be understood without working backwards from the code. Its name doesn't start
with `test_`.

## Style

### TS-FN-LENGTH

A function is 70 lines at most, which is about what can be seen at once.

### TS-COMMENT-STYLE

A comment is written as sentences, with their punctuation. The documentation
of anything public says when it returns an error and when it panics.

### TS-WEIGH-DEPS

A dependency is weighed against writing and owning the code it would bring.
The workspace has none.
