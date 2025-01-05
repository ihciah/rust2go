mod user;

use user::{DemoCall, DemoCallImpl, DemoUser};

fn main() {
    let user = DemoUser {
        name: "chihai".to_string(),
        age: 28,
    };
    println!("========== Start oneway demo ==========");
    println!(
        "[Rust Caller] will call golang with user name={}, age={}",
        user.name, user.age
    );
    DemoCallImpl::demo_oneway(&user);
    println!("[Rust Caller] done");

    println!("========== Start call demo ==========");
    println!(
        "[Rust Caller] will call golang with user name={}, age={}",
        user.name, user.age
    );
    let resp = DemoCallImpl::demo_call(&user);
    println!("[Rust Caller] done, user checking pass: {}", resp.pass);
}
