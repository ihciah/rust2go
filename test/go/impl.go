package main

import (
	"fmt"
)

type Demo struct {
	name2user map[string]*DBUser
	id2user   map[uint32]*DBUser
}

type DBUser struct {
	uid      uint32
	username string
	password string
}

func init() {
	user1 := DBUser{
		uid:      1,
		username: "test",
		password: "test_psw",
	}
	user2 := DBUser{
		uid:      2,
		username: "test2",
		password: "test2_psw",
	}
	user3 := DBUser{
		uid:      3,
		username: "test3",
		password: "test3_psw",
	}
	TestCallImpl = &Demo{
		name2user: map[string]*DBUser{
			"test":  &user1,
			"test2": &user2,
			"test3": &user3,
		},
		id2user: map[uint32]*DBUser{
			1: &user1,
			2: &user2,
			3: &user3,
		},
	}
}

func (d *Demo) ping(n uint) uint {
	return n
}

// Login with username or id.
func (d *Demo) login(req *LoginRequest) (r LoginResponse) {
	defer func() {
		if r.succ {
			fmt.Println("[go] login user success")
		} else {
			fmt.Println("[go] login user fail")
		}
	}()
	var user *DBUser
	if username := req.user.name; len(username) > 0 {
		if u, ok := d.name2user[username]; ok {
			user = u
		}
	}

	if uid := req.user.id; user == nil && uid > 0 {
		if u, ok := d.id2user[uid]; ok {
			user = u
		}
	}

	if user == nil {
		return LoginResponse{
			succ:    false,
			message: "user not exist",
			token:   []uint8{},
		}
	}
	if user.password == req.password {
		return LoginResponse{
			succ:    true,
			message: "login success",
			token:   []uint8{6, 6, 6},
		}
	}
	return LoginResponse{
		succ:    false,
		message: "invalid password",
		token:   []uint8{},
	}
}

func (d *Demo) logout(req *User) {
	fmt.Printf("[go] logout user %s\n", req.name)
}

func (d *Demo) add_friends(req *FriendsListRequest) FriendsListResponse {
	if !valid_token(req.token) {
		return FriendsListResponse{
			users: []User{},
		}
	}

	// Use a map to store processed user IDs to ensure deduplication
	processedIDs := make(map[uint32]bool)
	users := make([]User, 0, len(req.user_ids))

	for _, v := range req.user_ids {
		// Skip if this ID has already been processed
		if processedIDs[v] {
			continue
		}

		if user, ok := d.id2user[v]; ok {
			users = append(users, User{
				id:   user.uid,
				name: user.username,
				age:  0,
			})
			// Mark this ID as processed
			processedIDs[v] = true
		}
	}
	return FriendsListResponse{
		users,
	}
}
func (d *Demo) delete_friends(req *FriendsListRequest) FriendsListResponse {
	return d.add_friends(req)
}
func (d *Demo) pm_friend(req *PMFriendRequest) PMFriendResponse {
	if !valid_token(req.token) {
		return PMFriendResponse{
			succ:    false,
			message: "invalid token",
		}
	}
	if _, ok := d.id2user[req.user_id]; !ok {
		return PMFriendResponse{
			succ:    false,
			message: "user not exist",
		}
	}
	return PMFriendResponse{
		succ:    true,
		message: "send success",
	}
}

func valid_token(token []uint8) bool {
	if len(token) != 3 {
		return false
	}
	for _, v := range token {
		if v != 6 {
			return false
		}
	}
	return true
}

func (d *Demo) multi_param_test(user *User, message *string, token *[]uint8) LoginResponse {
	// Test implementation for multi-parameter function
	// This tests that all parameters are correctly passed with & prefix
	fmt.Printf("[go] multi_param_test called with user: %+v, message: %s, token: %v\n", user, *message, *token)

	return LoginResponse{
		succ:    true,
		message: fmt.Sprintf("Received user %s with message: %s", user.name, *message),
		token:   *token,
	}
}
