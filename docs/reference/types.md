# Types

## Void

`Void` is a type with exactly one value, so it carries no information. A
function that returns `Void` returns when its body ends.

`Void` is one of the types `main` can return. A function that returns `Void`
has no `return`: one that does is refused at its `return`. Tested by
[`void`](../../meaning/tests/fit.rs).

For now, `Void` and `U8` are the only types, so any other type's name is
refused at that name, even one that's almost one of them, like `Voids`,
`VoiD`, `U16` or `U9`. Tested by
[`unknown_types`](../../meaning/tests/resolve.rs),
[`names_near_a_type`](../../meaning/tests/resolve.rs) and
[`every_program`](../../meaning/tests/resolve.rs).

That a function returns when its body ends is tested by
[`the_smallest_program`](../../arm64/tests/generate.rs), which pins a body's
code as `ret`, and by running the binary, in
[`macos_checks_the_signature_and_runs_it`](../../macho/tests/write.rs).

## U8

`U8` is an integer type: its values are the whole numbers from 0 to 255, so a
value fits in a byte. A function that returns `U8` ends with `return` and an
integer, which is the value it gives back:

```stella
function main() returns U8 {
    return 3
}
```

The integer must be one `U8` holds. One that's greater is refused at the
integer, however long it is:

```
three.stella:2:12: this integer doesn't fit in `U8`, which holds 0 to 255.
        return 256
               ^
```

Every integer from 0 to 1,000 is tested by
[`every_integer_up_to_a_thousand`](../../meaning/tests/fit.rs), and ones
far past a `U8`, as long as a source can hold, by
[`long_integers`](../../meaning/tests/fit.rs). A function that returns `U8`
with no `return` is refused at the `}` that closes its body, since it ends
with nothing to give back. Tested by
[`missing_returns`](../../meaning/tests/fit.rs).

A `main` that returns `U8` makes the program exit with its value, as
[programs](programs.md) says.
