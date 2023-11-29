# Rust2Go

Rust2Go is a project that assists users in calling Golang from Rust. The goal of this project is to provide a simple and efficient way for Rust developers to leverage the powerful features of Golang.

## Features

- Sync and async calls from Rust to Golang
- Efficient data exchange
- Simple API design

## How to Use

1. Define the structs and calling interfaces in restricted Rust syntax, and include generated code in the same file.
2. Generate golang code with `rust2go-cli --src src/user.rs --dst go/gen.go`
3. Write a `build.rs` for you project.
4. You can then use generated implementation to call golang in your Rust project!

For detailed example, please checkout [the example project](./example).

## Milestones
### Init Version
- [x] IDL(in rust) parse
- [x] Go code generation
- [x] Build script helper
- [x] Basic data types and convertion generation
- [x] Rust impl generation
- [x] Future and basic synchronization primitives used

### Basic Ability Enhancement
- [ ] More complicated data types support
- [x] Support user passing references
- [ ] More elegant code generation implementation
- [ ] Better build cache control
- [ ] Golang interface support(separate user code from generated code)
- [ ] Dynamic linking support
- [ ] Golang helper library

### Extended Features
- [ ] Support calling rust from golang

## Credit
This project is inspired by [fcplug](https://github.com/andeya/fcplug).
