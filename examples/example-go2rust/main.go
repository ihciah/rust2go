package main

/*
// For statically link: #cgo LDFLAGS: ./librust_lib.a
// For dynamically link: #cgo LDFLAGS: -L. -lrust_lib
#cgo LDFLAGS: ./librust_lib.a

void rust_lib_init(void);
*/
import "C"
import "fmt"

func main() {
	// Install the stateful Rust implementation before any stateful g2r
	// call; calling an unregistered stateful trait aborts the process.
	C.rust_lib_init()

	user := DemoUser{
		name: "chihai",
		age:  28,
	}
	G2RCallImpl{}.demo_log(&user.name, &user.age)
	new_name := G2RCallImpl{}.demo_convert_name(&user)
	fmt.Printf("new name: %s\n", new_name)

	// Stateful g2r: the counter lives inside the registered Rust instance,
	// so the value increases across calls.
	var one uint64 = 1
	fmt.Printf("counter after incr(1): %d\n", G2RStatefulCallImpl{}.incr(&one))
	fmt.Printf("counter after incr(1): %d\n", G2RStatefulCallImpl{}.incr(&one))
	fmt.Printf("counter current: %d\n", G2RStatefulCallImpl{}.current())
}
