// Copyright 2024 ihciah. All Rights Reserved.

//go:build unix

package mem_ring

import (
	"io"
	"sync"
	"sync/atomic"
	"unsafe"

	"github.com/edwingeng/deque/v2"
)

// Guard controls the lifetime of the background goroutine started by
// ReadQueue.RunHandler or Queue.Write.
type Guard struct {
	stop    chan struct{}
	stopped chan struct{}
	once    sync.Once
	closer  io.Closer
}

func newGuard(closer io.Closer) *Guard {
	return &Guard{
		stop:    make(chan struct{}),
		stopped: make(chan struct{}),
		closer:  closer,
	}
}

// Stop requests the background goroutine to exit and closes the notification
// socket so a goroutine blocked in Wait wakes up. It is idempotent and does
// not wait for the goroutine to finish; use Done for that.
func (g *Guard) Stop() {
	g.once.Do(func() {
		close(g.stop)
		g.closer.Close()
	})
}

// Done returns a channel that is closed once the background goroutine has
// fully exited.
func (g *Guard) Done() <-chan struct{} {
	return g.stopped
}

func (g *Guard) exiting() bool {
	select {
	case <-g.stop:
		return true
	default:
		return false
	}
}

type QueueMeta struct {
	BufferPtr  uintptr
	BufferLen  uintptr
	HeadPtr    uintptr
	TailPtr    uintptr
	WorkingPtr uintptr
	StuckPtr   uintptr
	WorkingFd  int32
	UnstuckFd  int32
}

type Queue[T any] struct {
	bufferPtr  unsafe.Pointer
	bufferLen  uintptr
	headPtr    *uint64
	tailPtr    *uint64
	workingPtr *uint32
	stuckPtr   *uint32
	workingFd  int32
	unstuckFd  int32
}

type ReadQueue[T any] struct {
	q Queue[T]
}

type WriteQueue[T any] struct {
	q               Queue[T]
	Lock            *sync.Mutex
	pendingTasks    *deque.Deque[T]
	workingNotifier Notifier
	guard           *Guard
}

func NewQueue[T any](meta QueueMeta) Queue[T] {
	return Queue[T]{
		bufferPtr:  unsafe.Pointer(meta.BufferPtr),
		bufferLen:  meta.BufferLen,
		headPtr:    (*uint64)(unsafe.Pointer(meta.HeadPtr)),
		tailPtr:    (*uint64)(unsafe.Pointer(meta.TailPtr)),
		workingPtr: (*uint32)(unsafe.Pointer(meta.WorkingPtr)),
		stuckPtr:   (*uint32)(unsafe.Pointer(meta.StuckPtr)),
		workingFd:  meta.WorkingFd,
		unstuckFd:  meta.UnstuckFd,
	}
}

func (q *Queue[T]) push(item T) bool {
	t_size := unsafe.Sizeof(item)

	tail := atomic.LoadUint64(q.tailPtr)
	head := atomic.LoadUint64(q.headPtr)

	if tail-head == uint64(q.bufferLen) {
		return false
	}

	ptr := unsafe.Add(q.bufferPtr, uintptr(tail%uint64(q.bufferLen))*t_size)
	*(*T)(ptr) = item
	atomic.AddUint64(q.tailPtr, 1)
	return true
}

func (q *Queue[T]) pop() *T {
	var _t T
	t_size := unsafe.Sizeof(_t)

	tail := atomic.LoadUint64(q.tailPtr)
	head := atomic.LoadUint64(q.headPtr)

	if tail == head {
		return nil
	}

	ptr := unsafe.Add(q.bufferPtr, uintptr(head%uint64(q.bufferLen))*t_size)
	item := *(*T)(ptr)
	atomic.AddUint64(q.headPtr, 1)
	return &item
}

func (q *Queue[T]) isEmpty() bool {
	return atomic.LoadUint64(q.tailPtr) == atomic.LoadUint64(q.headPtr)
}

func (q *Queue[T]) isFull() bool {
	return atomic.LoadUint64(q.tailPtr)-atomic.LoadUint64(q.headPtr) == uint64(q.bufferLen)
}

func (q *Queue[T]) markWorking() {
	atomic.StoreUint32(q.workingPtr, 1)
}

func (q *Queue[T]) markUnworking() bool {
	atomic.StoreUint32(q.workingPtr, 0)
	if q.isEmpty() {
		return true
	}
	q.markWorking()
	return false
}

func (q *Queue[T]) working() bool {
	return atomic.LoadUint32(q.workingPtr) == 1
}

// markStuck tells the peer reader that this writer has pending tasks; the
// peer clears the flag and notifies the unstuck fd when it drains the ring.
func (q *Queue[T]) markStuck() {
	atomic.StoreUint32(q.stuckPtr, 1)
}

func (q Queue[T]) Read() ReadQueue[T] {
	return ReadQueue[T]{q: q}
}

func (q Queue[T]) Write() WriteQueue[T] {
	awaiter, err := NewAwaiter(q.unstuckFd)
	if err != nil {
		panic(err)
	}
	guard := newGuard(&awaiter)
	wq := WriteQueue[T]{
		q:               q,
		Lock:            &sync.Mutex{},
		pendingTasks:    deque.NewDeque[T](),
		workingNotifier: NewNotifier(q.workingFd),
		guard:           guard,
	}
	go func() {
		defer close(guard.stopped)
		for {
			if guard.exiting() {
				return
			}
			wq.Lock.Lock()
			for item, ok := wq.pendingTasks.TryPopFront(); ok; item, ok = wq.pendingTasks.TryPopFront() {
				if !wq.q.push(item) {
					wq.pendingTasks.PushFront(item)
					break
				}
			}
			if !wq.q.working() {
				wq.q.markWorking()
				// Best effort: if the peer is gone, the next Wait reports
				// the broken fd and this goroutine exits.
				_ = wq.workingNotifier.Notify()
			}
			if !wq.pendingTasks.IsEmpty() {
				wq.q.markStuck()
				if !wq.q.isFull() {
					// Unlock before continue: jumping back to the loop top
					// while holding the lock would self-deadlock.
					wq.Lock.Unlock()
					continue
				}
			}
			wq.Lock.Unlock()
			if err := awaiter.Wait(); err != nil {
				// The socket was closed by Stop or is broken; exit instead
				// of spinning on a dead fd.
				return
			}
		}
	}()
	return wq
}

// Stop shuts down the background flusher goroutine. Items still in
// pendingTasks are not flushed after Stop.
func (wq *WriteQueue[T]) Stop() {
	wq.guard.Stop()
}

// Done returns a channel that is closed once the background flusher
// goroutine has fully exited.
func (wq *WriteQueue[T]) Done() <-chan struct{} {
	return wq.guard.Done()
}

// RunHandler consumes items from the queue in a background goroutine and
// returns a Guard that stops it. While the queue is empty the goroutine
// yields the CPU through the given TinyWaiter (GoSchedWaiter by default)
// and then blocks on the working fd until the peer writer notifies.
func (rq *ReadQueue[T]) RunHandler(handler func(T), w ...TinyWaiter) *Guard {
	var waiter TinyWaiter
	if len(w) == 0 {
		waiter = &GoSchedWaiter{}
	} else {
		waiter = w[0]
	}
	awaiter, err := NewAwaiter(rq.q.workingFd)
	if err != nil {
		panic(err)
	}
	guard := newGuard(&awaiter)
	go func() {
		defer close(guard.stopped)
		rq.q.markWorking()
		var waited bool
	c:
		for {
			cnt := uint(0)
			for item := rq.q.pop(); item != nil; item = rq.q.pop() {
				handler(*item)
				cnt += 1
			}
			waiter.Reset(cnt, waited)
			waited = false
			for {
				if guard.exiting() {
					return
				}
				stop_wait := waiter.Wait()
				if !rq.q.isEmpty() || !rq.q.markUnworking() {
					continue c
				}
				if stop_wait {
					break
				}
			}

			if err := awaiter.Wait(); err != nil {
				// The socket was closed by Stop or is broken; exit instead
				// of spinning on a dead fd.
				return
			}
			if guard.exiting() {
				return
			}
			rq.q.markWorking()
			waited = true
		}
	}()
	return guard
}

func (wq *WriteQueue[T]) Push(item T) {
	wq.Lock.Lock()
	if wq.q.push(item) {
		if !wq.q.working() {
			wq.q.markWorking()
			wq.Lock.Unlock()
			_ = wq.workingNotifier.Notify()
			return
		}
	} else {
		wq.q.markStuck()
		wq.pendingTasks.PushBack(item)
	}
	wq.Lock.Unlock()
}
