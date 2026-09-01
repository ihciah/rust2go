// Copyright 2024 ihciah. All Rights Reserved.

use super::G2RTraitRepr;

impl G2RTraitRepr {
    pub fn to_importc(&self) -> String {
        let prefix = format!("const void c_{}_", self.name);
        let decs = self
            .fns
            .iter()
            .map(|f| match f.ffi_param_cnt() {
                0 => format!("{prefix}{}();\n", f.name),
                1 => format!("{prefix}{}(const void*);\n", f.name),
                _ => format!("{prefix}{}(const void*, const void*);\n", f.name),
            })
            .collect::<Vec<String>>();
        decs.join("")
    }
}
