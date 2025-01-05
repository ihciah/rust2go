package main

import (
	"fmt"
)

type Demo struct{}

func init() {
	DemoCallImpl = Demo{}
}

func (Demo) demo_oneway(user *DemoUser) {
	fmt.Printf("[Go Callee] Received name: %s, age: %d, will log them by calling rust\n", user.name, user.age)
	G2RCallImpl{}.demo_log(&user.name, &user.age)
	fmt.Println("[Go Callee] done")
}

func (Demo) demo_call(user *DemoUser) DemoResponse {
	fmt.Printf("[Go Callee] Received name: %s, age: %d, will convert name by calling rust\n", user.name, user.age)
	new_name := G2RCallImpl{}.demo_convert_name(user)
	fmt.Printf("[Go Callee] Converted name: %s -> %s, age: %d\n", user.name, new_name, user.age)
	return DemoResponse{pass: new_name == "CHIHAI"}
}
