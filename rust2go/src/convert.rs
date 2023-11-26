/// Convert a pointer type to its ownership type.
/// # Safety
/// The impl must be correct.
pub unsafe trait GetOwned<T> {
    /// # Safety
    /// The pointer must be valid.
    unsafe fn get_owned(&self) -> T;
}

/// Convert a ownership type to its pointer type.
/// # Safety
/// The impl must be correct, pointer must be stable.
pub unsafe trait GetRef<T> {
    fn get_ref(&self) -> T;
}

macro_rules! primitive_impl {
    ($($ty:ty),*) => {
        $(
            unsafe impl GetOwned<$ty> for $ty {
                unsafe fn get_owned(&self) -> $ty {
                    *self
                }
            }
            unsafe impl GetRef<$ty> for $ty {
                fn get_ref(&self) -> $ty {
                    *self
                }
            }
        )*
    };
}

primitive_impl!(u8, u16, u32, u64, usize, i8, i16, i32, i64, isize, f32, f64, bool, char);
