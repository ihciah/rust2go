package bench

import "testing"

func BenchmarkCgo(b *testing.B) {
	for i := 0; i < b.N; i++ {
		noopCgo()
	}
}

func BenchmarkAsm(b *testing.B) {
	for i := 0; i < b.N; i++ {
		noopAsm()
	}
}

func BenchmarkAsmLocal(b *testing.B) {
	for i := 0; i < b.N; i++ {
		noopAsmLocal()
	}
}