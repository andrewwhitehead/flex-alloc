# flex-alloc

This crate provides highly flexible storage for `std`-compatible container types (currently `Box`, `Cow`, and `Vec`), going beyond what is supported by unstable features such as `allocator-api`.

Both `no-std` as well as `no-alloc` environments are supported.

## Feature flags

- The `std` flag (off by default) enables compatibility with the `std::error::Error` trait for error types, adds `io::Write` support to `Vec`, and also enables the `alloc` feature.

- With the `alloc` feature (on by default), access to the global allocator is enabled, and default constructors for allocated containers (such as `Vec::new`) are supported.

- The `allocator-api2` feature enables integration with the `allocator-api2` crate, which offers support for the `allocator-api` feature set on stable Rust. This can allow for allocators implementing the API to be passed to `Box::new_in` and `Vec::new_in`.

- The `zeroize` feature enables integration with the `zeroize` crate, including a zeroizing allocator. This can be used to automatically zero out allocated memory for allocated types, including the intermediate buffers produced during resizing in the case of `Vec`.

## Credits

This crate is inspired by [coca](https://crates.io/crates/coca), which has generally broader functionality, but currently only supports fixed-size containers. Portions are adapted from the Rust standard library.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/andrewwhitehead/flex-collect/blob/main/LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](https://github.com/andrewwhitehead/flex-collect/blob/main/LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
