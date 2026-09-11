# Rust2Go Example for Go Call Rust

Calling Direction: Go -> Rust
Backend Technology: CGO

In this demo, we will call rust from go. Rust is compiled as a statically/dynamically linked lib, and go is compiled as the main program.

## Steps

1. Add dependency and build-dependency to rust side `Cargo.toml`:

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

2. Create an empty `user.rs` and add it to `lib.rs`: `mod user;`.

3. Define request and response structs, and calling conventions in `user.rs`. You should also add `#[derive(rust2go::R2G)]` to your structs and `#[rust2go::g2r]` to your traits.

    In `user.rs`, add:

    ```rust
    #[derive(rust2go::R2G, Clone)]
    pub struct DemoUser {
        pub name: String,
        pub age: u8,
    }

    // Define your own structs. You must derive `rust2go::R2G` for each struct.
    #[derive(rust2go::R2G, Clone, Copy)]
    pub struct DemoResponse {
        pub pass: bool,
    }

    #[rust2go::g2r]
    pub trait G2RCall {
        fn demo_log(name: String, age: u8);
        fn demo_convert_name(user: DemoUser) -> String;
    }
    ```

4. Implement the trait for `{$trait}Impl` manually:

    > You can do it within `lib.rs` or other places, but it is not recommended in the same file the traits and structs defined.

    ```rust
    impl user::G2RCall for user::G2RCallImpl {
        fn demo_log(name: String, age: u8) {
            println!("[Rust Callee] log user {name} and age {age}");
        }

        fn demo_convert_name(user: user::DemoUser) -> String {
            let new_user_name = user.name.to_ascii_uppercase();
            println!(
                "[Rust Callee] convert user username: {} -> {new_user_name}",
                user.name
            );
            new_user_name
        }
    }
    ```

5. Make the rust side compiles as library:

    In rust side `Cargo.toml`, add:

    ```toml
    [lib]
    crate-type = ["cdylib", "staticlib"]
    ```

    `cdylib` is for dynamically link; `staticlib` is for statically link.

6. Generate golang code with `rust2go-cli --src rust-lib/src/user.rs --dst gen.go --without-main`. Then you can call it in your `main.go` with `{$trait}Impl`, like:

    ```go
    func main() {
        user := DemoUser{
            name: "chihai",
            age:  28,
        }
        G2RCallImpl{}.demo_log(&user.name, &user.age)
        new_name := G2RCallImpl{}.demo_convert_name(&user)
        fmt.Printf("new name: %s", new_name)
    }
    ```

7. Add essential link arguments in your `main.go`.

    ```go
    /*
    // For statically link: #cgo LDFLAGS: ./librust_lib.a
    // For dynamically link: #cgo LDFLAGS: -L. -lrust_lib
    #cgo LDFLAGS: ./librust_lib.a
    */
    import "C"
    ```

    You have to adjust the path. One way is copying rust side output to current directory; another way is to set relative path directly to output. Here I use the first way.

8. Write a shell script `build_static.sh` to compile, which can avoid linking the old rust output.

    You have to adjust the rust output path.

    ```sh
    #!/bin/sh

    # build rust-lib
    cd rust-lib || exit
    cargo build --release || exit
    cd .. || exit

    # copy output
    # [NOTE] You may have to adjust the path by your own!
    cp ../../target/release/librust_lib.a ./

    # build go
    go build .
    ```

    > Remember to `chmod +x build_static.sh` to make it able to run.

9. Compile and Run.

    ```text
    ❯ ./build_static.sh && ./example-go2rust
    Compiling rust-lib v0.1.0 (/home/ihciah/code/ihciah/rust2go/examples/example-go2rust/rust-lib)
    Finished `release` profile [optimized] target(s) in 0.14s
    [Rust Callee] log user chihai and age 28
    [Rust Callee] convert user username: chihai -> CHIHAI
    new name: CHIHAI
    [Rust Callee] counter incr by 1 -> 1
    counter after incr(1): 1
    [Rust Callee] counter incr by 1 -> 2
    counter after incr(1): 2
    [Rust Callee] counter current: 2
    counter current: 2
    ```

    It works. Congratulations!

## Stateful Traits (`&self` methods)

The `G2RCall` trait above is stateless: its methods have no receiver, and the
generated `G2RCallImpl` unit struct implements the trait directly.

If **every** method of a `#[rust2go::g2r]` trait takes `&self`, the trait
becomes *stateful*: the macro generates a process-wide instance registry and
a `register` function, and FFI entries dispatch through the registered
instance. This demo includes one (`G2RStatefulCall` in
`rust-lib/src/user.rs`):

```rust
#[rust2go::g2r]
pub trait G2RStatefulCall {
    fn incr(&self, by: u64) -> u64;
    fn current(&self) -> u64;
}
```

Implement it on your own struct that holds the state, and register one
instance at startup. Since Go may call the methods from any thread, the
implementation must be `Send + Sync`; use atomics or a `Mutex` for mutable
state:

```rust
struct StatefulCounter {
    count: AtomicU64,
}

impl G2RStatefulCall for StatefulCounter {
    fn incr(&self, by: u64) -> u64 {
        self.count.fetch_add(by, Ordering::SeqCst) + by
    }

    fn current(&self) -> u64 {
        self.count.load(Ordering::SeqCst)
    }
}
```

A Rust staticlib/cdylib has no automatic init hook, so this demo exports
`rust_lib_init` (see `rust-lib/src/lib.rs`), which calls
`G2RStatefulCallImpl::register(...)`, and `main.go` invokes it via cgo
before any stateful call:

```go
C.rust_lib_init()
```

Notes:

- `register` consumes a global `OnceLock`: calling it twice returns `Err`.
- Calling a stateful trait method from Go before registration prints an
  error and aborts the process — always register during startup.
- The instance lives for the whole process; there is no unregister.
- Mixing `&self` and non-`&self` methods in one g2r trait is a compile
  error, as are `&mut self` / by-value `self` receivers.
- The Go side calling convention is identical to the stateless case:
  `G2RStatefulCallImpl{}.incr(&one)`.

10. Dynamically link:

    If you want to compile it as dynamically link, there are 2 things to do:

    1. In `main.go`, change the way to link:

        ```go
        /*
        // For statically link: #cgo LDFLAGS: ./librust_lib.a
        // For dynamically link: #cgo LDFLAGS: -L. -lrust_lib
        #cgo LDFLAGS: -L. -lrust_lib
        */
        import "C"
        ```

    2. Distribute the dynamic library with the executable.

        You don't have to copy `.a` any more when build go program, but you have to distribute the `.so`(linux)/`.dylib`(macos)/`.dll`(windows) when run and make it able to find.

        For example, `export LD_LIBRARY_PATH=$(pwd):$LD_LIBRARY_PATH` to add the current directory. There are many other ways like set RPATH but it is out of this document scope.

        The executable is expected to operate the same as static linking.
