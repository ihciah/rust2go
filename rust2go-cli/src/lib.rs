// Copyright 2024 ihciah. All Rights Reserved.

use clap::Parser;

pub use rust2go_gen::{generate, GenArgs};

#[derive(Parser, Debug, Default, Clone)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// Path of source rust file
    #[arg(short, long)]
    pub src: String,

    /// Path of destination go file
    #[arg(short, long)]
    pub dst: String,

    /// Package name of generated go file
    #[arg(long, default_value = "main")]
    pub package_name: String,

    /// With or without go main function
    #[arg(long, default_value = "false")]
    pub without_main: bool,

    /// Go 1.18 compatible
    #[arg(long, default_value = "false")]
    pub go118: bool,

    /// Disable auto format go file
    #[arg(long, default_value = "false")]
    pub no_fmt: bool,
}

impl From<Args> for GenArgs {
    fn from(args: Args) -> Self {
        GenArgs {
            src: args.src,
            dst: args.dst,
            package_name: args.package_name,
            without_main: args.without_main,
            go118: args.go118,
            no_fmt: args.no_fmt,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_convert_to_gen_args() {
        let args = Args::parse_from([
            "rust2go-cli",
            "--src",
            "in.rs",
            "--dst",
            "out.go",
            "--package-name",
            "custom",
            "--without-main",
            "--go118",
            "--no-fmt",
        ]);
        let gen: GenArgs = args.into();
        assert_eq!(gen.src, "in.rs");
        assert_eq!(gen.dst, "out.go");
        assert_eq!(gen.package_name, "custom");
        assert!(gen.without_main);
        assert!(gen.go118);
        assert!(gen.no_fmt);
    }

    #[test]
    fn args_defaults() {
        let gen: GenArgs = Args::parse_from(["rust2go-cli", "-s", "in.rs", "-d", "out.go"]).into();
        assert_eq!(gen.package_name, "main");
        assert!(!gen.without_main);
        assert!(!gen.go118);
        assert!(!gen.no_fmt);
    }
}
