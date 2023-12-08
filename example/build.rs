fn main() {
    rust2go::Builder::new()
        .with_rs_idl("./src/user.rs")
        .with_go_src("go/gen.go")
        .with_link(rust2go::LinkType::Dynamic)
        .build();
}
