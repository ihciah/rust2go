# Rust2Go

[![Crates.io](https://img.shields.io/crates/v/rust2go.svg)](https://crates.io/crates/rust2go)

Rust2Go is a project that provides users with a simple and efficient way to call Golang from Rust with native async support. It also support user calling Rust from Golang.

## Features

- Sync and async calls from Rust to Golang
- Sync calls from Golang to Rust
- Efficient data exchange: no serialization or  socket communication, but FFI
- Simple interface design: no new invented IDL except for native rust

## How to Use

1. Define the structs and calling interfaces in restricted Rust syntax, and include generated code in the same file.
2. Generate golang code with `rust2go-cli --src src/user.rs --dst go/gen.go`
3. Write a `build.rs` for you project.
4. You can then use generated implementation to call golang in your Rust project!

For detailed example, please checkout [the example projects](./examples).

## Key Design

> Detailed design details can be found in this article: [Design and Implementation of a Rust-Go FFI Framework](https://en.ihcblog.com/rust2go/).

### Why Fast?

1. Memory layout: Rust2go only manipulates memory when needed. In most cases it passes memory reference.
2. Message passing: Rust2go relies on CGO to pass calling information. In addition, it also supports lock-free queues based on shared memory to improve performance during high-frequency communication.
3. Other optimizations: Rust2go uses Go callback based on manual assembly instead of CGO to achieve better performance.

In order to achieve the ultimate performance, this project is not purely based on communication, but on FFI to pass specially encoded data. In order to reduce memory operations to a minimum, data that satisfies a specific memory layout is passed directly by reference rather than copied.

For example, `Vec<u8>` and `String` is represented as a pointer and a length. However, structs like `Vec<String>` or `Vec<Vec<u8>>` require intermediate representation. In order to reduce the number of memory allocations to one, I use a precomputed size buffer to store these intermediate structures.

### Memory Safety

On the Golang side, the data it receives is referenced from Rust. The Rust side will do its best to ensure the validity of this data during the call. So the Golang side can implement the handler arbitrarily, but manually deep copy when leaking data outside the function life cycle.

On the Rust side, it is needed to ensure that the slot pointer of the callback ffi operation, and the user parameters are valid when the future drops. This is archieved by implementing an atomic slot structure and providing a `[drop_safe]` attribute to require user passing parameters with ownership.

Note: Since golang may scan the stack, and when it meets peer pointer, it may panic. You should run the program with `GODEBUG=invalidptr=0,cgocheck=0` env to bypass it.

## Toolchain Requirements

- Golang: >=1.18
  - For >=1.18 && < 1.20: generate golang code with `--go118`
  - For >=1.20: generate golang code normally
- Rust: >=1.75 if you want to use async

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

### Performance Optimization

- [x] Shared memory based implementation
- [x] Faster ASM-based callback instead of CGO

### Extended Features

- [x] Support calling rust from golang

## Credit

This project is inspired by [fcplug](https://github.com/andeya/fcplug).
