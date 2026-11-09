# Comments

A comment is a line that, after its indentation, starts with `//`. After the
`//`, the line either ends, or goes on with a space and then any text. Tested
by [`comments`](../../linter/text/src/text_tests.rs) and
[`accepted_sources`](../../linter/text/src/text_tests.rs).

```stella
// This line is a comment.
function main() returns Void {
    // So is this one.
    //
}
```

That's the only way to write a comment. The compiler refuses each of these
lines, and [`comments`](../../linter/text/src/text_tests.rs) tests each one:

- `function main() returns Void { // hi`, because code and a comment can't
  share a line.
- `//hi`, because a space must follow the slashes.
- `/// hi`, because three slashes don't start a comment.
- `//! hi`, because neither do two slashes and a bang.
