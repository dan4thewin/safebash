pub mod arith_expr_mutation;
pub mod dynamic_execution;
pub mod export_rejected;
pub mod extglob_rejected;
pub mod glob_restricted;
pub mod heredoc_cmdsub;
pub mod indirect_expansion;
pub mod nameref_rejected;
pub mod param_transform;
pub mod parse_error;
pub mod process_substitution;
pub mod wordsplit_rejected;
pub mod command_resolution;
pub mod dollar_question;
pub mod numeric_test_rejected;
pub mod regex_validation;
pub mod rejected_vars;

use crate::rule::Rule;

pub fn all_rules() -> Vec<Box<dyn Rule>> {
    vec![
        Box::new(arith_expr_mutation::ArithExprMutation),
        Box::new(arith_expr_mutation::ArithStmtNoEffect),
        Box::new(dynamic_execution::DynamicExecution),
        Box::new(export_rejected::ExportRejected),
        Box::new(extglob_rejected::ExtglobRejected),
        Box::new(glob_restricted::GlobRestricted),
        Box::new(heredoc_cmdsub::HeredocCmdSub),
        Box::new(indirect_expansion::IndirectExpansion),
        Box::new(nameref_rejected::NamerefRejected),
        Box::new(parse_error::ParseError),
        Box::new(param_transform::ParamTransform),
        Box::new(process_substitution::ProcessSubstitution),
        Box::new(wordsplit_rejected::IfsRejected),
        Box::new(wordsplit_rejected::ForInExpansion),
        Box::new(command_resolution::CommandResolution),
        Box::new(dollar_question::DollarQuestion),
        Box::new(rejected_vars::RejectedVars),
        Box::new(numeric_test_rejected::NumericTestRejected),
        Box::new(regex_validation::RegexValidation),
    ]
}
