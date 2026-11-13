# Types

## Void

`Void` is a type with exactly one value, so it carries no information. A
function that returns `Void` returns when its body ends.

`Void` is the type `main` returns. For now, it's the only type, so any other
type's name is refused at that name, even one that's almost `Void`, like
`Voids` or `VoiD`. Tested by
[`unknown_types`](../../meaning/tests/resolve.rs),
[`names_near_a_type`](../../meaning/tests/resolve.rs) and
[`every_program`](../../meaning/tests/resolve.rs).

That a function returns when its body ends isn't built yet. It waits for the
rest of the compiler.
