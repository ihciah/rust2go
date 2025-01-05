// Copyright 2024 ihciah. All Rights Reserved.

package mem_ring

import (
	"net"
	"os"
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
	val := uint8(0)
	for {
		_, e := syscall.Write(int(n.fd), (*(*[1]byte)(unsafe.Pointer(&val)))[:])
		if e == unix.EINTR {
			continue
		}
		return
	}
}

type Awaiter struct {
	buf [64]byte
	c   net.Conn
}

func NewAwaiter(fd int32) Awaiter {
	f := os.NewFile(uintptr(fd), "fd")
	c, e := net.FileConn(f)
	if e != nil {
		panic(e)
	}
	var buf [64]byte
	return Awaiter{buf, c}
}

func (n *Awaiter) Wait() {
	n.c.Read(n.buf[:])
}
