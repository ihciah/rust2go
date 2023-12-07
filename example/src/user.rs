// This file must be included.
// The default filename `/rust2go.rs`, but it can be changed in build.rs.
include!(concat!(env!("OUT_DIR"), "/rust2go.rs"));

// Define your own structs.
#[derive(rust2go::R2G, Clone)]
pub struct DemoUser {
    pub name: String,
    pub age: u8,
}

// Define your own structs.
#[derive(rust2go::R2G, Clone)]
pub struct DemoComplicatedRequest {
    pub users: Vec<DemoUser>,
}

#[derive(rust2go::R2G, Clone)]
pub struct DemoResponse {
    pub pass: bool,
}

// Define the call trait.
// It can be defined in 2 styles: sync and async.
// If the golang side is purely calculation logic, and not very heavy, use sync can be more efficient.
// Otherwise, use async style:
// Both `async fn`` and `impl Future` styles are supported.

pub trait DemoCall {
    fn demo_oneway(req: &DemoUser);
    fn demo_check(req: &DemoComplicatedRequest) -> DemoResponse;
    fn demo_check_async(
        req: &DemoComplicatedRequest,
    ) -> impl std::future::Future<Output = DemoResponse>;
}
