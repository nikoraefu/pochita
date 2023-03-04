# `pochita`

*A allocation arena for Rust.*

## Example

```rust
use pochita::DroplessArena;

fn main() {
    let mut arena: DroplessArena<u8> = DroplessArena::default();

    assert_eq!(arena.alloc_str("Hello World!"), "Hello World!");
    assert_eq!(arena.alloc_slice_copy(b"Hello World!"), b"Hello World!");
}
```
