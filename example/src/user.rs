// This file must be included.
// The default filename `/rust2go.rs`, but it can be changed in build.rs.
include!(concat!(env!("OUT_DIR"), "/rust2go.rs"));

// Define your own structs.
pub struct DemoRequest {
    pub name: String,
    pub age: u8,
}

pub struct DemoResponse {
    pub pass: bool,
}

// Define the call trait.
// It can be defined in 2 styles: sync and async.
// If the golang side is purely calculation logic, and not very heavy, use sync can be more efficient.
// Otherwise, use async style:
// Both `async fn`` and `impl Future` styles are supported.

pub trait DemoCall {
    fn demo_check(req: DemoRequest) -> DemoResponse;
    fn demo_check_async(req: DemoRequest) -> impl std::future::Future<Output = DemoResponse>;
}
