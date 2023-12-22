use std::io::Cursor;

use clap::Parser;
use rust2go_common::raw_file::RawRsFile;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path of source rust file
    #[arg(short, long)]
    src: String,

    /// Path of destination go file
    #[arg(short, long)]
    dst: String,

    /// With or without go main function
    #[arg(long, default_value = "false")]
    without_main: bool,

    /// Go 1.18 compatible
    #[arg(long, default_value = "false")]
    go118: bool,
}

fn main() {
    let args = Args::parse();

    // Read and parse rs file.
    let file_content = std::fs::read_to_string(args.src).expect("Unable to read file");
    let raw_file = RawRsFile::new(file_content);

    // Convert to Ref structs and write to output file.
    let (name_mapping, ref_content) = raw_file
        .convert_structs_to_ref()
        .expect("Unable to convert to ref");
    std::fs::write(&args.dst, ref_content.to_string()).expect("Unable to write file");

    // Convert output file with cbindgen.
    let mut cbuilder = cbindgen::Builder::new()
        .with_language(cbindgen::Language::C)
        .with_src(&args.dst)
        .with_header("// Generated by rust2go. Please DO NOT edit this C part manually.");
    for name in name_mapping.values() {
        cbuilder = cbuilder.include_item(name.to_string());
    }
    let mut output = Vec::<u8>::new();
    cbuilder
        .generate()
        .expect("Unable to generate bindings")
        .write(Cursor::new(&mut output));

    // Convert headers into golang.
    let mut output = String::from_utf8(output).expect("Unable to convert to string");

    let traits = raw_file.convert_trait().unwrap();
    traits
        .iter()
        .for_each(|t| output.push_str(&t.generate_c_callbacks()));

    let import_reflect = if args.go118 { "\n\"reflect\"" } else { "" };
    let mut go_content = format!(
        "package main\n\n/*\n{output}*/\nimport \"C\"\nimport ({import_reflect}\n\"unsafe\"\n\"runtime\"\n)\n"
    );
    let levels = raw_file.convert_structs_levels().unwrap();
    traits.iter().for_each(|t| {
        go_content.push_str(&t.generate_go_interface());
        go_content.push_str(&t.generate_go_exports(&levels));
    });
    go_content.push_str(
        &raw_file
            .convert_structs_to_go(&levels, args.go118)
            .expect("Unable to generate go structs"),
    );
    if !args.without_main {
        go_content.push_str("func main() {}\n");
    }

    std::fs::write(&args.dst, go_content).expect("Unable to write file");
}
