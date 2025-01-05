package main

import (
	"fmt"
	"time"
)

type Demo struct{}

func init() {
	DemoCallImpl = Demo{}
}

func (Demo) demo_oneway(req *DemoUser) {
	fmt.Printf("[Go-oneway] Golang received name: %s, age: %d\n", req.name, req.age)
}

func (Demo) demo_check(req *DemoComplicatedRequest) DemoResponse {
	fmt.Printf("[Go-call] Golang received req: %d users\n", len(req.users))
	fmt.Printf("[Go-call] Golang returned result\n")
	return DemoResponse{pass: true}
}

func (Demo) demo_check_async(req *DemoComplicatedRequest) DemoResponse {
	fmt.Printf("[Go-call async] Golang received req, will sleep 1s\n")
	time.Sleep(1 * time.Second)
	fmt.Printf("[Go-call async] Golang returned result\n")
	return DemoResponse{pass: true}
}

func (Demo) demo_check_async_safe(req *DemoComplicatedRequest) DemoResponse {
	fmt.Printf("[Go-call async drop_safe] Golang received req, will sleep 1s\n")
	time.Sleep(1 * time.Second)
	resp := DemoResponse{pass: req.balabala[0] == 1}
	fmt.Printf("[Go-call async drop_safe] Golang returned result, pass: %v\n", req.balabala[0] == 1)
	return resp
}
