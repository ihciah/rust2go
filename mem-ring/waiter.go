// Copyright 2024 ihciah. All Rights Reserved.

//go:build unix

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
