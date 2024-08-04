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
#[derive(rust2go::R2G, Clone)]
pub struct DemoComplicatedRequest {
    pub users: Vec<DemoUser>,
    pub balabala: Vec<u8>,
}

// Define your own structs. You must derive `rust2go::R2G` for each struct.
#[derive(rust2go::R2G, Clone, Copy)]
pub struct DemoResponse {
    pub pass: bool,
}

// Define the call trait.
// It can be defined in 2 styles: sync and async.
// If the golang side is purely calculation logic, and not very heavy, use sync can be more efficient.
// Otherwise, use async style:
// Both `async fn` and `impl Future` styles are supported.
//
// If you want to use your own binding mod name(by default it is binding), use:
// `#[rust2go::r2g(binding)]` or `#[rust2go::r2g(binding = binding)]`
// If you want to set queue size manually(by default it is 4096), use:
// `#[rust2go::r2g(queue_size = 4096)]`
#[rust2go::r2g(binding = binding, queue_size = 40960)]
pub trait DemoCall {
    #[mem]
    fn demo_oneway(req: &DemoUser);
    #[mem]
    fn demo_check_async(
        req: &DemoComplicatedRequest,
    ) -> impl std::future::Future<Output = DemoResponse>;
    #[drop_safe_ret]
    #[mem]
    fn demo_check_async_safe(
        req: DemoComplicatedRequest,
    ) -> impl std::future::Future<Output = DemoResponse>;
}
