// Copyright 2024 ihciah. All Rights Reserved.

#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum MemType {
    Primitive,
    SimpleWrapper,
    Complex,
}

impl MemType {
    pub const fn next(self) -> Self {
        match self {
            MemType::Primitive => MemType::SimpleWrapper,
            MemType::SimpleWrapper => MemType::Complex,
            MemType::Complex => MemType::Complex,
        }
    }

    pub const fn max(self, other: Self) -> Self {
        match (self, other) {
            (MemType::Complex, _) => MemType::Complex,
            (MemType::SimpleWrapper, MemType::Complex) => MemType::Complex,
            (MemType::SimpleWrapper, _) => MemType::SimpleWrapper,
            (MemType::Primitive, r) => r,
        }
    }
}

#[macro_export]
macro_rules! max_mem_type {
    ($($ty:ty),*) => {
        $crate::MemType::Primitive$(.max(<$ty as $crate::ToRef>::MEM_TYPE))*
    };
}

pub struct Writer {
    ptr: *mut u8,
}

impl Writer {
    /// # Safety
    /// The pointer must be valid, and it must has enough capacity.
    #[inline]
    pub unsafe fn new(ptr: *mut u8) -> Self {
        Writer { ptr }
    }

    unsafe fn put<T>(&mut self, data: T) {
        self.ptr.cast::<T>().write_unaligned(data);
        self.ptr = self.ptr.add(std::mem::size_of::<T>());
    }

    unsafe fn reserve(&mut self, len: usize) -> Writer {
        let fork = Writer { ptr: self.ptr };
        self.ptr = self.ptr.add(len);
        fork
    }

    fn as_ptr(&self) -> *const u8 {
        self.ptr.cast()
    }
}

pub trait ToRef {
    const MEM_TYPE: MemType;

    type Ref;
    fn to_size(&self, acc: &mut usize);
    fn to_ref(&self, buffer: &mut Writer) -> Self::Ref;

    #[inline]
    fn calc_size(&self) -> usize {
        let mut size = 0;
        self.to_size(&mut size);
        size
    }
    #[inline]
    fn calc_ref(&self) -> (Vec<u8>, Self::Ref) {
        if matches!(Self::MEM_TYPE, MemType::Complex) {
            let size = self.calc_size();
            let mut buffer = Vec::with_capacity(size);
            let ref_ = self.to_ref(&mut unsafe { Writer::new(buffer.as_ptr() as _) });
            unsafe { buffer.set_len(size) };
            (buffer, ref_)
        } else {
            let buffer = Vec::new();
            let ref_ = self.to_ref(&mut unsafe { Writer::new(buffer.as_ptr() as _) });
            (buffer, ref_)
        }
    }
}

impl<T: ToRef> ToRef for &T {
    const MEM_TYPE: MemType = T::MEM_TYPE;
    type Ref = T::Ref;

    #[inline]
    fn to_size(&self, acc: &mut usize) {
        (**self).to_size(acc)
    }

    #[inline]
    fn to_ref(&self, buffer: &mut Writer) -> Self::Ref {
        (**self).to_ref(buffer)
    }
}

pub trait FromRef {
    type Ref;
    fn from_ref(ref_: &Self::Ref) -> Self;
}

#[derive(Copy, Clone, Debug)]
#[repr(C)]
pub struct DataView {
    ptr: *const (),
    len: usize,
}

impl DataView {
    #[inline]
    fn new<T>(mut ptr: *const T, len: usize) -> Self {
        if len == 0 {
            // prevent passing NonNull::dangling() to Go when len == 0
            ptr = std::ptr::null();
        }
        Self {
            ptr: ptr.cast(),
            len,
        }
    }
}

#[derive(Copy, Clone, Debug)]
#[repr(transparent)]
pub struct ListRef(DataView);

// Owned to Ref
// Vec<T> -> ListRef
impl<T: ToRef> ToRef for Vec<T> {
    const MEM_TYPE: MemType = T::MEM_TYPE.next();
    type Ref = ListRef;

    fn to_size(&self, acc: &mut usize) {
        if matches!(Self::MEM_TYPE, MemType::Complex) {
            *acc += self.len() * std::mem::size_of::<T::Ref>();
            self.iter().for_each(|elem| elem.to_size(acc));
        }
    }

    fn to_ref(&self, writer: &mut Writer) -> Self::Ref {
        let mut data = ListRef(DataView::new(self.as_ptr(), self.len()));

        if matches!(Self::MEM_TYPE, MemType::Complex) && !self.is_empty() {
            // prevent passing NonNull::dangling() to Go when self.is_empty()
            data.0.ptr = writer.as_ptr().cast();
            unsafe {
                let mut children = writer.reserve(self.len() * std::mem::size_of::<T::Ref>());
                self.iter()
                    .for_each(|elem| children.put(ToRef::to_ref(elem, writer)));
            }
        }
        data
    }
}

impl<T: FromRef> FromRef for Vec<T> {
    type Ref = ListRef;

    fn from_ref(ref_: &Self::Ref) -> Self {
        if ref_.0.len == 0 {
            return Vec::new();
        }
        let slice = unsafe { std::slice::from_raw_parts(ref_.0.ptr.cast(), ref_.0.len) };
        slice.iter().map(FromRef::from_ref).collect()
    }
}

// Owned to Ref
// Option<T> -> ListRef
impl<T: ToRef> ToRef for Option<T> {
    const MEM_TYPE: MemType = T::MEM_TYPE.next();
    type Ref = ListRef;

    fn to_size(&self, acc: &mut usize) {
        if matches!(Self::MEM_TYPE, MemType::Complex) {
            *acc += self.as_slice().len() * std::mem::size_of::<T::Ref>();
            self.iter().for_each(|elem| elem.to_size(acc));
        }
    }

    fn to_ref(&self, writer: &mut Writer) -> Self::Ref {
        let slice = self.as_slice();
        let mut data = ListRef(DataView::new(slice.as_ptr(), slice.len()));

        if matches!(Self::MEM_TYPE, MemType::Complex) && !slice.is_empty() {
            // prevent passing NonNull::dangling() to Go when slice.is_empty()
            data.0.ptr = writer.as_ptr().cast();
            unsafe {
                let mut children = writer.reserve(slice.len() * std::mem::size_of::<T::Ref>());
                self.iter()
                    .for_each(|elem| children.put(ToRef::to_ref(elem, writer)));
            }
        }
        data
    }
}

impl<T: FromRef> FromRef for Option<T> {
    type Ref = ListRef;

    fn from_ref(ref_: &Self::Ref) -> Self {
        if ref_.0.len == 0 {
            return None;
        }
        let slice = unsafe { std::slice::from_raw_parts(ref_.0.ptr.cast(), ref_.0.len) };
        slice.iter().map(FromRef::from_ref).next()
    }
}

#[derive(Copy, Clone, Debug)]
#[repr(transparent)]
pub struct StringRef(DataView);

impl ToRef for String {
    const MEM_TYPE: MemType = MemType::SimpleWrapper;
    type Ref = StringRef;

    #[inline]
    fn to_size(&self, _: &mut usize) {}

    #[inline]
    fn to_ref(&self, _: &mut Writer) -> Self::Ref {
        StringRef(DataView::new(self.as_ptr(), self.len()))
    }
}

impl FromRef for String {
    type Ref = StringRef;

    fn from_ref(ref_: &Self::Ref) -> Self {
        if ref_.0.len == 0 {
            return String::new();
        }
        let slice = unsafe { std::slice::from_raw_parts(ref_.0.ptr.cast(), ref_.0.len) };
        String::from_utf8_lossy(slice).into_owned()
    }
}

macro_rules! primitive_impl {
    ($(($ty:ty, $c:literal, $go:literal, $conv:literal)),*) => {
        $(
            impl ToRef for $ty {
                const MEM_TYPE: MemType = MemType::Primitive;
                type Ref = $ty;

                #[inline]
                fn to_size(&self, _: &mut usize) {}

                #[inline]
                fn to_ref(&self, _: &mut Writer) -> Self::Ref {
                    *self
                }
            }

            impl FromRef for $ty {
                type Ref = $ty;

                fn from_ref(ref_: &Self::Ref) -> Self {
                    *ref_
                }
            }
        )*
    };
}

// The impl list comes from the shared primitive table definition, so the
// impls and the table always cover the same set of types.
with_primitives!(primitive_impl);

macro_rules! tuple_impl {
    (($ty:ident, $name:tt)) => {
        tuple_impl!(@# ($ty, $name));
    };
    ($(($ty:ident, $name:tt)),+) => {
        tuple_impl!(@# $(($ty, $name)),*);
        tuple_impl!(@! [$(($ty, $name))*]);
    };
    (@# $(($ty:ident, $name:tt)),*) => {
        impl<$($ty,)*> ToRef for ($($ty,)*) where $($ty:ToRef,)* {
            const MEM_TYPE: MemType = MemType::Primitive$(.max($ty::MEM_TYPE))*;
            type Ref = ($($ty::Ref,)*);

            fn to_size(&self, acc: &mut usize) {
                $(self.$name.to_size(acc);)*
            }

            fn to_ref(&self, buffer: &mut Writer) -> Self::Ref {
                (
                    $(self.$name.to_ref(buffer),)*
                )
            }
        }
    };
    (@! [] ($ty_l:ident, $name_l:tt) $(($ty:ident, $name:tt))*) => {
        tuple_impl!(@~ [$(($ty, $name))*]);
    };
    (@! [($ty_f:ident, $name_f:tt) $(($ty:ident, $name:tt))*] $(($ty_r:ident, $name_r:tt))*) => {
        tuple_impl!(@! [$(($ty, $name))*] ($ty_f, $name_f) $(($ty_r, $name_r))*);
    };
    (@~ [] $(($ty:ident, $name:tt))*) => {
        tuple_impl!($(($ty, $name)),*);
    };
    (@~ [($ty_f:ident, $name_f:tt) $(($ty:ident, $name:tt))*] $(($ty_r:ident, $name_r:tt))*) => {
        tuple_impl!(@~ [$(($ty, $name))*] ($ty_f, $name_f) $(($ty_r, $name_r))*);
    };
}

tuple_impl!(
    (T1, 0),
    (T2, 1),
    (T3, 2),
    (T4, 3),
    (T5, 4),
    (T6, 5),
    (T7, 6),
    (T8, 7),
    (T9, 8),
    (T10, 9),
    (T11, 10),
    (T12, 11),
    (T13, 12),
    (T14, 13),
    (T15, 14),
    (T16, 15)
);

#[inline]
fn copy_item<T>(buf: &mut Writer, item: T) {
    unsafe { buf.put(item) };
}

trait CopyTuple {
    fn tuple_copy_to(self, buf: &mut Writer);
}

macro_rules! copy_tuple {
    (($ty:ident, $name:tt)) => {
        copy_tuple!(@# ($ty, $name));
    };
    ($(($ty:ident, $name:tt)),+) => {
        copy_tuple!(@# $(($ty, $name)),*);
        copy_tuple!(@! [$(($ty, $name))*]);
    };
    (@# $(($ty:ident, $name:tt)),*) => {
        impl<$($ty,)*> CopyTuple for ($($ty,)*) {
            fn tuple_copy_to(self, buf: &mut Writer) {
                $(copy_item(buf, self.$name);)*
            }
        }
    };
    (@! [] ($ty_l:ident, $name_l:tt) $(($ty:ident, $name:tt))*) => {
        copy_tuple!(@~ [$(($ty, $name))*]);
    };
    (@! [($ty_f:ident, $name_f:tt) $(($ty:ident, $name:tt))*] $(($ty_r:ident, $name_r:tt))*) => {
        copy_tuple!(@! [$(($ty, $name))*] ($ty_f, $name_f) $(($ty_r, $name_r))*);
    };
    (@~ [] $(($ty:ident, $name:tt))*) => {
        copy_tuple!($(($ty, $name)),*);
    };
    (@~ [($ty_f:ident, $name_f:tt) $(($ty:ident, $name:tt))*] $(($ty_r:ident, $name_r:tt))*) => {
        copy_tuple!(@~ [$(($ty, $name))*] ($ty_f, $name_f) $(($ty_r, $name_r))*);
    };
}

copy_tuple!(
    (T1, 0),
    (T2, 1),
    (T3, 2),
    (T4, 3),
    (T5, 4),
    (T6, 5),
    (T7, 6),
    (T8, 7),
    (T9, 8),
    (T10, 9),
    (T11, 10),
    (T12, 11),
    (T13, 12),
    (T14, 13),
    (T15, 14),
    (T16, 15)
);

pub struct CopyStruct<T>(pub T);

macro_rules! copy_struct_for_tuple {
    (($ty:ident, $name:tt)) => {
        copy_struct_for_tuple!(@# ($ty, $name));
    };
    ($(($ty:ident, $name:tt)),+) => {
        copy_struct_for_tuple!(@# $(($ty, $name)),*);
        copy_struct_for_tuple!(@! [$(($ty, $name))*]);
    };
    (@# $(($ty:ident, $name:tt)),*) => {
        impl<$($ty,)*> ToRef for CopyStruct<($($ty,)*)> where $($ty:ToRef,)* {
            // Complex since we need buffer
            const MEM_TYPE: MemType = MemType::Complex;
            type Ref = *const u8;

            fn to_size(&self, acc: &mut usize) {
                if matches!(MemType::Primitive$(.max($ty::MEM_TYPE))*, MemType::Complex) {
                    $(self.0.$name.to_size(acc);)*
                }
                *acc += (0 $(+::std::mem::size_of::<$ty::Ref>())*);
            }

            fn to_ref(&self, buffer: &mut Writer) -> Self::Ref {
                let r = ($(self.0.$name.to_ref(buffer),)*);
                let ptr = buffer.ptr as *const u8;
                r.tuple_copy_to(buffer);
                ptr
            }
        }
    };
    (@! [] ($ty_l:ident, $name_l:tt) $(($ty:ident, $name:tt))*) => {
        copy_struct_for_tuple!(@~ [$(($ty, $name))*]);
    };
    (@! [($ty_f:ident, $name_f:tt) $(($ty:ident, $name:tt))*] $(($ty_r:ident, $name_r:tt))*) => {
        copy_struct_for_tuple!(@! [$(($ty, $name))*] ($ty_f, $name_f) $(($ty_r, $name_r))*);
    };
    (@~ [] $(($ty:ident, $name:tt))*) => {
        copy_struct_for_tuple!($(($ty, $name)),*);
    };
    (@~ [($ty_f:ident, $name_f:tt) $(($ty:ident, $name:tt))*] $(($ty_r:ident, $name_r:tt))*) => {
        copy_struct_for_tuple!(@~ [$(($ty, $name))*] ($ty_f, $name_f) $(($ty_r, $name_r))*);
    };
}

copy_struct_for_tuple!(
    (T1, 0),
    (T2, 1),
    (T3, 2),
    (T4, 3),
    (T5, 4),
    (T6, 5),
    (T7, 6),
    (T8, 7),
    (T9, 8),
    (T10, 9),
    (T11, 10),
    (T12, 11),
    (T13, 12),
    (T14, 13),
    (T15, 14),
    (T16, 15)
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mem_type_next() {
        assert!(matches!(MemType::Primitive.next(), MemType::SimpleWrapper));
        assert!(matches!(MemType::SimpleWrapper.next(), MemType::Complex));
        assert!(matches!(MemType::Complex.next(), MemType::Complex));
    }

    #[test]
    fn mem_type_max() {
        assert!(matches!(
            MemType::Primitive.max(MemType::Primitive),
            MemType::Primitive
        ));
        assert!(matches!(
            MemType::Primitive.max(MemType::SimpleWrapper),
            MemType::SimpleWrapper
        ));
        assert!(matches!(
            MemType::Primitive.max(MemType::Complex),
            MemType::Complex
        ));
        assert!(matches!(
            MemType::SimpleWrapper.max(MemType::Primitive),
            MemType::SimpleWrapper
        ));
        assert!(matches!(
            MemType::SimpleWrapper.max(MemType::SimpleWrapper),
            MemType::SimpleWrapper
        ));
        assert!(matches!(
            MemType::SimpleWrapper.max(MemType::Complex),
            MemType::Complex
        ));
        assert!(matches!(
            MemType::Complex.max(MemType::Primitive),
            MemType::Complex
        ));
    }

    #[test]
    fn max_mem_type_macro() {
        assert!(matches!(crate::max_mem_type!(u32, u64), MemType::Primitive));
        assert!(matches!(
            crate::max_mem_type!(u32, String),
            MemType::SimpleWrapper
        ));
        assert!(matches!(
            crate::max_mem_type!(String, Vec<String>),
            MemType::Complex
        ));
    }

    #[test]
    fn primitive_roundtrip() {
        macro_rules! case {
            ($($ty:ty: $v:expr),*) => { $({
                let v: $ty = $v;
                assert!(matches!(<$ty as ToRef>::MEM_TYPE, MemType::Primitive));
                let (buf, r) = v.calc_ref();
                assert!(buf.is_empty());
                assert_eq!(<$ty as FromRef>::from_ref(&r), v);
            })* };
        }
        case!(
            u8: 1, u16: 2, u32: 3, u64: 4, usize: 5,
            i8: -1, i16: -2, i32: -3, i64: -4, isize: -5,
            f32: 1.5, f64: -2.5, bool: true, char: 'x'
        );
    }

    #[test]
    fn string_roundtrip() {
        let s = "hello rust2go".to_string();
        assert!(matches!(
            <String as ToRef>::MEM_TYPE,
            MemType::SimpleWrapper
        ));
        let (buf, r) = s.calc_ref();
        assert!(buf.is_empty());
        assert_eq!(String::from_ref(&r), s);

        // empty string -> null ptr
        let empty = String::new();
        let (_, r) = empty.calc_ref();
        assert!(r.0.ptr.is_null());
        assert_eq!(r.0.len, 0);
        assert_eq!(String::from_ref(&r), String::new());
    }

    #[test]
    fn ref_to_ref() {
        let s = "hello".to_string();
        let rs = &s;
        let (_, r) = rs.calc_ref();
        assert_eq!(String::from_ref(&r), s);
    }

    #[test]
    fn vec_primitive_roundtrip() {
        let v = vec![1u32, 2, 3, 4];
        assert!(matches!(
            <Vec<u32> as ToRef>::MEM_TYPE,
            MemType::SimpleWrapper
        ));
        let (buf, r) = v.calc_ref();
        assert!(buf.is_empty());
        assert_eq!(r.0.len, 4);
        assert_eq!(Vec::<u32>::from_ref(&r), v);

        // empty vec -> null ptr
        let empty: Vec<u32> = Vec::new();
        let (_, r) = empty.calc_ref();
        assert!(r.0.ptr.is_null());
        assert_eq!(r.0.len, 0);
        assert_eq!(Vec::<u32>::from_ref(&r), empty);
    }

    #[test]
    fn vec_string_roundtrip() {
        let v = vec!["a".to_string(), "bb".to_string(), "ccc".to_string()];
        assert!(matches!(<Vec<String> as ToRef>::MEM_TYPE, MemType::Complex));
        let size = v.calc_size();
        assert_eq!(size, 3 * std::mem::size_of::<StringRef>());
        let (buf, r) = v.calc_ref();
        assert_eq!(buf.len(), size);
        assert_eq!(Vec::<String>::from_ref(&r), v);

        // empty complex vec -> null ptr
        let empty: Vec<String> = Vec::new();
        let (_, r) = empty.calc_ref();
        assert!(r.0.ptr.is_null());
        assert_eq!(Vec::<String>::from_ref(&r), empty);
    }

    #[test]
    fn vec_vec_roundtrip() {
        let v = vec![vec![1u32, 2], vec![], vec![3, 4, 5]];
        assert!(matches!(
            <Vec<Vec<u32>> as ToRef>::MEM_TYPE,
            MemType::Complex
        ));
        let (buf, r) = v.calc_ref();
        assert_eq!(buf.len(), v.calc_size());
        assert_eq!(Vec::<Vec<u32>>::from_ref(&r), v);
    }

    #[test]
    fn option_roundtrip() {
        let some = Some("hello".to_string());
        assert!(matches!(
            <Option<String> as ToRef>::MEM_TYPE,
            MemType::Complex
        ));
        let (buf, r) = some.calc_ref();
        assert_eq!(buf.len(), std::mem::size_of::<StringRef>());
        assert_eq!(Option::<String>::from_ref(&r), some);

        // None -> null ptr
        let none: Option<String> = None;
        let (_, r) = none.calc_ref();
        assert!(r.0.ptr.is_null());
        assert_eq!(Option::<String>::from_ref(&r), none);

        // Option of primitive is a simple wrapper
        let some = Some(42u64);
        assert!(matches!(
            <Option<u64> as ToRef>::MEM_TYPE,
            MemType::SimpleWrapper
        ));
        let (buf, r) = some.calc_ref();
        assert!(buf.is_empty());
        assert_eq!(Option::<u64>::from_ref(&r), some);

        let none: Option<u64> = None;
        let (_, r) = none.calc_ref();
        assert!(r.0.ptr.is_null());
        assert_eq!(Option::<u64>::from_ref(&r), none);
    }

    #[test]
    fn tuple_mem_type() {
        assert!(matches!(
            <(u32, u64) as ToRef>::MEM_TYPE,
            MemType::Primitive
        ));
        assert!(matches!(
            <(u32, String) as ToRef>::MEM_TYPE,
            MemType::SimpleWrapper
        ));
        assert!(matches!(
            <(u32, Vec<String>) as ToRef>::MEM_TYPE,
            MemType::Complex
        ));
    }

    #[test]
    fn tuple_to_ref() {
        let t = (1u32, "two".to_string());
        let (buf, r) = t.calc_ref();
        assert!(buf.is_empty());
        assert_eq!(r.0, 1u32);
        assert_eq!(String::from_ref(&r.1), "two");
    }

    #[test]
    fn copy_struct_primitive() {
        let s = CopyStruct((1u32, 2u64));
        assert!(matches!(
            <CopyStruct<(u32, u64)> as ToRef>::MEM_TYPE,
            MemType::Complex
        ));
        let size = s.calc_size();
        assert_eq!(
            size,
            std::mem::size_of::<u32>() + std::mem::size_of::<u64>()
        );
        let (buf, ptr) = s.calc_ref();
        assert_eq!(buf.len(), size);
        assert!(!ptr.is_null());
        let a = unsafe { std::ptr::read_unaligned(ptr as *const u32) };
        let b = unsafe { std::ptr::read_unaligned(ptr.add(4) as *const u64) };
        assert_eq!((a, b), (1, 2));
    }

    #[test]
    fn copy_struct_with_string() {
        let s = CopyStruct(("hello".to_string(), 7u32));
        let size = s.calc_size();
        assert_eq!(
            size,
            std::mem::size_of::<StringRef>() + std::mem::size_of::<u32>()
        );
        let (buf, ptr) = s.calc_ref();
        assert_eq!(buf.len(), size);
        assert!(!ptr.is_null());
        let sr = unsafe { std::ptr::read_unaligned(ptr as *const StringRef) };
        assert_eq!(String::from_ref(&sr), "hello");
        let n = unsafe {
            std::ptr::read_unaligned(ptr.add(std::mem::size_of::<StringRef>()) as *const u32)
        };
        assert_eq!(n, 7);
    }

    #[test]
    fn writer_put_reserve() {
        let mut storage = [0u8; 16];
        let base = storage.as_ptr() as usize;
        let mut w = unsafe { Writer::new(storage.as_mut_ptr()) };
        unsafe { w.put(0x11223344u32) };
        assert_eq!(w.as_ptr() as usize - base, 4);

        let mut fork = unsafe { w.reserve(4) };
        assert_eq!(w.as_ptr() as usize - base, 8);
        unsafe { fork.put(0x55667788u32) };

        assert_eq!(
            u32::from_le_bytes(storage[0..4].try_into().unwrap()),
            0x11223344
        );
        assert_eq!(
            u32::from_le_bytes(storage[4..8].try_into().unwrap()),
            0x55667788
        );
    }
}
