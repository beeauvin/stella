# Keywords

Keywords are reserved: a keyword can never be used as a name. A
[token](tokens.md) spelled exactly as a keyword is always read as that keyword,
wherever it is, so it's never read as a name. Tested by
[`keywords`](../../lexer/tests/lex.rs). So a keyword where a name goes, as in
`function function()`, is refused. Tested by
[`function_lines`](../../grammar/tests/parse.rs).

Every keyword so far is a whole word: it's `function`, not `fn`. Keywords match
by case, so `Function` is a type's [name](names.md), not the keyword. Tested by
[`keywords`](../../lexer/tests/lex.rs).

- `function` begins a [function](functions.md).
- `return` starts the line in a function's body that gives back its value, as
  [functions](functions.md) says. Anywhere else, it's refused. Tested by
  [`return_lines`](../../grammar/tests/parse.rs) and
  [`return_and_integers`](../../grammar/tests/parse.rs).
- `returns` comes before a function's return type.

A keyword's voice follows what it does. One that declares, describing what
something does, reads "does this": `function main() returns Void` says what
`main` returns. One that acts at its own line reads "do this": `return` gives
the value back where it is. The two differ by one letter, but each has its own
place, and either one in the other's place is refused there. Tested by
[`return_and_integers`](../../grammar/tests/parse.rs).

Where each keyword goes is checked, as [functions](functions.md) says.
