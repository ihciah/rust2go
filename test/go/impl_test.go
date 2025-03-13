package main

import (
	"testing"
)

func TestPing(t *testing.T) {
	d := &Demo{}
	if result := d.ping(1995); result != 1995 {
		t.Errorf("ping(1995) = %d; want 1995", result)
	}
}

func TestLogin(t *testing.T) {
	d := &Demo{}
	tests := []struct {
		name     string
		req      *LoginRequest
		wantSucc bool
		wantMsg  string
	}{
		{
			name: "login by id success",
			req: &LoginRequest{
				user: User{
					id:   1,
					name: "",
					age:  0,
				},
				password: "test_psw",
			},
			wantSucc: true,
			wantMsg:  "login success",
		},
		{
			name: "login by username success",
			req: &LoginRequest{
				user: User{
					id:   0,
					name: "test",
					age:  0,
				},
				password: "test_psw",
			},
			wantSucc: true,
			wantMsg:  "login success",
		},
		{
			name: "invalid password",
			req: &LoginRequest{
				user: User{
					id:   1,
					name: "",
					age:  0,
				},
				password: "wrong_psw",
			},
			wantSucc: false,
			wantMsg:  "invalid password",
		},
		{
			name: "user not exist",
			req: &LoginRequest{
				user: User{
					id:   100,
					name: "",
					age:  0,
				},
				password: "test_psw",
			},
			wantSucc: false,
			wantMsg:  "user not exist",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := d.login(tt.req)
			if got.succ != tt.wantSucc {
				t.Errorf("login() succ = %v, want %v", got.succ, tt.wantSucc)
			}
			if got.message != tt.wantMsg {
				t.Errorf("login() message = %v, want %v", got.message, tt.wantMsg)
			}
			if tt.wantSucc && len(got.token) != 3 {
				t.Errorf("login() token length = %v, want 3", len(got.token))
			}
		})
	}
}

func TestAddFriends(t *testing.T) {
	d := &Demo{}
	tests := []struct {
		name    string
		req     *FriendsListRequest
		wantLen int
		wantIDs []uint32
	}{
		{
			name: "valid request",
			req: &FriendsListRequest{
				token:    []uint8{6, 6, 6},
				user_ids: []uint32{1, 2, 3},
			},
			wantLen: 3,
			wantIDs: []uint32{1, 2, 3},
		},
		{
			name: "invalid token",
			req: &FriendsListRequest{
				token:    []uint8{},
				user_ids: []uint32{1, 2, 3},
			},
			wantLen: 0,
			wantIDs: []uint32{},
		},
		{
			name: "non-existent users",
			req: &FriendsListRequest{
				token:    []uint8{6, 6, 6},
				user_ids: []uint32{100, 101, 102},
			},
			wantLen: 0,
			wantIDs: []uint32{},
		},
		{
			name: "empty user list",
			req: &FriendsListRequest{
				token:    []uint8{6, 6, 6},
				user_ids: []uint32{},
			},
			wantLen: 0,
			wantIDs: []uint32{},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := d.add_friends(tt.req)
			if len(got.users) != tt.wantLen {
				t.Errorf("add_friends() users length = %v, want %v", len(got.users), tt.wantLen)
			}
			gotIDs := make([]uint32, len(got.users))
			for i, u := range got.users {
				gotIDs[i] = u.id
			}
			if !sliceEqual(gotIDs, tt.wantIDs) {
				t.Errorf("add_friends() user IDs = %v, want %v", gotIDs, tt.wantIDs)
			}
		})
	}
}

func TestDeleteFriends(t *testing.T) {
	d := &Demo{}
	tests := []struct {
		name    string
		req     *FriendsListRequest
		wantLen int
		wantIDs []uint32
	}{
		{
			name: "valid request",
			req: &FriendsListRequest{
				token:    []uint8{6, 6, 6},
				user_ids: []uint32{1, 2, 3},
			},
			wantLen: 3,
			wantIDs: []uint32{1, 2, 3},
		},
		{
			name: "invalid token",
			req: &FriendsListRequest{
				token:    []uint8{},
				user_ids: []uint32{1, 2, 3},
			},
			wantLen: 0,
			wantIDs: []uint32{},
		},
		{
			name: "non-existent users",
			req: &FriendsListRequest{
				token:    []uint8{6, 6, 6},
				user_ids: []uint32{100, 101, 102},
			},
			wantLen: 0,
			wantIDs: []uint32{},
		},
		{
			name: "empty user list",
			req: &FriendsListRequest{
				token:    []uint8{6, 6, 6},
				user_ids: []uint32{},
			},
			wantLen: 0,
			wantIDs: []uint32{},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := d.delete_friends(tt.req)
			if len(got.users) != tt.wantLen {
				t.Errorf("delete_friends() users length = %v, want %v", len(got.users), tt.wantLen)
			}
			gotIDs := make([]uint32, len(got.users))
			for i, u := range got.users {
				gotIDs[i] = u.id
			}
			if !sliceEqual(gotIDs, tt.wantIDs) {
				t.Errorf("delete_friends() user IDs = %v, want %v", gotIDs, tt.wantIDs)
			}
		})
	}
}

func TestPMFriend(t *testing.T) {
	d := &Demo{}
	tests := []struct {
		name     string
		req      *PMFriendRequest
		wantSucc bool
		wantMsg  string
	}{
		{
			name: "valid request",
			req: &PMFriendRequest{
				token:   []uint8{6, 6, 6},
				user_id: 1,
				message: "hello",
			},
			wantSucc: true,
			wantMsg:  "send success",
		},
		{
			name: "invalid token",
			req: &PMFriendRequest{
				token:   []uint8{},
				user_id: 1,
				message: "hello",
			},
			wantSucc: false,
			wantMsg:  "invalid token",
		},
		{
			name: "non-existent user",
			req: &PMFriendRequest{
				token:   []uint8{6, 6, 6},
				user_id: 100,
				message: "hello",
			},
			wantSucc: false,
			wantMsg:  "user not exist",
		},
		{
			name: "empty message",
			req: &PMFriendRequest{
				token:   []uint8{6, 6, 6},
				user_id: 1,
				message: "",
			},
			wantSucc: true,
			wantMsg:  "send success",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := d.pm_friend(tt.req)
			if got.succ != tt.wantSucc {
				t.Errorf("pm_friend() succ = %v, want %v", got.succ, tt.wantSucc)
			}
			if got.message != tt.wantMsg {
				t.Errorf("pm_friend() message = %v, want %v", got.message, tt.wantMsg)
			}
		})
	}
}

func TestValidToken(t *testing.T) {
	tests := []struct {
		name  string
		token []uint8
		want  bool
	}{
		{
			name:  "valid token",
			token: []uint8{6, 6, 6},
			want:  true,
		},
		{
			name:  "invalid length",
			token: []uint8{6, 6},
			want:  false,
		},
		{
			name:  "invalid value",
			token: []uint8{6, 6, 7},
			want:  false,
		},
		{
			name:  "empty token",
			token: []uint8{},
			want:  false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := valid_token(tt.token); got != tt.want {
				t.Errorf("valid_token() = %v, want %v", got, tt.want)
			}
		})
	}
}

func sliceEqual(a, b []uint32) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}
