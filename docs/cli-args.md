---
title: Commandline tool arguments
date: 2024-11-07 00:00:00
author: ihciah
---

# Commandline Tool Arguments
1. `src` \[required\]: Path of source rust file
2. `dst` \[required\]: Path of destination go file
3. `without_main` \[optional, default=`false`\]: With or without go main function
4. `go118` \[optional, default=`false`\]: Go 1.18 compatible
5. `no_fmt` \[optional, default=`false`\]: Disable auto format go file
6. `recycle` \[optional, default=`false`\]: Enable object pool

# Usage
1. The arguments can be used in commline tool:
```shell
rust2go-cli --src src.rs --dst dst.go --without_main --go118 --no_fmt --recycle
```

2. The arguments can also be used in `build.rs` to generate go file automatically:
```rust
use rust2go::RegenArgs;

fn main() {
    rust2go::Builder::new()
        .with_go_src("./go")
        .with_regen_arg(RegenArgs {
            src: "./src/user.rs".into(),
            dst: "./go/gen.go".into(),
            go118: true,
            recycle: true,
            ..Default::default()
        })
        .build();
}
```
