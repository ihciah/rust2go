// Copyright 2024 ihciah. All Rights Reserved.

/// Mapping between a Rust primitive type and its C / Go counterparts.
#[derive(Debug)]
pub struct PrimitiveInfo {
    /// Rust type name, e.g. `u8`.
    pub rust_ident: &'static str,
    /// C type name, e.g. `uint8_t`.
    pub c_name: &'static str,
    /// Go type name, e.g. `uint8`.
    pub go_name: &'static str,
    /// Whether Go field converters (`newC_*` / `cntC_*` / `refC_*`) are
    /// generated for this type. `char` maps to `uint32_t` on the C side but
    /// has no converters of its own.
    pub has_go_converters: bool,
}

// The single list of all supported primitives. It drives both the PRIMITIVES
// table below and the ToRef/FromRef impls in convert.rs, so they cannot
// drift apart. The codegen crates (rust2go-common, and rust2go-macro through
// it) read the table as their own source of truth.
macro_rules! with_primitives {
    ($cb:ident) => {
        $cb!(
            (u8, "uint8_t", "uint", true),
            (u16, "uint16_t", "uint16", true),
            (u32, "uint32_t", "uint32", true),
            (u64, "uint64_t", "uint64", true),
            (usize, "uintptr_t", "uint", true),
            (i8, "int8_t", "int8", true),
            (i16, "int16_t", "int16", true),
            (i32, "int32_t", "int32", true),
            (i64, "int64_t", "int64", true),
            (isize, "intptr_t", "int", true),
            (f32, "float", "float32", true),
            (f64, "double", "float64", true),
            (bool, "bool", "bool", true),
            (char, "uint32_t", "rune", false)
        );
    };
}

macro_rules! primitives_table {
    ($(($rust:ident, $c:literal, $go:literal, $conv:literal)),*) => {
        /// All supported primitive types.
        pub static PRIMITIVES: &[PrimitiveInfo] = &[$(
            PrimitiveInfo {
                rust_ident: stringify!($rust),
                c_name: $c,
                go_name: $go,
                has_go_converters: $conv,
            }
        ),*];
    };
}

with_primitives!(primitives_table);

/// Look up a primitive by its Rust type name.
pub fn primitive_by_rust_ident(name: &str) -> Option<&'static PrimitiveInfo> {
    PRIMITIVES.iter().find(|p| p.rust_ident == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_lookup() {
        let info = primitive_by_rust_ident("u8").unwrap();
        assert_eq!(info.c_name, "uint8_t");
        assert_eq!(info.go_name, "uint8");
        assert!(info.has_go_converters);

        let info = primitive_by_rust_ident("f64").unwrap();
        assert_eq!(info.c_name, "double");
        assert_eq!(info.go_name, "float64");

        // char has C/Go names but no generated Go converters.
        let info = primitive_by_rust_ident("char").unwrap();
        assert_eq!(info.c_name, "uint32_t");
        assert_eq!(info.go_name, "rune");
        assert!(!info.has_go_converters);

        assert!(primitive_by_rust_ident("String").is_none());
        assert_eq!(PRIMITIVES.len(), 14);
    }

    #[test]
    fn rust_idents_match_type_names() {
        // The table is built with stringify!; make sure the idents match the
        // real type names so lookups by actual types keep working.
        macro_rules! case {
            ($($ty:ty),*) => { $({
                assert_eq!(
                    primitive_by_rust_ident(std::any::type_name::<$ty>())
                        .unwrap()
                        .rust_ident,
                    std::any::type_name::<$ty>()
                );
            })* };
        }
        case!(u8, u16, u32, u64, usize, i8, i16, i32, i64, isize, f32, f64, bool, char);
    }
}
