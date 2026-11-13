# Names

For now, a name has one of two shapes, and the compiler refuses any other.
Tested by [`names`](../../lexer/tests/lex.rs) and
[`every_short_word`](../../lexer/tests/lex.rs).

- A function's, a variable's or an instance's name is snake case: lowercase
  letters and digits, with single underscores between them, like `main` or
  `my_function`. It starts with a letter, and a digit can go anywhere after
  that, as in `rung_0`. A capital is refused, so camel case like `myFunction`
  is too, and so are two underscores in a row and an underscore at the end.
- A type's name is Pascal case: words that each start with one capital, then
  lowercase letters and digits, like `Void` or `MessageQueue`. An underscore is
  refused, and so are two capitals in a row, so it's `Http`, not `HTTP`.

So the first letter says which a name is: lowercase for a function's, a
variable's or an instance's, and a capital for a type's.

A name that breaks its shape is refused at the first character that breaks it:

```
hello.stella:1:12: a name must be lowercase letters and digits joined by single underscores, like `my_function`.
    function myFunction() returns Void {
               ^
```

Each character is tried in a name and in a type's name by
[`every_printable_character`](../../lexer/tests/lex.rs), and the sentences by
[`sentences`](../../lexer/tests/problem.rs).
