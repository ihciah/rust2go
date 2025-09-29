pub mod binding {
    #![allow(warnings)]
    rust2go::r2g_include_binding!();
}

#[derive(rust2go::R2G, Clone)]
pub struct User {
    pub id: u32,
    pub name: String,
    pub age: u8,
}

#[derive(rust2go::R2G, Clone)]
#[allow(dead_code)]
pub struct LoginRequest {
    pub user: User,
    pub password: String,
}

#[derive(rust2go::R2G, Clone)]
pub struct LoginResponse {
    pub succ: bool,
    pub message: String,
    pub token: Vec<u8>,
}

#[derive(rust2go::R2G, Clone)]
#[allow(dead_code)]
pub struct LogoutRequest {
    pub token: Vec<u8>,
    pub user_ids: Vec<u32>,
}

#[derive(rust2go::R2G, Clone)]
pub struct FriendsListRequest {
    pub token: Vec<u8>,
    pub user_ids: Vec<u32>,
}

#[derive(rust2go::R2G, Clone)]
pub struct FriendsListResponse {
    pub users: Vec<User>,
}

#[derive(rust2go::R2G, Clone)]
pub struct PMFriendRequest {
    pub user_id: u32,
    pub token: Vec<u8>,
    pub message: String,
}

#[derive(rust2go::R2G, Clone)]
pub struct PMFriendResponse {
    pub succ: bool,
    pub message: String,
}

#[allow(non_snake_case)]
#[derive(rust2go::R2G, Clone)]
pub struct PreserveStructAttrsRequest{
    pub UserId: u64,
    pub UserName: String,
}

#[allow(non_snake_case)]
#[derive(rust2go::R2G, Clone)]
pub struct PreserveStructAttrsResponse{
    pub Success: bool,
}


#[rust2go::r2g]
#[allow(clippy::ptr_arg)]
#[allow(dead_code)]
pub trait TestCall {
    #[go_pass_struct]
    fn ping(n: usize) -> usize;
    fn login(req: &LoginRequest) -> LoginResponse;
    fn logout(req: &User);
    async fn add_friends(req: &FriendsListRequest) -> FriendsListResponse;
    #[drop_safe]
    async fn delete_friends(req: FriendsListRequest) -> FriendsListResponse;
    #[drop_safe_ret]
    async fn pm_friend(req: PMFriendRequest) -> PMFriendResponse;
    #[mem_call]
    async fn multi_param_test(user: &User, message: &String, token: &Vec<u8>) -> LoginResponse;

    async fn preserve_struct_attrs_test(data: &PreserveStructAttrsRequest) -> PreserveStructAttrsResponse;
}
