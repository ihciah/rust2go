use user::{DemoUser, G2RCall, G2RCallImpl};

mod user;

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
