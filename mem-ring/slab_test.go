// Copyright 2024 ihciah. All Rights Reserved.

//go:build unix

package mem_ring

import (
	"sync"
	"testing"
)

func TestSlabPushAllocatesSequentialIndexes(t *testing.T) {
	s := NewSlab[int]()
	for i := 0; i < 10; i++ {
		idx := s.Push(i)
		if idx != uint(i) {
			t.Fatalf("Push(%d) returned index %d, want %d", i, idx, i)
		}
	}
}

func TestSlabPushPopRoundTrip(t *testing.T) {
	s := NewSlab[int]()
	for i := 0; i < 10; i++ {
		s.Push(i * 7)
	}
	for i := 0; i < 10; i++ {
		if got := s.Pop(uint(i)); got != i*7 {
			t.Fatalf("Pop(%d) = %d, want %d", i, got, i*7)
		}
	}
}

// Slot 0 must be reusable: the freelist used to treat index 0 as the
// "empty" sentinel, so slot 0 was never handed out again after Pop.
func TestSlabSlotZeroReuse(t *testing.T) {
	s := NewSlab[int]()
	idx0 := s.Push(100)
	idx1 := s.Push(200)
	if idx0 != 0 || idx1 != 1 {
		t.Fatalf("initial indexes = %d, %d, want 0, 1", idx0, idx1)
	}
	if got := s.Pop(idx0); got != 100 {
		t.Fatalf("Pop(%d) = %d, want 100", idx0, got)
	}
	if idx := s.Push(300); idx != 0 {
		t.Fatalf("Push after Pop(0) reused index %d, want 0", idx)
	}
	if got := s.Pop(0); got != 300 {
		t.Fatalf("Pop(0) = %d, want 300", got)
	}
	if got := s.Pop(1); got != 200 {
		t.Fatalf("Pop(1) = %d, want 200", got)
	}
}

// Popped slots are recycled in LIFO order through the freelist.
func TestSlabFreelistReuseOrder(t *testing.T) {
	s := NewSlab[int]()
	for i := 0; i < 3; i++ {
		s.Push(i)
	}
	for i := 0; i < 3; i++ {
		s.Pop(uint(i))
	}
	// Freelist after popping 0,1,2 is 2 -> 1 -> 0.
	want := []uint{2, 1, 0}
	for i, w := range want {
		if idx := s.Push(100 + i); idx != w {
			t.Fatalf("reuse push %d got index %d, want %d", i, idx, w)
		}
	}
}

// Pop must zero the stored item so the GC can reclaim referenced objects.
func TestSlabPopClearsReference(t *testing.T) {
	s := NewSlab[*int]()
	v := 42
	idx := s.Push(&v)
	if got := s.Pop(idx); got != &v {
		t.Fatalf("Pop(%d) = %v, want %v", idx, got, &v)
	}
	if s.data[idx].data != nil {
		t.Fatalf("slot %d still holds a reference after Pop", idx)
	}
}

func TestSlabPopOutOfBoundsPanics(t *testing.T) {
	s := NewSlab[int]()
	s.Push(1)
	defer func() {
		if recover() == nil {
			t.Fatal("Pop with out-of-range index did not panic")
		}
	}()
	s.Pop(5)
}

func TestLockedSlabRoundTrip(t *testing.T) {
	s := NewLockedSlab[int]()
	var idxs [16]uint
	for i := 0; i < 16; i++ {
		idxs[i] = s.Push(i * 3)
	}
	for i := 0; i < 16; i++ {
		if got := s.Pop(idxs[i]); got != i*3 {
			t.Fatalf("Pop(%d) = %d, want %d", idxs[i], got, i*3)
		}
	}
	// Freelist is LIFO: the last popped slot (15) is reused first.
	if idx := s.Push(1000); idx != 15 {
		t.Fatalf("Push after popping all returned %d, want 15", idx)
	}
}

func TestLockedSlabConcurrentPushPop(t *testing.T) {
	s := NewLockedSlab[int]()
	const workers = 8
	const perWorker = 200
	var wg sync.WaitGroup
	errCh := make(chan string, workers)
	for w := 0; w < workers; w++ {
		wg.Add(1)
		go func(base int) {
			defer wg.Done()
			for i := 0; i < perWorker; i++ {
				idx := s.Push(base + i)
				if got := s.Pop(idx); got != base+i {
					errCh <- "value mismatch"
					return
				}
			}
		}(w * perWorker)
	}
	wg.Wait()
	close(errCh)
	for msg := range errCh {
		t.Fatal(msg)
	}
}

func TestMultiSlabRoundTrip(t *testing.T) {
	s := NewMultiSlab[int]()
	const n = 64
	idxs := make([]uint, n)
	for i := 0; i < n; i++ {
		idxs[i] = s.Push(i * 5)
	}
	for i := 0; i < n; i++ {
		if got := s.Pop(idxs[i]); got != i*5 {
			t.Fatalf("Pop(%d) = %d, want %d", idxs[i], got, i*5)
		}
	}
}

// Pushes must be spread over the inner slabs: the low `exp` bits of the
// returned index select the slab, the high bits are the inner index.
func TestMultiSlabIndexDistribution(t *testing.T) {
	s := NewMultiSlab[int]() // default exp = 4, 16 slabs
	const n = 64
	seen := make(map[uint]struct{})
	for i := 0; i < n; i++ {
		idx := s.Push(i)
		seen[idx&s.mask] = struct{}{}
		if inner := idx >> s.exp; inner != 0 && inner != 1 && inner != 2 && inner != 3 {
			t.Fatalf("inner index %d unexpectedly large after %d pushes", inner, i+1)
		}
	}
	if len(seen) < 2 {
		t.Fatalf("all %d pushes landed on a single slab", n)
	}
}

// After popping every slot of an inner slab, the next push routed to that
// slab must reuse the freed inner index (including inner index 0).
func TestMultiSlabSlotReuse(t *testing.T) {
	s := NewMultiSlab[int]()
	const n = 16
	first := make([]uint, n)
	for i := 0; i < n; i++ {
		first[i] = s.Push(i)
	}
	for i := 0; i < n; i++ {
		s.Pop(first[i])
	}
	// Round-robin routing returns to each slab in the same order; every
	// slab's freelist head is its just-freed slot, so indexes repeat.
	for i := 0; i < n; i++ {
		if idx := s.Push(1000 + i); idx != first[i] {
			t.Fatalf("reuse push %d got index %d, want %d", i, idx, first[i])
		}
	}
	for i := 0; i < n; i++ {
		if got := s.Pop(first[i]); got != 1000+i {
			t.Fatalf("Pop(%d) = %d, want %d", first[i], got, 1000+i)
		}
	}
}
