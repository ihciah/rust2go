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
