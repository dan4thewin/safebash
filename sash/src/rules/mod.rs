pub mod dynamic_execution;
pub mod export_rejected;
pub mod heredoc_cmdsub;
pub mod indirect_expansion;

use crate::rule::Rule;

pub fn all_rules() -> Vec<Box<dyn Rule>> {
    vec![
        Box::new(dynamic_execution::DynamicExecution),
        Box::new(export_rejected::ExportRejected),
        Box::new(heredoc_cmdsub::HeredocCmdSub),
        Box::new(indirect_expansion::IndirectExpansion),
    ]
}
