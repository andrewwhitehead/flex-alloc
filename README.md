# flex-alloc

`flex-alloc` provides highly flexible container types (currently `Box`, `Cow`, and `Vec`) which mimic the API provided in `std`, with allocation flexibility going beyond what is supported by unstable features such as `allocator-api`.

Both `no-std` and `no-alloc` environments are supported.

The `flex-alloc-secure` crate adds data types for working with security-sensitive data such as cryptographic keys.

See the API documentation:

- [`flex-alloc`](https://docs.rs/flex-alloc)
- [`flex-alloc-secure`](https://docs.rs/flex-alloc-secure)

## `flex-alloc` highlights

- Optional `alloc` support, such that application may easily alternate between fixed buffers and heap allocation.
- Custom allocator implementations, including the ability to spill from a small stack allocation to a heap allocation.
- Additional fallible update methods, allowing for more ergonomic fixed size collections and handling of allocation errors.
- `const` initializers.
- Support for inline collections.
- Custom index types and growth behavior to manage memory usage.

## `flex-alloc-secure` highlights

- Collection types for working with secured allocations, using multiple levels of protections: memory locking (`mlock`/`VirtualLock`), memory protection (`mprotect`/`VirtualProtect`), and encryption at rest.
- Secure stack variables for working with sensitive data.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/andrewwhitehead/flex-collect/blob/main/LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](https://github.com/andrewwhitehead/flex-collect/blob/main/LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
