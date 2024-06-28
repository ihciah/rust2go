package mem_ring

import (
	"syscall"
	"unsafe"

	"golang.org/x/sys/unix"
)

type Notifier struct {
	fd int32
}

func NewNotifier(fd int32) Notifier {
	return Notifier{fd: fd}
}

func (n Notifier) Notify() {
	val := uint64(1)
	for {
		_, e := syscall.Write(int(n.fd), (*(*[8]byte)(unsafe.Pointer(&val)))[:])
		if e == unix.EINTR {
			continue
		}
		return
	}
}

type Awaiter struct {
	fd int32
}

func NewAwaiter(fd int32) Awaiter {
	return Awaiter{fd: fd}
}

func (n Awaiter) Wait() int {
	type PollEvent struct {
		FD      int32
		Events  int16
		Revents int16
	}

	event := PollEvent{
		FD:     n.fd,
		Events: 1,
	}

	for {
		n, _, e := syscall.Syscall6(unix.SYS_PPOLL, uintptr(unsafe.Pointer(&event)), uintptr(1), uintptr(unsafe.Pointer(nil)), 0, 0, 0)
		if e == unix.EINTR {
			continue
		}
		return int(n)
	}
}
