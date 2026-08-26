// Copyright 2024 ihciah. All Rights Reserved.

package mem_ring

import (
	"testing"
	"unsafe"
)

// newTestQueue builds a Queue over locally allocated memory so the pure
// ring arithmetic can be tested without socketpairs/eventfd.
// The returned slice keeps the backing buffer alive.
func newTestQueue[T any](n int) (*Queue[T], []T) {
	buf := make([]T, n)
	var head, tail uint64
	var working, stuck uint32
	q := &Queue[T]{
		bufferPtr:  unsafe.Pointer(&buf[0]),
		bufferLen:  uintptr(n),
		headPtr:    &head,
		tailPtr:    &tail,
		workingPtr: &working,
		stuckPtr:   &stuck,
	}
	return q, buf
}

func TestQueueEmptyPopReturnsNil(t *testing.T) {
	q, _ := newTestQueue[uint64](4)
	if !q.isEmpty() {
		t.Fatal("new queue is not empty")
	}
	if item := q.pop(); item != nil {
		t.Fatalf("pop on empty queue = %v, want nil", *item)
	}
}

func TestQueuePushUntilFull(t *testing.T) {
	q, _ := newTestQueue[uint64](4)
	for i := uint64(0); i < 4; i++ {
		if !q.push(i) {
			t.Fatalf("push(%d) failed on non-full queue", i)
		}
	}
	if !q.isFull() {
		t.Fatal("queue not full after 4 pushes into buffer of 4")
	}
	if q.push(4) {
		t.Fatal("push succeeded on full queue")
	}
}

func TestQueuePopFIFOOrder(t *testing.T) {
	q, _ := newTestQueue[uint64](4)
	for i := uint64(0); i < 4; i++ {
		q.push(i * 11)
	}
	for i := uint64(0); i < 4; i++ {
		item := q.pop()
		if item == nil {
			t.Fatalf("pop %d returned nil", i)
		}
		if *item != i*11 {
			t.Fatalf("pop %d = %d, want %d", i, *item, i*11)
		}
	}
	if !q.isEmpty() {
		t.Fatal("queue not empty after popping all items")
	}
	if item := q.pop(); item != nil {
		t.Fatalf("pop on drained queue = %v, want nil", *item)
	}
}

// Interleaved push/pop must wrap around the buffer via modulo indexing.
func TestQueueWrapAround(t *testing.T) {
	q, _ := newTestQueue[uint64](4)
	for i := uint64(0); i < 100; i++ {
		if !q.push(i) {
			t.Fatalf("push(%d) failed", i)
		}
		item := q.pop()
		if item == nil || *item != i {
			t.Fatalf("iteration %d: pop = %v, want %d", i, item, i)
		}
	}
	if !q.isEmpty() {
		t.Fatal("queue not empty after interleaved push/pop")
	}
}

// Fill, drain partially, refill: head/tail keep advancing past bufferLen.
func TestQueueRefillAfterDrain(t *testing.T) {
	q, _ := newTestQueue[uint64](4)
	var next uint64
	push := func() {
		if !q.push(next) {
			t.Fatalf("push(%d) failed", next)
		}
		next++
	}
	popWant := func(want uint64) {
		item := q.pop()
		if item == nil || *item != want {
			t.Fatalf("pop = %v, want %d", item, want)
		}
	}
	for i := 0; i < 4; i++ {
		push()
	}
	if q.push(next) {
		t.Fatal("push succeeded on full queue")
	}
	for round := uint64(0); round < 10; round++ {
		popWant(round * 2)
		popWant(round*2 + 1)
		push()
		push()
	}
}

func TestQueueWorkingFlags(t *testing.T) {
	q, _ := newTestQueue[uint64](2)
	if q.working() {
		t.Fatal("new queue is working")
	}
	q.markWorking()
	if !q.working() {
		t.Fatal("working() false after markWorking")
	}
	// Empty queue: markUnworking clears the flag and reports done.
	if !q.markUnworking() {
		t.Fatal("markUnworking on empty queue returned false")
	}
	if q.working() {
		t.Fatal("working() true after markUnworking on empty queue")
	}
	// Non-empty queue: markUnworking re-marks working and reports not done.
	q.push(7)
	q.markWorking()
	if q.markUnworking() {
		t.Fatal("markUnworking on non-empty queue returned true")
	}
	if !q.working() {
		t.Fatal("working() false after markUnworking on non-empty queue")
	}
}

func TestQueueStuckFlags(t *testing.T) {
	q, _ := newTestQueue[uint64](2)
	if q.stuck() {
		t.Fatal("new queue is stuck")
	}
	q.markStuck()
	if !q.stuck() {
		t.Fatal("stuck() false after markStuck")
	}
	q.markUnstuck()
	if q.stuck() {
		t.Fatal("stuck() true after markUnstuck")
	}
}

// NewQueue must wire QueueMeta pointers up the same way as manual
// construction (fds are irrelevant for push/pop and left unset).
func TestNewQueueFromMeta(t *testing.T) {
	const n = 8
	buf := make([]uint64, n)
	var head, tail uint64
	var working, stuck uint32
	q := NewQueue[uint64](QueueMeta{
		BufferPtr:  uintptr(unsafe.Pointer(&buf[0])),
		BufferLen:  n,
		HeadPtr:    uintptr(unsafe.Pointer(&head)),
		TailPtr:    uintptr(unsafe.Pointer(&tail)),
		WorkingPtr: uintptr(unsafe.Pointer(&working)),
		StuckPtr:   uintptr(unsafe.Pointer(&stuck)),
	})
	for i := uint64(0); i < n; i++ {
		if !q.push(i + 100) {
			t.Fatalf("push(%d) failed", i)
		}
	}
	if !q.isFull() {
		t.Fatal("queue not full")
	}
	for i := uint64(0); i < n; i++ {
		item := q.pop()
		if item == nil || *item != i+100 {
			t.Fatalf("pop %d = %v, want %d", i, item, i+100)
		}
	}
	if head != n || tail != n {
		t.Fatalf("head/tail = %d/%d, want %d/%d", head, tail, n, n)
	}
}
