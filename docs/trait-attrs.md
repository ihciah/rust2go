---
title: Use attribute to control code generation
date: 2023-12-22 15:12:00
author: ihciah
---

Now rust2go supports 6 attributes on trait's async function:
1. `#[send]`: the function will be generated as `impl Future<Output=..> + Send + Sync`. Use it when you need it.
2. `#[drop_safe]`: this makes the function safe, but requires all parameters passing ownership. Use it when you cannot make sure the future may cancel.
3. `#[drop_safe_ret]`: to make the function safe, it requires passing ownership; this attribute allow users to get the parameters ownership back. Use it when you cannot make sure the future may cancel, and you want to get back the parameters ownership after the calling.
4. `#[mem]` or `#[shm]`: make this function implemented based on shared memory, whose performance is highly improved(but it requires unix now). Unless you find obvious performance bottlenecks, there is no need to enable it.
5. `#[go_pass_struct]`: make the generated go side code use pointer instead of value at parameters. This is useful when the parameter is large. This does not affect the rust side code. It is not recommended to enable this unless you explicitly want to pass the structure itself.
6. `#[cgo_callback]` (alias: `#[cgo]`): make the generated go side code use CGO based method instead of ASM. It is not recommended to enable it unless you find some failures caused by ASMCALL.

In the Go-to-Rust direction (`#[rust2go::g2r]`, see `examples/example-bidirectional`), a function-level `#[cgo_call]` (alias: `#[cgo]`) similarly makes the call CGO based instead of ASM.

## Trait-level parameters

The `#[rust2go::r2g(...)]` attribute itself accepts optional parameters:

- `binding` (or `binding = path::to::binding`): path of the module that includes the generated bindings. Example: `#[rust2go::r2g(binding = binding)]`.
- `queue_size = <N>`: capacity of the shared memory queue used by `#[mem]`/`#[shm]` functions. Defaults to `4096`.

They can be combined: `#[rust2go::r2g(binding = binding, queue_size = 4096)]`.

## Struct-level attributes

- Structs deriving `rust2go::R2G` keep their own attribute macros: attributes like `#[derive(Clone)]` or `#[allow(non_snake_case)]` are preserved on the generated `XxxRef` struct.
- `#[rust2go::r2g_struct_tag(key = "case", ...)]` adds tags to the fields of the generated Go struct. The key is the tag name (e.g. `json`, `yaml`) and the value is the field naming convention. Supported conventions: `snake_case`, `lowerCamelCase`, `UpperCamelCase`, `kebab-case`, `SHOUTY_SNAKE_CASE`, `SHOUTY-KEBAB-CASE`, `Title Case`, `Train-Case`.

For example:
```rust
#[rust2go::r2g_struct_tag(json = "snake_case", yaml = "lowerCamelCase")]
#[allow(non_snake_case)]
#[derive(rust2go::R2G, Clone)]
pub struct TaggedUser {
    pub UserName: String,
    pub LoginCount: u32,
}
```
generates the Go struct:
```go
type TaggedUser struct {
    UserName   string `json:"user_name" yaml:"userName"`
    LoginCount uint32 `json:"login_count" yaml:"loginCount"`
}
```

Note that Go field names are copied verbatim from the Rust field names; only the tag values are converted. Use exported-style Rust field names (with `#[allow(non_snake_case)]`) if you need exported Go fields.

## Type mapping notes

- `Option<T>` is treated as `Vec<T>`: `None` maps to an empty list on the Go side.
- Non-generic type aliases (e.g. `pub type Amount = i64;`) can be used in struct fields and trait signatures; they are expanded during code generation. Cyclic aliases are rejected: the code generator fails the build with a `cyclic type alias detected` error.

For example, here is the original trait:
```rust
#[rust2go::r2g]
pub trait DemoCall {
    #[send]
    fn demo_check_async(
        req: &DemoComplicatedRequest,
    ) -> impl std::future::Future<Output = DemoResponse>;
    #[drop_safe]
    fn demo_check_async_safe(
        req: DemoComplicatedRequest,
    ) -> impl std::future::Future<Output = DemoResponse>;
    #[drop_safe_ret]
    fn demo_check_async_safe_with_ret(
        req: DemoComplicatedRequest,
    ) -> impl std::future::Future<Output = DemoResponse>;
}
```

Here is the generated trait:
```rust
pub trait DemoCall {
    unsafe fn demo_check_async(
        req: &DemoComplicatedRequest,
    ) -> impl ::std::future::Future<Output = DemoResponse> + Send + Sync;

    fn demo_check_async_safe(
        req: DemoComplicatedRequest,
    ) -> impl ::std::future::Future<Output = DemoResponse> + 'static;

    fn demo_check_async_safe_with_ret(
        req: DemoComplicatedRequest,
    ) -> impl ::std::future::Future<Output = (DemoResponse, (DemoComplicatedRequest,))> + 'static;
}
```

Note, if all parameters are with ownership, the generated impl Future will be added with a `'static` lifetime automatically. This is useful for spawning tasks.
