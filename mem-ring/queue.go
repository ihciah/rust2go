package mem_ring

import (
	"sync"
	"sync/atomic"
	"unsafe"

	"github.com/edwingeng/deque/v2"
)

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
	q               Queue[T]
	unstuckNotifier Notifier
}

type WriteQueue[T any] struct {
	q               Queue[T]
	Lock            *sync.Mutex
	pendingTasks    *deque.Deque[T]
	workingNotifier Notifier
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

func (q *Queue[T]) markStuck() {
	atomic.StoreUint32(q.stuckPtr, 1)
}

func (q *Queue[T]) markUnstuck() {
	atomic.StoreUint32(q.stuckPtr, 0)
}

func (q *Queue[T]) stuck() bool {
	return atomic.LoadUint32(q.stuckPtr) == 1
}

func (q Queue[T]) Read() ReadQueue[T] {
	unstuckNotifier := NewNotifier(q.unstuckFd)
	return ReadQueue[T]{
		q,
		unstuckNotifier,
	}
}

func (q Queue[T]) Write() WriteQueue[T] {
	awaiter := NewAwaiter(q.unstuckFd)
	wq := WriteQueue[T]{
		q:               q,
		Lock:            &sync.Mutex{},
		pendingTasks:    deque.NewDeque[T](),
		workingNotifier: NewNotifier(q.workingFd),
	}
	go func() {
		for {
			wq.Lock.Lock()
			for item, ok := wq.pendingTasks.TryPopFront(); ok; item, ok = wq.pendingTasks.TryPopFront() {
				if !wq.q.push(item) {
					wq.pendingTasks.PushFront(item)
					break
				}
			}
			if !wq.q.working() {
				wq.q.markWorking()
				wq.workingNotifier.Notify()
			}
			if !wq.pendingTasks.IsEmpty() {
				wq.q.markStuck()
				if !wq.q.isFull() {
					continue
				}
			}
			wq.Lock.Unlock()
			awaiter.Wait()
		}
	}()
	return wq
}

func (rq *ReadQueue[T]) RunHandler(handler func(T), w ...TinyWaiter) {
	// TODO: return channel-based guard
	var waiter TinyWaiter
	if len(w) == 0 {
		waiter = &GoSchedWaiter{}
	} else {
		waiter = w[0]
	}
	go func() {
		awaiter := NewAwaiter(rq.q.workingFd)
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
				stop_wait := waiter.Wait()
				if !rq.q.isEmpty() || !rq.q.markUnworking() {
					continue c
				}
				if stop_wait {
					break
				}
			}

			awaiter.Wait()
			rq.q.markWorking()
			waited = true
		}
	}()
}

func (wq *WriteQueue[T]) Push(item T) {
	wq.Lock.Lock()
	if wq.q.push(item) {
		if !wq.q.working() {
			wq.q.markWorking()
			wq.Lock.Unlock()
			wq.workingNotifier.Notify()
			return
		}
	} else {
		wq.q.markStuck()
		wq.pendingTasks.PushBack(item)
	}
	wq.Lock.Unlock()
}
