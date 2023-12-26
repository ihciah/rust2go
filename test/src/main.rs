mod user;

fn main() {}

#[cfg(test)]
mod tests {
    use super::user::*;

    #[test]
    fn echo() {
        assert_eq!(TestCallImpl::ping(1995), 1995);
    }

    #[monoio::test(timer_enabled = true)]
    async fn login() {
        macro_rules! login {
            ($id:expr, $name:expr, $pass:expr, $message: expr) => {{
                let r = TestCallImpl::login(&LoginRequest {
                    user: User {
                        id: $id,
                        name: $name,
                        age: 0,
                    },
                    password: $pass.to_string(),
                });
                assert_eq!(r.message, $message);
                r.succ
            }};
        }
        assert!(login!(1, String::new(), "test_psw", "login success"));
        assert!(login!(0, "test".to_string(), "test_psw", "login success"));
        assert!(!login!(1, String::new(), "wrong_psw", "invalid password"));
        assert!(!login!(100, String::new(), "", "user not exist"));
        TestCallImpl::logout(&User {
            id: 1,
            name: String::new(),
            age: 0,
        });
    }

    #[monoio::test(timer_enabled = true)]
    async fn add_friends() {
        let wrong_token_req = unsafe {
            TestCallImpl::add_friends(&FriendsListRequest {
                token: vec![],
                user_ids: vec![],
            })
        }
        .await;
        assert!(wrong_token_req.users.is_empty());

        let no_valid_users_req = unsafe {
            TestCallImpl::add_friends(&FriendsListRequest {
                token: vec![6, 6, 6],
                user_ids: vec![100, 101, 102],
            })
        }
        .await;
        assert!(no_valid_users_req.users.is_empty());

        let valid_req = unsafe {
            TestCallImpl::add_friends(&FriendsListRequest {
                token: vec![6, 6, 6],
                user_ids: vec![1, 2, 3],
            })
        }
        .await;
        assert!(valid_req
            .users
            .iter()
            .zip([1, 2, 3].iter())
            .all(|(x, y)| x.id == *y));
    }
}
