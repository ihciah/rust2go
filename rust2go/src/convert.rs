/// Convert between a pointer type and its ownership type.
/// # Safety
/// The impl must be correct, pointer must be stable.
pub unsafe trait RefConvertion {
    type Owned;
    fn get_ref(owned: &Self::Owned) -> Self;
    /// # Safety
    /// The pointer must be valid.
    unsafe fn get_owned(&self) -> Self::Owned;
}

macro_rules! primitive_impl {
    ($($ty:ty),*) => {
        $(
            unsafe impl RefConvertion for $ty {
                type Owned = $ty;
                fn get_ref(owned: &$ty) -> Self {
                    *owned
                }
                unsafe fn get_owned(&self) -> $ty {
                    *self
                }
            }
        )*
    };
}

primitive_impl!(u8, u16, u32, u64, usize, i8, i16, i32, i64, isize, f32, f64, bool, char);
