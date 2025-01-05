// Copyright 2024 ihciah. All Rights Reserved.

package mem_ring

import (
	"runtime"
	"time"
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

type SleepWaiter struct {
	Interval time.Duration
	Max      time.Duration
	Current  time.Duration
}

func (w *SleepWaiter) Reset(_ uint, _ bool) {
	w.Current = 0
}

func (w *SleepWaiter) Wait() bool {
	time.Sleep(w.Interval)
	w.Current += w.Interval
	return w.Current > w.Max
}
