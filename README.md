# Rust2Go
[![Crates.io](https://img.shields.io/crates/v/rust2go.svg)](https://crates.io/crates/rust2go)

Rust2Go is a project that provides users with a simple and efficient way to call Golang from Rust with native async support.

## Features

- Sync and async calls from Rust to Golang
- Efficient data exchange: no serialization or  socket communication, but FFI
- Simple interface design: no new invented IDL except for native rust

## How to Use

1. Define the structs and calling interfaces in restricted Rust syntax, and include generated code in the same file.
2. Generate golang code with `rust2go-cli --src src/user.rs --dst go/gen.go`
3. Write a `build.rs` for you project.
4. You can then use generated implementation to call golang in your Rust project!

For detailed example, please checkout [the example project](./example).

## Key Design

> Detailed design details can be found in this article: [Design and Implementation of a Rust-Go FFI Framework](https://en.ihcblog.com/rust2go/).

### Why Fast?
In order to achieve the ultimate performance, this project is not based on communication, but on FFI to pass specially encoded data. In order to reduce memory operations to a minimum, data that satisfies a specific memory layout is passed directly by reference rather than copied.

For example, `Vec<u8>` and `String` is represented as a pointer and a length. However, structs like `Vec<String>` or `Vec<Vec<u8>>` require intermediate representation. In order to reduce the number of memory allocations to one, I use a precomputed size buffer to store these intermediate structures.

### Memory Safety
On the Golang side, the data it receives is referenced from Rust. The Rust side will do its best to ensure the validity of this data during the call. So the Golang side can implement the handler arbitrarily, but manually deep copy when leaking data outside the function life cycle.

On the Rust side, it is needed to ensure that the slot pointer of the callback ffi operation, and the user parameters are valid when the future drops. This is archieved by implementing an atomic slot structure and providing a `[drop_safe]` attribute to require user passing parameters with ownership.

## Milestones
### Init Version
- [x] IDL(in rust) parse
- [x] Go code generation
- [x] Build script helper
- [x] Basic data types and convertion generation
- [x] Rust impl generation
- [x] Future and basic synchronization primitives used

### Basic Ability Enhancement
- [x] More complicated data types support
- [x] Support user passing references
- [x] More elegant code generation implementation
- [x] Better build cache control
- [x] Golang interface support(separate user code from generated code)
- [x] Dynamic linking support
- [x] Golang helper library

### Extended Features
- [ ] Support calling rust from golang

## Credit
This project is inspired by [fcplug](https://github.com/andeya/fcplug).
