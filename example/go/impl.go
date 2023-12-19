package main

type Demo struct{}

func init() {
	DemoCallImpl = Demo{}
}

func (Demo) demo_oneway(req DemoUser) {
}

func (Demo) demo_check(req DemoComplicatedRequest) DemoResponse {
	return DemoResponse{pass: true}
}

func (Demo) demo_check_async(req DemoComplicatedRequest) DemoResponse {
	return DemoResponse{pass: true}
}
