# caplog
A Rust library providing log capture facilities for testing.

This crate is primarily intended for use with the
[`log`](https://crates.io/crates/log) crate, however additional logging
facilities are welcome.

# Usage

```rust
use log::warn;

#[test]
fn test_scramble_message() {
   let handle = caplog::get_handle();
   warn!("scrambled eggs");
   assert!(handle.any_msg_contains("scrambled eggs"));
}
```

