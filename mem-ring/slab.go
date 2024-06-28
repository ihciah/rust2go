package mem_ring

import "sync"

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

func NewSlab[T any]() *Slab[T] {
	return &Slab[T]{
		data: make([]Item[T], 0, 1),
		next: 0,
	}
}

func NewLockedSlab[T any]() *LockedSlab[T] {
	return &LockedSlab[T]{
		slab: *NewSlab[T](),
		lock: sync.Mutex{},
	}
}

func (s *Slab[T]) Push(data T) uint {
	if s.next == 0 {
		s.data = append(s.data, Item[T]{next: 0, data: data})
		return uint(len(s.data) - 1)
	}
	index := s.next
	item := &s.data[index]
	item.data = data
	s.next = item.next
	item.next = 0
	return index
}

func (s *LockedSlab[T]) Push(data T) uint {
	s.lock.Lock()
	defer s.lock.Unlock()
	return s.slab.Push(data)
}

func (s *Slab[T]) Pop(index uint) T {
	item := &s.data[index]
	item.next = s.next
	s.next = index
	return item.data
}

func (s *LockedSlab[T]) Pop(index uint) T {
	s.lock.Lock()
	defer s.lock.Unlock()
	return s.slab.Pop(index)
}
