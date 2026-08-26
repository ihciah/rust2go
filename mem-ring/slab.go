// Copyright 2024 ihciah. All Rights Reserved.

package mem_ring

import (
	"sync"
	"sync/atomic"
)

// freelistEmpty is the sentinel for an empty freelist.
// It must not be a valid slot index (0 was a bug: slot 0 could never be reused).
const freelistEmpty = ^uint(0)

// Note: T should be Copy
type Item[T any] struct {
	next uint
	data T
}

type Slab[T any] struct {
	data []Item[T]
	next uint
}

type LockedSlab[T any] struct {
	slab Slab[T]
	lock sync.Mutex
}

type MultiSlab[T any] struct {
	slabs []LockedSlab[T]
	index uint32
	exp   uint
	mask  uint
}

func NewSlab[T any]() *Slab[T] {
	return &Slab[T]{
		data: make([]Item[T], 0, 1),
		next: freelistEmpty,
	}
}

func NewLockedSlab[T any]() *LockedSlab[T] {
	return &LockedSlab[T]{
		slab: *NewSlab[T](),
		lock: sync.Mutex{},
	}
}

func NewMultiSlab[T any](opts ...MultiSlabOption) *MultiSlab[T] {
	opt := MultiSlabOpt{
		SlabSizeExp: 4,
	}
	for _, op := range opts {
		op.apply(&opt)
	}

	size := 1 << opt.SlabSizeExp
	slabs := make([]LockedSlab[T], size)
	for i := 0; i < size; i++ {
		slabs[i] = *NewLockedSlab[T]()
	}
	return &MultiSlab[T]{
		slabs: slabs,
		exp:   opt.SlabSizeExp,
		mask:  1<<opt.SlabSizeExp - 1,
	}
}

type MultiSlabOpt struct {
	// bit width of slab size
	SlabSizeExp uint
}

type MultiSlabOption interface {
	apply(*MultiSlabOpt)
}

func (s *Slab[T]) Push(data T) uint {
	if s.next == freelistEmpty {
		s.data = append(s.data, Item[T]{next: freelistEmpty, data: data})
		return uint(len(s.data) - 1)
	}
	index := s.next
	item := &s.data[index]
	item.data = data
	s.next = item.next
	item.next = freelistEmpty
	return index
}

func (s *LockedSlab[T]) Push(data T) uint {
	s.lock.Lock()
	defer s.lock.Unlock()
	return s.slab.Push(data)
}

func (s *MultiSlab[T]) Push(data T) uint {
	slab_idx := uint(atomic.AddUint32(&s.index, 1)) & s.mask
	inner_idx := s.slabs[slab_idx].Push(data)
	return (inner_idx << s.exp) | slab_idx
}

func (s *Slab[T]) Pop(index uint) T {
	if index >= uint(len(s.data)) {
		panic("mem_ring: Slab.Pop index out of range")
	}
	item := &s.data[index]
	data := item.data
	var zero T
	item.data = zero // clear the reference so GC can reclaim it
	item.next = s.next
	s.next = index
	return data
}

func (s *LockedSlab[T]) Pop(index uint) T {
	s.lock.Lock()
	defer s.lock.Unlock()
	return s.slab.Pop(index)
}

func (s *MultiSlab[T]) Pop(index uint) T {
	slab_idx := index & s.mask
	inner_idx := index >> s.exp
	return s.slabs[slab_idx].Pop(inner_idx)
}
