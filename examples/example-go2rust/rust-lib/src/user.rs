// Define your own structs. You must derive `rust2go::R2G` for each struct.
#[derive(rust2go::R2G, Clone)]
pub struct DemoUser {
    pub name: String,
    pub age: u8,
}

// Define your own structs. You must derive `rust2go::R2G` for each struct.
#[derive(rust2go::R2G, Clone, Copy)]
#[allow(dead_code)]
pub struct DemoResponse {
    pub pass: bool,
}

#[rust2go::g2r]
pub trait G2RCall {
    fn demo_log(name: String, age: u8);
    fn demo_convert_name(user: DemoUser) -> String;
}

// Stateful g2r trait: every method takes `&self`. The macro generates a
// process-wide instance registry; implement this trait on your own struct
// and install it once via `G2RStatefulCallImpl::register(...)` (see lib.rs).
#[rust2go::g2r]
pub trait G2RStatefulCall {
    fn incr(&self, by: u64) -> u64;
    fn current(&self) -> u64;
}
