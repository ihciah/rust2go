// Copyright 2024 ihciah. All Rights Reserved.

// The unix GOOS set is spelled out instead of the `unix` build
// constraint, which Go only recognizes since 1.19 (go.mod declares 1.18).
//go:build aix || android || darwin || dragonfly || freebsd || hurd || illumos || ios || linux || netbsd || openbsd || solaris

package mem_ring

import (
	"runtime"
)

type TinyWaiter interface {
	Reset(uint, bool)
	// return true if the waiter is done
	Wait() bool
}

type GoSchedWaiter struct{}

func (w *GoSchedWaiter) Reset(_ uint, _ bool) {}
func (w *GoSchedWaiter) Wait() bool {
	runtime.Gosched()
	return true
}
