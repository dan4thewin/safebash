pub mod ir;
pub mod lower;
pub mod printer;
pub mod ssa;
pub mod transform;
pub mod ssa_printer;
pub mod emit_janet;
pub mod oracle;
pub mod type_check;
pub mod codegen;
pub mod diagnostic;
pub mod validator;
pub mod rule;
pub mod rules;

pub fn parse(source: &str) -> tree_sitter::Tree {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_bash::LANGUAGE.into())
        .expect("Error loading Bash grammar");
    parser.parse(source, None).expect("Failed to parse")
}
pub mod taint;
