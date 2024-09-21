use std::path::PathBuf;

const COMMAND_FILE: &str = "src/command.rs";

fn main() {
    println!("cargo:rerun-if-changed={COMMAND_FILE}");
    rust_sitter_tool::build_parsers(&PathBuf::from(COMMAND_FILE));
}