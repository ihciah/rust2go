package main

/*
// Generated by rust2go. Please DO NOT edit this C part manually.

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

typedef struct StringRef {
  const uint8_t *ptr;
  uintptr_t len;
} StringRef;

typedef struct DemoUserRef {
  struct StringRef name;
  uint8_t age;
} DemoUserRef;

typedef struct DemoResponseRef {
  bool pass;
} DemoResponseRef;

typedef struct ListRef {
  const void *ptr;
  uintptr_t len;
} ListRef;

typedef struct DemoComplicatedRequestRef {
  struct ListRef users;
  struct ListRef balabala;
} DemoComplicatedRequestRef;

// hack from: https://stackoverflow.com/a/69904977
__attribute__((weak))
inline void DemoCall_demo_check_async_cb(const void *f_ptr, struct DemoResponseRef resp, const void *slot) {
((void (*)(struct DemoResponseRef, const void*))f_ptr)(resp, slot);
}

// hack from: https://stackoverflow.com/a/69904977
__attribute__((weak))
inline void DemoCall_demo_check_async_safe_cb(const void *f_ptr, struct DemoResponseRef resp, const void *slot) {
((void (*)(struct DemoResponseRef, const void*))f_ptr)(resp, slot);
}

typedef struct QueueMeta {
    uintptr_t buffer_ptr;
    uintptr_t buffer_len;
    uintptr_t head_ptr;
    uintptr_t tail_ptr;
    uintptr_t working_ptr;
    uintptr_t stuck_ptr;
    int32_t working_fd;
    int32_t unstuck_fd;
    } QueueMeta;
*/
import "C"
import (
	"reflect"
	"unsafe"

	mem_ring "github.com/ihciah/rust2go/mem-ring"
	"github.com/panjf2000/ants/v2"
)

var DemoCallImpl DemoCall

type DemoCall interface {
	demo_oneway(req DemoUser)
	demo_check_async(req DemoComplicatedRequest) DemoResponse
	demo_check_async_safe(req DemoComplicatedRequest) DemoResponse
}

func ringHandleDemoCall0(ptr unsafe.Pointer) (interface{}, []byte, uint) {
	return nil, nil, 0
}
func ringHandleDemoCall1(ptr unsafe.Pointer) (interface{}, []byte, uint) {
	req := *(*C.DemoComplicatedRequestRef)(ptr)
	ptr = unsafe.Pointer(uintptr(ptr) + unsafe.Sizeof(req))
	resp := DemoCallImpl.demo_check_async(newDemoComplicatedRequest(req))
	resp_ref_size := uint(unsafe.Sizeof(C.DemoResponseRef{}))
	resp_ref, buffer := cvt_ref_cap(cntDemoResponse, refDemoResponse, resp_ref_size)(&resp)
	offset := uint(len(buffer))
	buffer = append(buffer, unsafe.Slice((*byte)(unsafe.Pointer(&resp_ref)), resp_ref_size)...)
	return resp, buffer, offset
}
func ringHandleDemoCall2(ptr unsafe.Pointer) (interface{}, []byte, uint) {
	req := *(*C.DemoComplicatedRequestRef)(ptr)
	ptr = unsafe.Pointer(uintptr(ptr) + unsafe.Sizeof(req))
	resp := DemoCallImpl.demo_check_async_safe(newDemoComplicatedRequest(req))
	resp_ref_size := uint(unsafe.Sizeof(C.DemoResponseRef{}))
	resp_ref, buffer := cvt_ref_cap(cntDemoResponse, refDemoResponse, resp_ref_size)(&resp)
	offset := uint(len(buffer))
	buffer = append(buffer, unsafe.Slice((*byte)(unsafe.Pointer(&resp_ref)), resp_ref_size)...)
	return resp, buffer, offset
}

//export RingsInitDemoCall
func RingsInitDemoCall(crr, crw C.QueueMeta) {
	ringsInit(crr, crw, []func(ptr unsafe.Pointer) (interface{}, []byte, uint){ringHandleDemoCall0, ringHandleDemoCall1, ringHandleDemoCall2})
}

// An alternative impl of unsafe.String for go1.18
func unsafeString(ptr *byte, length int) string {
	sliceHeader := &reflect.SliceHeader{
		Data: uintptr(unsafe.Pointer(ptr)),
		Len:  length,
		Cap:  length,
	}
	return *(*string)(unsafe.Pointer(sliceHeader))
}

// An alternative impl of unsafe.StringData for go1.18
func unsafeStringData(s string) *byte {
	return (*byte)(unsafe.Pointer((*reflect.StringHeader)(unsafe.Pointer(&s)).Data))
}
func newString(s_ref C.StringRef) string {
	return unsafeString((*byte)(unsafe.Pointer(s_ref.ptr)), int(s_ref.len))
}
func refString(s *string, _ *[]byte) C.StringRef {
	return C.StringRef{
		ptr: (*C.uint8_t)(unsafeStringData(*s)),
		len: C.uintptr_t(len(*s)),
	}
}

func cntString(_ *string, _ *uint) [0]C.StringRef { return [0]C.StringRef{} }
func new_list_mapper[T1, T2 any](f func(T1) T2) func(C.ListRef) []T2 {
	return func(x C.ListRef) []T2 {
		input := unsafe.Slice((*T1)(unsafe.Pointer(x.ptr)), x.len)
		output := make([]T2, len(input))
		for i, v := range input {
			output[i] = f(v)
		}
		return output
	}
}
func new_list_mapper_primitive[T1, T2 any](_ func(T1) T2) func(C.ListRef) []T2 {
	return func(x C.ListRef) []T2 {
		return unsafe.Slice((*T2)(unsafe.Pointer(x.ptr)), x.len)
	}
}

// only handle non-primitive type T
func cnt_list_mapper[T, R any](f func(s *T, cnt *uint) [0]R) func(s *[]T, cnt *uint) [0]C.ListRef {
	return func(s *[]T, cnt *uint) [0]C.ListRef {
		for _, v := range *s {
			f(&v, cnt)
		}
		*cnt += uint(len(*s)) * size_of[R]()
		return [0]C.ListRef{}
	}
}

// only handle primitive type T
func cnt_list_mapper_primitive[T, R any](_ func(s *T, cnt *uint) [0]R) func(s *[]T, cnt *uint) [0]C.ListRef {
	return func(s *[]T, cnt *uint) [0]C.ListRef { return [0]C.ListRef{} }
}

// only handle non-primitive type T
func ref_list_mapper[T, R any](f func(s *T, buffer *[]byte) R) func(s *[]T, buffer *[]byte) C.ListRef {
	return func(s *[]T, buffer *[]byte) C.ListRef {
		if len(*buffer) == 0 {
			return C.ListRef{
				ptr: unsafe.Pointer(nil),
				len: C.uintptr_t(len(*s)),
			}
		}
		ret := C.ListRef{
			ptr: unsafe.Pointer(&(*buffer)[0]),
			len: C.uintptr_t(len(*s)),
		}
		children_bytes := int(size_of[R]()) * len(*s)
		children := (*buffer)[:children_bytes]
		*buffer = (*buffer)[children_bytes:]
		for _, v := range *s {
			child := f(&v, buffer)
			len := unsafe.Sizeof(child)
			copy(children, unsafe.Slice((*byte)(unsafe.Pointer(&child)), len))
			children = children[len:]
		}
		return ret
	}
}

// only handle primitive type T
func ref_list_mapper_primitive[T, R any](_ func(s *T, buffer *[]byte) R) func(s *[]T, buffer *[]byte) C.ListRef {
	return func(s *[]T, buffer *[]byte) C.ListRef {
		if len(*s) == 0 {
			return C.ListRef{
				ptr: unsafe.Pointer(nil),
				len: C.uintptr_t(0),
			}
		}
		return C.ListRef{
			ptr: unsafe.Pointer(&(*s)[0]),
			len: C.uintptr_t(len(*s)),
		}
	}
}
func size_of[T any]() uint {
	var t T
	return uint(unsafe.Sizeof(t))
}
func cvt_ref[R, CR any](cnt_f func(s *R, cnt *uint) [0]CR, ref_f func(p *R, buffer *[]byte) CR) func(p *R) (CR, []byte) {
	return func(p *R) (CR, []byte) {
		var cnt uint
		cnt_f(p, &cnt)
		buffer := make([]byte, cnt)
		return ref_f(p, &buffer), buffer
	}
}
func cvt_ref_cap[R, CR any](cnt_f func(s *R, cnt *uint) [0]CR, ref_f func(p *R, buffer *[]byte) CR, add_cap uint) func(p *R) (CR, []byte) {
	return func(p *R) (CR, []byte) {
		var cnt uint
		cnt_f(p, &cnt)
		buffer := make([]byte, cnt, cnt+add_cap)
		return ref_f(p, &buffer), buffer
	}
}

func newC_uint8_t(n C.uint8_t) uint8    { return uint8(n) }
func newC_uint16_t(n C.uint16_t) uint16 { return uint16(n) }
func newC_uint32_t(n C.uint32_t) uint32 { return uint32(n) }
func newC_uint64_t(n C.uint64_t) uint64 { return uint64(n) }
func newC_int8_t(n C.int8_t) int8       { return int8(n) }
func newC_int16_t(n C.int16_t) int16    { return int16(n) }
func newC_int32_t(n C.int32_t) int32    { return int32(n) }
func newC_int64_t(n C.int64_t) int64    { return int64(n) }
func newC_bool(n C.bool) bool           { return bool(n) }
func newC_uintptr_t(n C.uintptr_t) uint { return uint(n) }
func newC_intptr_t(n C.intptr_t) int    { return int(n) }
func newC_float(n C.float) float32      { return float32(n) }
func newC_double(n C.double) float64    { return float64(n) }

func cntC_uint8_t(_ *uint8, _ *uint) [0]C.uint8_t    { return [0]C.uint8_t{} }
func cntC_uint16_t(_ *uint16, _ *uint) [0]C.uint16_t { return [0]C.uint16_t{} }
func cntC_uint32_t(_ *uint32, _ *uint) [0]C.uint32_t { return [0]C.uint32_t{} }
func cntC_uint64_t(_ *uint64, _ *uint) [0]C.uint64_t { return [0]C.uint64_t{} }
func cntC_int8_t(_ *int8, _ *uint) [0]C.int8_t       { return [0]C.int8_t{} }
func cntC_int16_t(_ *int16, _ *uint) [0]C.int16_t    { return [0]C.int16_t{} }
func cntC_int32_t(_ *int32, _ *uint) [0]C.int32_t    { return [0]C.int32_t{} }
func cntC_int64_t(_ *int64, _ *uint) [0]C.int64_t    { return [0]C.int64_t{} }
func cntC_bool(_ *bool, _ *uint) [0]C.bool           { return [0]C.bool{} }
func cntC_uintptr_t(_ *uint, _ *uint) [0]C.uintptr_t { return [0]C.uintptr_t{} }
func cntC_intptr_t(_ *int, _ *uint) [0]C.intptr_t    { return [0]C.intptr_t{} }
func cntC_float(_ *float32, _ *uint) [0]C.float      { return [0]C.float{} }
func cntC_double(_ *float64, _ *uint) [0]C.double    { return [0]C.double{} }

func refC_uint8_t(p *uint8, _ *[]byte) C.uint8_t    { return C.uint8_t(*p) }
func refC_uint16_t(p *uint16, _ *[]byte) C.uint16_t { return C.uint16_t(*p) }
func refC_uint32_t(p *uint32, _ *[]byte) C.uint32_t { return C.uint32_t(*p) }
func refC_uint64_t(p *uint64, _ *[]byte) C.uint64_t { return C.uint64_t(*p) }
func refC_int8_t(p *int8, _ *[]byte) C.int8_t       { return C.int8_t(*p) }
func refC_int16_t(p *int16, _ *[]byte) C.int16_t    { return C.int16_t(*p) }
func refC_int32_t(p *int32, _ *[]byte) C.int32_t    { return C.int32_t(*p) }
func refC_int64_t(p *int64, _ *[]byte) C.int64_t    { return C.int64_t(*p) }
func refC_bool(p *bool, _ *[]byte) C.bool           { return C.bool(*p) }
func refC_uintptr_t(p *uint, _ *[]byte) C.uintptr_t { return C.uintptr_t(*p) }
func refC_intptr_t(p *int, _ *[]byte) C.intptr_t    { return C.intptr_t(*p) }
func refC_float(p *float32, _ *[]byte) C.float      { return C.float(*p) }
func refC_double(p *float64, _ *[]byte) C.double    { return C.double(*p) }

type DemoUser struct {
	name string
	age  uint8
}

func newDemoUser(p C.DemoUserRef) DemoUser {
	return DemoUser{
		name: newString(p.name),
		age:  newC_uint8_t(p.age),
	}
}
func cntDemoUser(s *DemoUser, cnt *uint) [0]C.DemoUserRef {
	return [0]C.DemoUserRef{}
}
func refDemoUser(p *DemoUser, buffer *[]byte) C.DemoUserRef {
	return C.DemoUserRef{
		name: refString(&p.name, buffer),
		age:  refC_uint8_t(&p.age, buffer),
	}
}

type DemoComplicatedRequest struct {
	users    []DemoUser
	balabala []uint8
}

func newDemoComplicatedRequest(p C.DemoComplicatedRequestRef) DemoComplicatedRequest {
	return DemoComplicatedRequest{
		users:    new_list_mapper(newDemoUser)(p.users),
		balabala: new_list_mapper_primitive(newC_uint8_t)(p.balabala),
	}
}
func cntDemoComplicatedRequest(s *DemoComplicatedRequest, cnt *uint) [0]C.DemoComplicatedRequestRef {
	cnt_list_mapper(cntDemoUser)(&s.users, cnt)
	return [0]C.DemoComplicatedRequestRef{}
}
func refDemoComplicatedRequest(p *DemoComplicatedRequest, buffer *[]byte) C.DemoComplicatedRequestRef {
	return C.DemoComplicatedRequestRef{
		users:    ref_list_mapper(refDemoUser)(&p.users, buffer),
		balabala: ref_list_mapper_primitive(refC_uint8_t)(&p.balabala, buffer),
	}
}

type DemoResponse struct {
	pass bool
}

func newDemoResponse(p C.DemoResponseRef) DemoResponse {
	return DemoResponse{
		pass: newC_bool(p.pass),
	}
}
func cntDemoResponse(s *DemoResponse, cnt *uint) [0]C.DemoResponseRef {
	return [0]C.DemoResponseRef{}
}
func refDemoResponse(p *DemoResponse, buffer *[]byte) C.DemoResponseRef {
	return C.DemoResponseRef{
		pass: refC_bool(&p.pass, buffer),
	}
}

func ringsInit(crr, crw C.QueueMeta, fns []func(ptr unsafe.Pointer) (interface{}, []byte, uint)) {
	const MULTIPOOL_SIZE = 8
	const SIZE_PER_POOL = -1

	type Storage struct {
		resp   interface{}
		buffer []byte
	}

	type Payload struct {
		Ptr          uint
		UserData     uint
		NextUserData uint
		CallId       uint32
		Flag         uint32
	}

	const CALL = 0b0101
	const REPLY = 0b1110
	const DROP = 0b1000

	queueMetaCvt := func(cq C.QueueMeta) mem_ring.QueueMeta {
		return mem_ring.QueueMeta{
			BufferPtr:  uintptr(cq.buffer_ptr),
			BufferLen:  uintptr(cq.buffer_len),
			HeadPtr:    uintptr(cq.head_ptr),
			TailPtr:    uintptr(cq.tail_ptr),
			WorkingPtr: uintptr(cq.working_ptr),
			StuckPtr:   uintptr(cq.stuck_ptr),
			WorkingFd:  int32(cq.working_fd),
			UnstuckFd:  int32(cq.unstuck_fd),
		}
	}

	rr := queueMetaCvt(crr)
	rw := queueMetaCvt(crw)

	rrq := mem_ring.NewQueue[Payload](rr)
	rwq := mem_ring.NewQueue[Payload](rw)

	gr := rwq.Read()
	gw := rrq.Write()

	slab := mem_ring.NewMultiSlab[Storage]()
	pool, _ := ants.NewMultiPool(MULTIPOOL_SIZE, SIZE_PER_POOL, ants.RoundRobin)

	gr.RunHandler(func(p Payload) {
		if p.Flag == CALL {
			// handle request
			pool.Submit(func() {
				resp, buffer, offset := fns[p.CallId](unsafe.Pointer(uintptr(p.Ptr)))
				if resp == nil {
					payload := Payload{
						Ptr:          0,
						UserData:     p.UserData,
						NextUserData: 0,
						CallId:       p.CallId,
						Flag:         DROP,
					}
					gw.Push(payload)
					return
				}

				// Use slab to hold reference of resp and buffer
				sid := slab.Push(Storage{
					resp,
					buffer,
				})
				payload := Payload{
					Ptr:          uint(uintptr(unsafe.Pointer(&buffer[offset]))),
					UserData:     p.UserData,
					NextUserData: sid,
					CallId:       p.CallId,
					Flag:         REPLY,
				}
				gw.Push(payload)
			})
		} else if p.Flag == DROP {
			// drop memory instantly
			slab.Pop(p.UserData)
		}
	})
}
func main() {}
