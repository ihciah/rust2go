// Copyright 2024 ihciah. All Rights Reserved.

//go:build unix

package mem_ring

import (
	"runtime"
	"sync"
	"sync/atomic"
	"syscall"
	"testing"
	"time"
	"unsafe"

	"golang.org/x/sys/unix"
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

func TestQueueMarkStuck(t *testing.T) {
	q, _ := newTestQueue[uint64](2)
	if got := atomic.LoadUint32(q.stuckPtr); got != 0 {
		t.Fatalf("new queue stuck flag = %d, want 0", got)
	}
	q.markStuck()
	if got := atomic.LoadUint32(q.stuckPtr); got != 1 {
		t.Fatalf("stuck flag = %d after markStuck, want 1", got)
	}
}

// testFd wraps an fd so tests and cleanups can close it idempotently:
// closing the same fd number twice risks closing an unrelated reused fd.
type testFd struct {
	fd   int32
	once sync.Once
}

func (f *testFd) Close() {
	f.once.Do(func() { syscall.Close(int(f.fd)) })
}

// newFdPair returns a connected unix socketpair like the Rust side creates
// for the working/unstuck notification channels. The queue-side fd gets no
// cleanup: NewAwaiter takes ownership of it and closes it (and any fd only
// wrapped by a Notifier is leaked until the test process exits). Only the
// peer end is closed idempotently via testFd.
func newFdPair(t *testing.T) (int32, *testFd) {
	t.Helper()
	fds, err := unix.Socketpair(unix.AF_UNIX, unix.SOCK_STREAM, 0)
	if err != nil {
		t.Fatalf("socketpair: %v", err)
	}
	b := &testFd{fd: int32(fds[1])}
	t.Cleanup(b.Close)
	return int32(fds[0]), b
}

// fdQueueState keeps the ring memory and counters in one heap-allocated
// struct referenced by real pointers. Routing them through QueueMeta's
// uintptr fields would escape the GC's liveness tracking and risk the
// backing storage being reclaimed or the stack frame reused (see the
// unsafe.Pointer rules on uintptr conversions).
type fdQueueState[T any] struct {
	buf     []T
	head    uint64
	tail    uint64
	working uint32
	stuck   uint32
}

// newFdQueue builds a Queue over local memory with real socketpair fds so
// the Write/RunHandler goroutines and their stop mechanism can be tested.
// It returns the queue and the peer ends of the working/unstuck channels.
// The returned Queue keeps the state alive via its pointer fields.
func newFdQueue[T any](t *testing.T, n int) (q Queue[T], workingPeer, unstuckPeer *testFd) {
	t.Helper()
	state := &fdQueueState[T]{buf: make([]T, n)}
	workingFd, wp := newFdPair(t)
	unstuckFd, up := newFdPair(t)
	return Queue[T]{
		bufferPtr:  unsafe.Pointer(&state.buf[0]),
		bufferLen:  uintptr(n),
		headPtr:    &state.head,
		tailPtr:    &state.tail,
		workingPtr: &state.working,
		stuckPtr:   &state.stuck,
		workingFd:  workingFd,
		unstuckFd:  unstuckFd,
	}, wp, up
}

func waitStopped(t *testing.T, done <-chan struct{}) {
	t.Helper()
	select {
	case <-done:
	case <-time.After(5 * time.Second):
		buf := make([]byte, 1<<20)
		n := runtime.Stack(buf, true)
		t.Fatalf("background goroutine did not exit after Stop\ngoroutines:\n%s", buf[:n])
	}
}

// Stop must terminate the blocked flusher goroutine instead of letting it
// wait on the unstuck fd forever.
func TestWriteQueueStop(t *testing.T) {
	q, _, _ := newFdQueue[uint64](t, 8)
	wq := q.Write()
	wq.Push(42)
	wq.Stop()
	waitStopped(t, wq.Done())
	// Stop is idempotent.
	wq.Stop()
	// The item pushed before Stop stays in the ring.
	item := q.pop()
	if item == nil {
		t.Fatalf("pop = nil, want 42 (head=%d tail=%d)",
			atomic.LoadUint64(q.headPtr), atomic.LoadUint64(q.tailPtr))
	}
	if *item != 42 {
		t.Fatalf("pop = %d, want 42 (head=%d tail=%d)",
			*item, atomic.LoadUint64(q.headPtr), atomic.LoadUint64(q.tailPtr))
	}
}

// The flusher goroutine must also exit when the peer closes the unstuck
// channel without Stop being called, instead of spinning on the dead fd.
func TestWriteQueueExitsOnBrokenFd(t *testing.T) {
	q, _, unstuckPeer := newFdQueue[uint64](t, 8)
	wq := q.Write()
	// Closing the peer end makes the awaiter's next read report EOF.
	unstuckPeer.Close()
	waitStopped(t, wq.Done())
	// Closing the guard afterwards is still safe and idempotent.
	wq.Stop()
}

// The handler goroutine must also exit when the peer closes the working
// channel without Stop being called, instead of spinning on the dead fd.
func TestRunHandlerExitsOnBrokenFd(t *testing.T) {
	q, workingPeer, _ := newFdQueue[uint64](t, 8)
	rq := q.Read()
	guard := rq.RunHandler(func(uint64) {})
	// Closing the peer end makes the awaiter's next read report EOF.
	workingPeer.Close()
	waitStopped(t, guard.Done())
	// Closing the guard afterwards is still safe and idempotent.
	guard.Stop()
}

// Stop must terminate the handler goroutine blocked on the working fd.
func TestRunHandlerStop(t *testing.T) {
	q, _, _ := newFdQueue[uint64](t, 8)
	// Push before starting the handler: the first drain loop picks it up.
	if !q.push(7) {
		t.Fatal("push failed on empty queue")
	}
	rq := q.Read()
	got := make(chan uint64, 1)
	var mu sync.Mutex
	var vals []uint64
	guard := rq.RunHandler(func(v uint64) {
		mu.Lock()
		vals = append(vals, v)
		mu.Unlock()
		select {
		case got <- v:
		default:
		}
	})
	select {
	case v := <-got:
		if v != 7 {
			t.Fatalf("handler got %d, want 7", v)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("handler did not consume the pushed item")
	}
	guard.Stop()
	waitStopped(t, guard.Done())
	// Stop is idempotent.
	guard.Stop()
	mu.Lock()
	defer mu.Unlock()
	if len(vals) != 1 {
		t.Fatalf("handler called %d times, want 1: vals=%v head=%d tail=%d",
			len(vals), vals, atomic.LoadUint64(q.headPtr), atomic.LoadUint64(q.tailPtr))
	}
}

// NewQueue must wire QueueMeta pointers up the same way as manual
// construction (fds are irrelevant for push/pop and left unset).
func TestNewQueueFromMeta(t *testing.T) {
	const n = 8
	// Heap-allocated state kept alive across the QueueMeta uintptr
	// conversions; see fdQueueState for why real liveness matters here.
	state := &fdQueueState[uint64]{buf: make([]uint64, n)}
	q := NewQueue[uint64](QueueMeta{
		BufferPtr:  uintptr(unsafe.Pointer(&state.buf[0])),
		BufferLen:  n,
		HeadPtr:    uintptr(unsafe.Pointer(&state.head)),
		TailPtr:    uintptr(unsafe.Pointer(&state.tail)),
		WorkingPtr: uintptr(unsafe.Pointer(&state.working)),
		StuckPtr:   uintptr(unsafe.Pointer(&state.stuck)),
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
	if state.head != n || state.tail != n {
		t.Fatalf("head/tail = %d/%d, want %d/%d", state.head, state.tail, n, n)
	}
	runtime.KeepAlive(state)
}
