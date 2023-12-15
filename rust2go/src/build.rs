use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkType {
    Static,
    Dynamic,
}

pub struct Builder<GOSRC = ()> {
    go_src: GOSRC,
    out_dir: Option<PathBuf>,
    out_name: Option<String>,
    binding_name: Option<String>,
    link: LinkType,
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl Builder {
    pub fn new() -> Self {
        Builder {
            go_src: (),
            out_dir: None,
            out_name: None,
            binding_name: None,
            link: LinkType::Static,
        }
    }
}

impl<GOSRC> Builder<GOSRC> {
    pub fn with_go_src<S: Into<PathBuf>>(self, go_src: S) -> Builder<PathBuf> {
        Builder {
            go_src: go_src.into(),
            out_name: self.out_name,
            out_dir: self.out_dir,
            binding_name: self.binding_name,
            link: self.link,
        }
    }

    /// Default binding name is "_go_bindings.rs".
    /// Use with_binding to set it.
    pub fn with_binding(self, out_name: &str) -> Self {
        Builder {
            go_src: self.go_src,
            out_name: Some(out_name.to_string()),
            out_dir: self.out_dir,
            binding_name: self.binding_name,
            link: self.link,
        }
    }

    /// Default link type is static linking.
    /// Use with_link to set it.
    pub fn with_link(self, link: LinkType) -> Self {
        Builder {
            go_src: self.go_src,
            out_name: self.out_name,
            out_dir: self.out_dir,
            binding_name: self.binding_name,
            link,
        }
    }
}

impl Builder<PathBuf> {
    pub fn build(self) {
        // Golang -> $OUT_DIR/_go_bindings.rs
        // This file must be in OUT_DIR, not user specified
        // File name can be specified by users
        let binding_name = self
            .binding_name
            .as_deref()
            .unwrap_or(crate::DEFAULT_BINDING_FILE);
        Self::build_go(&self.go_src, binding_name, self.link);
    }

    fn build_go(go_src: &Path, binding_name: &str, link: LinkType) {
        let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
        let mut go_build = Command::new("go");
        go_build
            .env("GO111MODULE", "off")
            // Why add GODEBUG=cgocheck=0: https://pkg.go.dev/cmd/cgo#hdr-Passing_pointers
            .env("GODEBUG", "cgocheck=0")
            .arg("build")
            .arg(if link == LinkType::Static {
                "-buildmode=c-archive"
            } else {
                "-buildmode=c-shared"
            })
            .arg("-o")
            .arg(out_dir.join(if link == LinkType::Static {
                "libgo.a"
            } else {
                "libgo.so"
            }))
            .arg(go_src);

        go_build.status().expect("Go build failed");

        let bindings = bindgen::Builder::default()
            .header(out_dir.join("libgo.h").to_str().unwrap())
            .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
            .generate()
            .expect("Unable to generate bindings");

        bindings
            .write_to_file(out_dir.join(binding_name))
            .expect("Couldn't write bindings!");

        println!("cargo:rerun-if-changed={}", go_src.to_str().unwrap());
        println!(
            "cargo:rustc-link-search=native={}",
            out_dir.to_str().unwrap()
        );
        if link == LinkType::Static {
            println!("cargo:rustc-link-lib=static=go");
        } else {
            println!("cargo:rustc-link-lib=dylib=go");
        }
    }
}
