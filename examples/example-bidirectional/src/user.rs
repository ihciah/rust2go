// Include the binding file. There are 2 ways to include it:
// 1. Use rust2go's macro:
// ```rust
// pub mod binding {
//     rust2go::r2g_include_binding!();
// }
// ```
// 2. Include it manually:
// ```rust
// pub mod binding {
//     include!(concat!(env!("OUT_DIR"), "/_go_bindings.rs"));
// }
// ```
// If you want to use your own binding file name, use:
// `rust2go::r2g_include_binding!("_go_bindings.rs");`
pub mod binding {
    #![allow(warnings)]
    rust2go::r2g_include_binding!();
}

// Define your own structs. You must derive `rust2go::R2G` for each struct.
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

// Define the call trait(Rust -> Go).
// It can be defined in 2 styles: sync and async.
// If the golang side is purely calculation logic, and not very heavy, use sync can be more efficient.
// Otherwise, use async style:
// Both `async fn` and `impl Future` styles are supported.
//
// If you want to use your own binding mod name, use:
// `#[rust2go::r2g(binding)]`
#[rust2go::r2g]
pub trait DemoCall {
    fn demo_oneway(user: &DemoUser);
    fn demo_call(user: &DemoUser) -> DemoResponse;
}

// Define the call trait(Go -> Rust).
// It can only be sync call for now.
// If you need to execute some async logic, you may spawn the future.
// For now, all parameters comes from golang side are copied.
#[rust2go::g2r]
pub trait G2RCall {
    fn demo_log(name: String, age: u8);
    fn demo_convert_name(user: DemoUser) -> String;
}

impl G2RCall for G2RCallImpl {
    fn demo_log(name: String, age: u8) {
        println!("[Rust Callee] log user {name} and age {age}");
    }

    fn demo_convert_name(user: DemoUser) -> String {
        let new_user_name = user.name.to_ascii_uppercase();
        println!(
            "[Rust Callee] convert user username: {} -> {new_user_name}",
            user.name
        );
        new_user_name
    }
}
