fn main() {
    rust2go::Builder::new()
        .with_go_src("./go")
        .with_regen("./src/user.rs", "./go/gen.go", false)
        .build();
}
