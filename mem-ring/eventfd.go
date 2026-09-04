// Copyright 2024 ihciah. All Rights Reserved.

//go:build unix

package mem_ring

import (
	"fmt"
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

// Notify writes one byte to the socketpair peer. It returns the write error,
// if any; a closed or broken fd is reported instead of being retried forever.
func (n Notifier) Notify() error {
	val := uint8(0)
	for {
		_, e := syscall.Write(int(n.fd), (*(*[1]byte)(unsafe.Pointer(&val)))[:])
		if e == unix.EINTR {
			continue
		}
		return e
	}
}

type Awaiter struct {
	buf [64]byte
	c   net.Conn
}

// NewAwaiter takes ownership of fd: the original fd is closed once the
// connection (which holds a duplicate of it) has been established.
func NewAwaiter(fd int32) (Awaiter, error) {
	f := os.NewFile(uintptr(fd), "fd")
	c, e := net.FileConn(f)
	if e != nil {
		return Awaiter{}, fmt.Errorf("mem_ring: build awaiter from fd %d: %w", fd, e)
	}
	// FileConn duplicated the fd; close the original explicitly. Otherwise
	// the os.File finalizer would close it at an arbitrary GC point, which
	// may hit a reused, unrelated fd number.
	f.Close()
	var buf [64]byte
	return Awaiter{buf, c}, nil
}

// Wait blocks until the peer notifies or the socket is closed. A closed or
// broken socket returns the read error so callers can exit instead of
// spinning on a dead fd.
func (n *Awaiter) Wait() error {
	_, e := n.c.Read(n.buf[:])
	return e
}

// Close closes the underlying connection, unblocking any pending Wait.
func (n *Awaiter) Close() error {
	return n.c.Close()
}
