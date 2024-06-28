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
        let mut data = ListRef(DataView {
            ptr: self.as_ptr().cast(),
            len: self.len(),
        });

        if matches!(Self::MEM_TYPE, MemType::Complex) {
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
        StringRef(DataView {
            ptr: self.as_ptr().cast(),
            len: self.len(),
        })
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
    ($($ty:ty),*) => {
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

primitive_impl!(u8, u16, u32, u64, usize, i8, i16, i32, i64, isize, f32, f64, bool, char);

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
