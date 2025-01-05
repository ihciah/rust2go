# Examples

| Name                   | Call Direction | Runtime | Backend Technology |
|------------------------|----------------|---------|--------------------|
| example-monoio         | Rust -> Go     | Monoio  | CGO                |
| example-tokio          | Rust -> Go     | Tokio   | CGO                |
| example-monoio-mem     | Rust -> Go     | Monoio  | Shared Memory Lockless Queue |
| example-tokio-mem      | Rust -> Go     | Tokio   | Shared Memory Lockless Queue |
| example-bidirectional  | Rust -> Go & Go -> Rust  | N/A | CGO          |
| example-go2rust        | Go -> Rust     | N/A     | CGO                |
