// Copyright 2024 ihciah. All Rights Reserved.

use super::G2RTraitRepr;

impl G2RTraitRepr {
    pub fn to_importc(&self) -> String {
        let prefix = format!("const void c_{}_", self.name);
        let decs = self
            .fns
            .iter()
            .map(|f| {
                let mut out = match f.ffi_param_cnt() {
                    0 => format!("{prefix}{}();\n", f.name),
                    1 => format!("{prefix}{}(const void*);\n", f.name),
                    _ => format!("{prefix}{}(const void*, const void*);\n", f.name),
                };
                // Every function with a return value also gets a typed drop
                // entry that releases the boxed response storage.
                if f.ret.is_some() {
                    out.push_str(&format!("{prefix}{}_drop(void*);\n", f.name));
                }
                out
            })
            .collect::<Vec<String>>();
        decs.join("")
    }
}
