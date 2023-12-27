use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

use rust2go_cli::Args;

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
    regen_arg: Args,
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
            regen_arg: Args::default(),
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
            regen_arg: self.regen_arg,
        }
    }

    /// Default binding name is "_go_bindings.rs".
    /// Use with_binding to set it.
    pub fn with_binding(mut self, out_name: &str) -> Self {
        self.out_name = Some(out_name.to_string());
        self
    }

    /// Default link type is static linking.
    /// Use with_link to set it.
    pub fn with_link(mut self, link: LinkType) -> Self {
        self.link = link;
        self
    }

    /// Regenerate go code.
    /// Note: you should generate go code before build with rust2go-cli.
    /// This function is to make sure the go code is updated.
    pub fn with_regen(mut self, src: &str, dst: &str) -> Self {
        self.regen_arg.src = src.to_string();
        self.regen_arg.dst = dst.to_string();
        self
    }

    /// Regenerate go code.
    /// Note: you should generate go code before build with rust2go-cli.
    /// This function is to make sure the go code is updated.
    pub fn with_regen_arg(mut self, arg: Args) -> Self {
        self.regen_arg = arg;
        self
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
        // Regenerate go code.
        if !self.regen_arg.src.is_empty() && !self.regen_arg.dst.is_empty() {
            rust2go_cli::generate(&self.regen_arg);
        }
        Self::build_go(&self.go_src, binding_name, self.link);
    }

    fn build_go(go_src: &Path, binding_name: &str, link: LinkType) {
        let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
        let mut go_build = Command::new("go");
        go_build
            .env("GO111MODULE", "on")
            .current_dir(go_src)
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
            .arg(".");

        go_build.status().expect("Go build failed");

        // Copy .so file to CARGO_TARGET_DIR
        if link == LinkType::Dynamic {
            let target_dir = PathBuf::from(env::var("CARGO_TARGET_DIR").unwrap());
            let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
            std::fs::copy(out_dir.join("libgo.so"), target_dir.join("libgo.so"))
                .expect("unable to copy dynamic library");
        }

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
