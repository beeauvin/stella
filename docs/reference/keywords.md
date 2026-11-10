# Keywords

Keywords are reserved: a keyword can never be used as a name. A
[token](tokens.md) spelled exactly as a keyword is always read as that keyword,
wherever it is, so it's never read as a name. Tested by
[`keywords`](../../lexer/tests/lex.rs). Refusing a keyword where a name goes,
as in `function function()`, isn't built yet. It waits for the parser.

Every keyword so far is a whole word: it's `function`, not `fn`. Keywords match
by case, so `Function` is a type's [name](names.md), not the keyword. Tested by
[`keywords`](../../lexer/tests/lex.rs).

- `function` begins a [function](functions.md).
- `returns` comes before a function's return type.

What each keyword does isn't built yet. It waits for the parser.
