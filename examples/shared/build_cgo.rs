use rust2go::RegenArgs;

fn main() {
    rust2go::Builder::new()
        .with_go_src("./go")
        .with_regen_arg(RegenArgs {
            src: "../shared/user_cgo.rs".into(),
            dst: "./go/gen.go".into(),
            // Newer Go versions are faster; keep this off unless you must target Go 1.18.
            go118: true,
            ..Default::default()
        })
        .build();
}
