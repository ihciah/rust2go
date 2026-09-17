# Rust2Go Example for Bidirectional Calling

Calling Direction: Bidirectional(Rust -> Go && Go -> Rust)
Backend Technology: CGO

In this demo, we will call go from rust, and call rust within the go handler.

> Note: Though we only put sync calling here, but you can always use async when calling go from rust.

## Steps

1. Add dependency and build-dependency to `Cargo.toml`:

    ```toml
    [dependencies]
    rust2go = { version = "0.4.0" }

    [build-dependencies]
    rust2go = { version = "0.4.0", features = ["build"] }
    ```

    And also install commandline tool:

    ```sh
    cargo install --force rust2go-cli
    ```

2. Create an empty `user.rs` and add it to `main.rs`: `mod user;`.

3. Define request and response structs, and calling conventions in `user.rs`. You should also add `#[derive(rust2go::R2G)]` to your structs.

    ```rust
    #[derive(rust2go::R2G)]
    pub struct DemoUser {
        pub name: String,
        pub age: u8,
    }

    #[derive(rust2go::R2G)]
    pub struct DemoResponse {
        pub pass: bool,
    }

    pub trait DemoCall {
        fn demo_oneway(user: &DemoUser);
        fn demo_call(user: &DemoUser) -> DemoResponse;
    }
    ```

4. Create an empty go project and initialize it: `mkdir go && cd go && go mod init r2gexample`; or you can use any existed project.

5. Generate golang code with `rust2go-cli --src src/user.rs --dst go/gen.go`. Then create `impl.go`(the file name can be anything you want) and define the struct to implement generated `{$trait}` interface and assign it to `{$trait}Impl`.

6. Write a `build.rs` with the following content:

    ```rust
    fn main() {
        rust2go::Builder::new().with_go_src("./go").build();
    }
    ```

7. Add an include to the top of `user.rs` to make sure the generated code is used:

    ```rust
    pub mod binding {
        rust2go::r2g_include_binding!();
    }
    ```

    Also add macro `#[rust2go::r2g]` to your trait:

    ```rust
    #[rust2go::r2g]
    pub trait DemoCall {
        fn demo_oneway(user: &DemoUser);
        fn demo_call(user: &DemoUser) -> DemoResponse;
    }
    ```

8. Call the golang with `user::{$trait}Impl`.

    ```rust
    fn main() {
        let user = DemoUser {
            name: "chihai".to_string(),
            age: 28,
        };
        let resp = DemoCallImpl::demo_call(&user);
        println!("user checking pass: {}", resp.pass);
    }
    ```

9. Run it and it will show `user checking pass: true`! Then you can edit the golang code in `go/gen.go` and customize golang side logic.

## The Go -> Rust half

This demo also defines a `#[rust2go::g2r]` trait `G2RCall` in
`src/user.rs` (implemented on the generated `G2RCallImpl` in the same file).
The Go handlers in `go/impl.go` call back into Rust through
`G2RCallImpl{}.demo_log(...)` / `G2RCallImpl{}.demo_convert_name(...)` while
serving the Rust -> Go calls. See [examples/example-go2rust](../example-go2rust)
for a full Go -> Rust walkthrough, including stateful (`&self`) traits.
