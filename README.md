# `pochita`

*A allocation arena for Rust.*

## Example

```rust
use std::collections::HashMap;

use pochita::DroplessArena;

#[derive(Default)]
struct SymbolTable {
    arena: DroplessArena<u8>,
    ids: HashMap<&'static str, usize>,
    strings: Vec<&'static str>,
}

impl SymbolTable {
    fn intern(&mut self, string: &str) -> usize {
        if let Some(key) = self.ids.get(string) {
            return *key;
        }

        let key = self.strings.len();
        let string = self.arena.alloc_str(string);
        let string: &'static str = unsafe { std::mem::transmute(string) };

        self.ids.insert(string, key);
        self.strings.push(string);

        key
    }

    fn as_str(&self, id: usize) -> &str {
        self.strings[id]
    }
}

fn main() {
    let mut table = SymbolTable::default();

    assert_eq!(table.intern("fn"), table.intern("fn"));
    assert_eq!(table.as_str(0), "fn");
}
```
