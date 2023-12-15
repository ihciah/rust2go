package main

import "fmt"

type Demo struct{}

func init() {
	DemoCallImpl = Demo{}
}

func (Demo) demo_oneway(req DemoUser) {
	fmt.Printf("[oneway] Golang received name: %s, age: %d\n", req.name, req.age)
}

func (Demo) demo_check(req DemoComplicatedRequest) DemoResponse {
	fmt.Printf("[call] Golang received req\n")
	return DemoResponse{pass: true}
}

func (Demo) demo_check_async(req DemoComplicatedRequest) DemoResponse {
	fmt.Printf("[call async] Golang received req\n")
	return DemoResponse{pass: true}
}
