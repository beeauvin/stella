# Tokens

Code is made of tokens. The compiler reads them a line at a time, once the
[source text](source-text.md) has passed, so outside comments it's always
ASCII. The smallest program is eight tokens: `function`, `main`, `(`, `)`,
`returns`, `Void`, `{` and `}`. Tested by
[`the_smallest_program`](../../lexer/tests/lex.rs).

- An ASCII letter, then any ASCII letters, digits and `_`, is one token. The
  longest run is one token, so `main2` is one, and `main(` is `main` and then
  `(`. Tested by [`words`](../../lexer/tests/lex.rs).
- Spelled exactly as a [keyword](keywords.md), such a token is that keyword.
  Tested by [`keywords`](../../lexer/tests/lex.rs).
- Otherwise it's a name. A name that starts with a lowercase letter, like
  `main`, is a function's, a variable's or an instance's. One that starts with
  a capital, like `Void`, is a type's. Each has its one [shape](names.md).
- `(`, `)`, `{` and `}` are each a token of their own, with or without spaces
  around them. Tested by [`spaces_and_lines`](../../lexer/tests/lex.rs).
- A [comment](comments.md) is one token, from its `//` to the end of its line.
  Nothing in it is code. Tested by [`comments`](../../lexer/tests/lex.rs).
- Spaces separate tokens, and a blank line has none. Which spaces go where is
  for [layout](layout.md), so `function  main ( )returns Void{}` is still the
  same eight tokens. Tested by
  [`spaces_and_lines`](../../lexer/tests/lex.rs).

A token can be as long as a line, and the largest source can hold a token in
every byte but its last newline: 16,383 tokens. The compiler makes room for
that many once, before it reads any program, and every program it checks
reuses that room. The most tokens are tested by
[`the_most_tokens`](../../lexer/tests/lex.rs), and that one room serves source
after source by [`one_buffer_for_every_source`](../../lexer/tests/lex.rs).

Anything else in code is refused: every character in code is in a token or is
a space, and the first one that isn't is the problem. A name can hold a digit
or `_`, but can't start with one, so `9lives` is refused at the `9`. For now,
every other character, like `;`, `"` or a lone `/`, can't appear in code at
all. For example:

```
hello.stella:1:32: `;` can't start a token.
    function main() returns Void { ;
                                   ^
```

Every character code can hold is tried where a token starts, in a name and in
a type's name, by [`every_printable_character`](../../lexer/tests/lex.rs), and
where a line starts, after indentation, right after a symbol, and after a
comment that isn't ASCII, by
[`every_printable_character_in_context`](../../lexer/tests/lex.rs). The order is
tested by [`the_first_problem_wins`](../../lexer/tests/lex.rs), random sources
by [`fuzz_against_the_oracle`](../../lexer/tests/lex.rs), the sentence by
[`sentences`](../../lexer/tests/problem.rs), and the command by
[`code_that_cant_be_lexed`](../../cli/tests/lint.rs).
