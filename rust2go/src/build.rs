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

pub struct Builder<GOSRC = (), GOC = DefaultGoCompiler> {
    go_src: GOSRC,
    out_dir: Option<PathBuf>,
    out_name: Option<String>,
    binding_name: Option<String>,
    link: LinkType,
    regen_arg: Args,
    copy_lib: CopyLib,
    go_comp: GOC,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CopyLib {
    Disabled,
    DefaultPath,
    CustomPath(PathBuf),
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
            copy_lib: CopyLib::Disabled,
            go_comp: DefaultGoCompiler,
        }
    }
}

impl<GOSRC, GOC> Builder<GOSRC, GOC> {
    pub fn with_go_src<S: Into<PathBuf>>(self, go_src: S) -> Builder<PathBuf, GOC> {
        Builder {
            go_src: go_src.into(),
            out_name: self.out_name,
            out_dir: self.out_dir,
            binding_name: self.binding_name,
            link: self.link,
            regen_arg: self.regen_arg,
            copy_lib: self.copy_lib,
            go_comp: self.go_comp,
        }
    }

    pub fn with_go_compiler<GOC2>(self, go_comp: GOC2) -> Builder<GOSRC, GOC2> {
        Builder {
            go_src: self.go_src,
            out_name: self.out_name,
            out_dir: self.out_dir,
            binding_name: self.binding_name,
            link: self.link,
            regen_arg: self.regen_arg,
            copy_lib: self.copy_lib,
            go_comp,
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

    /// Copy DLL to target dir.
    pub fn with_copy_lib(mut self, copy_lib: CopyLib) -> Self {
        self.copy_lib = copy_lib;
        self
    }
}

pub trait GoCompiler {
    fn go_build(&self, go_src: &Path, link: LinkType, output: &Path);

    fn build(&self, go_src: &Path, binding_name: &str, link: LinkType, copy_lib: &CopyLib) {
        let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
        let out_filename = filename(link);
        let output = out_dir.join(&out_filename);

        self.go_build(go_src, link, output.as_path());

        // Copy the DLL file to target dir.
        if link == LinkType::Dynamic {
            // A workaround to get target dir.
            // From https://github.com/rust-lang/cargo/issues/9661#issuecomment-1722358176
            fn get_cargo_target_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
                let out_dir = PathBuf::from(env::var("OUT_DIR")?);
                let profile = env::var("PROFILE")?;
                let mut target_dir = None;
                let mut sub_path = out_dir.as_path();
                while let Some(parent) = sub_path.parent() {
                    if parent.ends_with(&profile) {
                        target_dir = Some(parent);
                        break;
                    }
                    sub_path = parent;
                }
                let target_dir = target_dir.ok_or("not found")?;
                Ok(target_dir.to_path_buf())
            }

            match copy_lib {
                CopyLib::Disabled => (),
                CopyLib::DefaultPath => {
                    let target_dir = get_cargo_target_dir().unwrap();
                    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
                    std::fs::copy(out_dir.join(&out_filename), target_dir.join(&out_filename))
                        .expect("unable to copy dynamic library");
                }
                CopyLib::CustomPath(p) => {
                    std::fs::copy(out_dir.join(&out_filename), p.join(&out_filename))
                        .expect("unable to copy dynamic library");
                }
            }
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

#[derive(Debug, Clone, Copy)]
pub struct DefaultGoCompiler;
impl GoCompiler for DefaultGoCompiler {
    fn go_build(&self, go_src: &Path, link: LinkType, output: &Path) {
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
            .arg(output)
            .arg(".");

        assert!(
            go_build.status().expect("Go build failed").success(),
            "Go build failed"
        );
    }
}

impl<GOC: GoCompiler> Builder<PathBuf, GOC> {
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
        self.go_comp
            .build(&self.go_src, binding_name, self.link, &self.copy_lib);
    }
}

fn filename(link_type: LinkType) -> String {
    use std::env::consts::{DLL_PREFIX, DLL_SUFFIX};

    match link_type {
        LinkType::Static => format!("{DLL_PREFIX}go.a"),
        LinkType::Dynamic => format!("{DLL_PREFIX}go{DLL_SUFFIX}"),
    }
}
