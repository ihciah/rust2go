package main

/*
// For statically link: #cgo LDFLAGS: ./librust_lib.a
// For dynamically link: #cgo LDFLAGS: -L. -lrust_lib
#cgo LDFLAGS: ./librust_lib.a
*/
import "C"
import "fmt"

func main() {
	user := DemoUser{
		name: "chihai",
		age:  28,
	}
	G2RCallImpl{}.demo_log(&user.name, &user.age)
	new_name := G2RCallImpl{}.demo_convert_name(&user)
	fmt.Printf("new name: %s", new_name)
}
