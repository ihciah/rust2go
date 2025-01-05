package bench

// void noop() {}
import "C"
import (
	"github.com/ihciah/rust2go/asmcall"
	"github.com/ihciah/rust2go/cgocall"
)

func noopCgo() {
	cgocall.CallFuncG0P0(C.noop)
}

func noopAsm() {
	asmcall.CallFuncG0P0(C.noop)
}

func noopAsmLocal() {
	asmcall.CallFuncP0(C.noop)
}
