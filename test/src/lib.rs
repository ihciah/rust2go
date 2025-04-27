mod user;

#[cfg(test)]
mod tests {
    use super::user::*;

    #[test]
    fn echo() {
        assert_eq!(TestCallImpl::ping(1995), 1995);
    }

    #[test]
    fn ping_zero() {
        assert_eq!(TestCallImpl::ping(0), 0);
    }

    #[test]
    fn ping_max() {
        let max = usize::MAX;
        assert_eq!(TestCallImpl::ping(max), max);
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
        // Basic login tests
        assert!(login!(1, String::new(), "test_psw", "login success"));
        assert!(login!(0, "test".to_string(), "test_psw", "login success"));
        assert!(!login!(1, String::new(), "wrong_psw", "invalid password"));
        assert!(!login!(100, String::new(), "", "user not exist"));

        // Edge cases
        assert!(!login!(0, "".to_string(), "", "user not exist")); // Empty username and password
        assert!(!login!(0, "test".to_string(), "", "invalid password")); // Empty password
        assert!(!login!(0, "".to_string(), "test_psw", "user not exist")); // Empty username

        // Logout test
        TestCallImpl::logout(&User {
            id: 1,
            name: String::new(),
            age: 0,
        });
    }

    #[monoio::test(timer_enabled = true)]
    async fn login_token() {
        let success_resp = TestCallImpl::login(&LoginRequest {
            user: User {
                id: 1,
                name: String::new(),
                age: 0,
            },
            password: "test_psw".to_string(),
        });
        assert!(success_resp.succ);
        assert!(!success_resp.token.is_empty());

        let failure_resp = TestCallImpl::login(&LoginRequest {
            user: User {
                id: 1,
                name: String::new(),
                age: 0,
            },
            password: "wrong_psw".to_string(),
        });
        assert!(!failure_resp.succ);
        assert!(failure_resp.token.is_empty());
    }

    #[monoio::test(timer_enabled = true)]
    async fn add_friends() {
        // Invalid token test
        let wrong_token_req = unsafe {
            TestCallImpl::add_friends(&FriendsListRequest {
                token: vec![],
                user_ids: vec![],
            })
        }
        .await;
        assert!(wrong_token_req.users.is_empty());

        // Invalid user IDs test
        let no_valid_users_req = unsafe {
            TestCallImpl::add_friends(&FriendsListRequest {
                token: vec![6, 6, 6],
                user_ids: vec![100, 101, 102],
            })
        }
        .await;
        assert!(no_valid_users_req.users.is_empty());

        // Valid request test
        let valid_req = unsafe {
            TestCallImpl::add_friends(&FriendsListRequest {
                token: vec![6, 6, 6],
                user_ids: vec![1, 2, 3],
            })
        }
        .await;
        assert_eq!(
            valid_req.users.iter().map(|u| u.id).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );

        // Duplicate user IDs test
        let duplicate_req = unsafe {
            TestCallImpl::add_friends(&FriendsListRequest {
                token: vec![6, 6, 6],
                user_ids: vec![1, 1, 1], // Duplicate user IDs
            })
        }
        .await;
        assert_eq!(
            duplicate_req.users.iter().map(|u| u.id).collect::<Vec<_>>(),
            vec![1]
        );

        // Empty user list test
        let empty_req = unsafe {
            TestCallImpl::add_friends(&FriendsListRequest {
                token: vec![6, 6, 6],
                user_ids: vec![],
            })
        }
        .await;
        assert!(empty_req.users.is_empty());

        // Test with maximum number of users
        let max_users_req = unsafe {
            TestCallImpl::add_friends(&FriendsListRequest {
                token: vec![6, 6, 6],
                user_ids: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10], // Test with 10 users
            })
        }
        .await;
        assert_eq!(
            max_users_req.users.iter().map(|u| u.id).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }

    #[monoio::test(timer_enabled = true)]
    async fn delete_friends() {
        // Add some friends first
        unsafe {
            TestCallImpl::add_friends(&FriendsListRequest {
                token: vec![6, 6, 6],
                user_ids: vec![1, 2, 3],
            })
        }
        .await;

        // Normal deletion test
        let valid_req = TestCallImpl::delete_friends(FriendsListRequest {
            token: vec![6, 6, 6],
            user_ids: vec![1, 3],
        })
        .await;
        assert_eq!(
            valid_req.users.iter().map(|u| u.id).collect::<Vec<_>>(),
            vec![1, 3]
        );

        // Delete non-existent users test
        let non_exist_req = TestCallImpl::delete_friends(FriendsListRequest {
            token: vec![6, 6, 6],
            user_ids: vec![100, 101],
        })
        .await;
        assert!(non_exist_req.users.is_empty());

        // Delete empty list test
        let empty_req = TestCallImpl::delete_friends(FriendsListRequest {
            token: vec![6, 6, 6],
            user_ids: vec![],
        })
        .await;
        assert!(empty_req.users.is_empty());

        // Invalid token test
        let invalid_token_req = TestCallImpl::delete_friends(FriendsListRequest {
            token: vec![],
            user_ids: vec![1, 2],
        })
        .await;
        assert!(invalid_token_req.users.is_empty());

        // Test deleting all friends
        let delete_all_req = TestCallImpl::delete_friends(FriendsListRequest {
            token: vec![6, 6, 6],
            user_ids: vec![1, 2, 3],
        })
        .await;
        assert_eq!(
            delete_all_req
                .users
                .iter()
                .map(|u| u.id)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }

    #[monoio::test(timer_enabled = true)]
    async fn pm_friends() {
        // Add some friends first
        unsafe {
            TestCallImpl::add_friends(&FriendsListRequest {
                token: vec![6, 6, 6],
                user_ids: vec![1, 2, 3],
            })
        }
        .await;

        // Normal message sending test
        let (valid_req, _) = TestCallImpl::pm_friend(PMFriendRequest {
            token: vec![6, 6, 6],
            user_id: 1,
            message: "hello".into(),
        })
        .await;
        assert!(valid_req.succ);

        // Send to non-existent user test
        let (invalid_req, _) = TestCallImpl::pm_friend(PMFriendRequest {
            token: vec![6, 6, 6],
            user_id: 8,
            message: "hello".into(),
        })
        .await;
        assert!(!invalid_req.succ);

        // Invalid token test
        let (invalid_token_req, _) = TestCallImpl::pm_friend(PMFriendRequest {
            token: vec![],
            user_id: 1,
            message: "hello".into(),
        })
        .await;
        assert!(!invalid_token_req.succ);

        // Empty message test
        let (empty_msg_req, _) = TestCallImpl::pm_friend(PMFriendRequest {
            token: vec![6, 6, 6],
            user_id: 1,
            message: "".into(),
        })
        .await;
        assert!(empty_msg_req.succ);

        // Test with long message
        let long_msg = "a".repeat(1000); // Create a 1000-character message
        let (long_msg_req, _) = TestCallImpl::pm_friend(PMFriendRequest {
            token: vec![6, 6, 6],
            user_id: 1,
            message: long_msg,
        })
        .await;
        assert!(long_msg_req.succ);

        // Test sending to multiple users
        for user_id in 1..=3 {
            let (multi_user_req, _) = TestCallImpl::pm_friend(PMFriendRequest {
                token: vec![6, 6, 6],
                user_id,
                message: "test message".into(),
            })
            .await;
            assert!(multi_user_req.succ);
        }
    }

    #[monoio::test(timer_enabled = true)]
    async fn test_user_operations_sequence() {
        // Test a complete sequence of user operations
        // 1. Login
        let login_resp = TestCallImpl::login(&LoginRequest {
            user: User {
                id: 1,
                name: String::new(),
                age: 0,
            },
            password: "test_psw".to_string(),
        });
        assert!(login_resp.succ);

        // 2. Add friends
        let add_friends_resp = unsafe {
            TestCallImpl::add_friends(&FriendsListRequest {
                token: vec![6, 6, 6],
                user_ids: vec![2, 3],
            })
        }
        .await;
        assert_eq!(
            add_friends_resp
                .users
                .iter()
                .map(|u| u.id)
                .collect::<Vec<_>>(),
            vec![2, 3]
        );

        // 3. Send messages
        for user_id in 2..=3 {
            let (pm_resp, _) = TestCallImpl::pm_friend(PMFriendRequest {
                token: vec![6, 6, 6],
                user_id,
                message: "test message".into(),
            })
            .await;
            assert!(pm_resp.succ);
        }

        // 4. Delete some friends
        let delete_resp = TestCallImpl::delete_friends(FriendsListRequest {
            token: vec![6, 6, 6],
            user_ids: vec![2],
        })
        .await;
        assert_eq!(
            delete_resp.users.iter().map(|u| u.id).collect::<Vec<_>>(),
            vec![2]
        );

        // 5. Logout
        TestCallImpl::logout(&User {
            id: 1,
            name: String::new(),
            age: 0,
        });
    }
}
