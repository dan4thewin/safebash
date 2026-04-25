use std::collections::HashMap;
use tree_sitter::Node;
use crate::diagnostic::Diagnostic;

/// Taint status of a variable.
#[derive(Debug, Clone, PartialEq)]
pub enum TaintStatus {
    /// Originated from external source, not yet validated.
    Tainted,
    /// Validated via `bless` — safe to use.
    Blessed,
    /// Originated from a literal, arithmetic, or other internal source.
    Clean,
}

/// Flow-sensitive taint tracker.
///
/// Walks the program sequentially, tracking which variables are tainted,
/// blessed, or clean. Emits diagnostics when tainted variables are read
/// outside of a `bless` expression.
///
/// Taint sources:
/// - Positional params ($1–$N, $@) at program entry
/// - `read` targets
/// - `$(external cmd)` assignment targets
/// - Undeclared variables (environment — getenv)
///
/// Clean sources:
/// - Literals, arithmetic from clean operands
/// - $0, $#, $?, $$ — compiler-constrained
/// - Variables assigned from clean expressions
pub struct TaintTracker {
    vars: HashMap<String, TaintStatus>,
    /// Track which variables have been declared/assigned in the script.
    declared: std::collections::HashSet<String>,
    /// Track which functions have been defined in the script.
    functions: std::collections::HashSet<String>,
    /// Are we inside a bless expression? If so, which variable?
    bless_context: Option<String>,
    /// Auto-blessed variables: name → (pattern, description, source line)
    auto_blessed: Vec<AutoBless>,
}

/// Record of a variable auto-blessed via known_externals.
#[derive(Clone, Debug)]
pub struct AutoBless {
    pub var_name: String,
    pub pattern: String,
    pub description: String,
    pub line: usize,
}

impl TaintTracker {
    pub fn new() -> Self {
        Self {
            vars: HashMap::new(),
            declared: std::collections::HashSet::new(),
            functions: std::collections::HashSet::new(),
            bless_context: None,
            auto_blessed: Vec::new(),
        }
    }

    /// Mark a variable as tainted.
    pub fn mark_tainted(&mut self, name: &str) {
        self.vars.insert(name.to_string(), TaintStatus::Tainted);
    }

    /// Mark a variable as blessed (validated).
    pub fn mark_blessed(&mut self, name: &str) {
        self.vars.insert(name.to_string(), TaintStatus::Blessed);
    }

    /// Mark a variable as clean (internal origin).
    pub fn mark_clean(&mut self, name: &str) {
        self.vars.insert(name.to_string(), TaintStatus::Clean);
        self.declared.insert(name.to_string());
    }

    /// Record that a variable was declared/assigned in the script.
    pub fn mark_declared(&mut self, name: &str) {
        self.declared.insert(name.to_string());
    }

    /// Check if a variable is tainted.
    pub fn is_tainted(&self, name: &str) -> bool {
        match self.vars.get(name) {
            Some(TaintStatus::Tainted) => true,
            None => !self.is_special_var(name) && !self.declared.contains(name),
            _ => false,
        }
    }

    /// Check if we're inside a bless context for this variable.
    pub fn in_bless_for(&self, name: &str) -> bool {
        self.bless_context.as_deref() == Some(name)
    }

    /// Special variables that are always clean.
    fn is_special_var(&self, name: &str) -> bool {
        matches!(name, "0" | "#" | "?" | "$" | "LINENO" | "PIPESTATUS"
            | "BASHPID" | "BASH_SUBSHELL" | "SHLVL" | "PPID" | "UID" | "EUID"
            | "GROUPS" | "HOSTNAME" | "HOSTTYPE" | "MACHTYPE" | "OSTYPE"
            | "SHELLOPTS" | "BASHOPTS" | "BASH_VERSION" | "BASH_VERSINFO"
            | "SECONDS" | "RANDOM" | "SRANDOM" | "EPOCHSECONDS" | "EPOCHREALTIME"
            | "IFS" | "OLDPWD" | "_")
    }

    /// Run taint analysis on the full program.
    /// Returns diagnostics for tainted variable reads.
    pub fn analyze(&mut self, root: Node, source: &[u8]) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        self.walk_program(root, source, &mut diags);
        diags
    }

    /// Return auto-blessed variables and their patterns (for --emit-source).
    pub fn auto_blessed(&self) -> &[AutoBless] {
        &self.auto_blessed
    }

    fn walk_program(&mut self, node: Node, source: &[u8], diags: &mut Vec<Diagnostic>) {
        match node.kind() {
            "program" => {
                for child in node.children(&mut node.walk()) {
                    self.walk_statement(child, source, diags);
                }
            }
            _ => self.walk_statement(node, source, diags),
        }
    }

    fn walk_statement(&mut self, node: Node, source: &[u8], diags: &mut Vec<Diagnostic>) {
        match node.kind() {
            "variable_assignment" => {
                self.handle_assignment(node, source, diags);
            }
            "declaration_command" => {
                self.handle_declaration(node, source, diags);
            }
            "command" => {
                self.handle_command(node, source, diags);
            }
            "if_statement" => {
                for child in node.children(&mut node.walk()) {
                    match child.kind() {
                        "test_command" => {
                            self.check_expr_reads(child, source, diags);
                        }
                        "command" => {
                            self.handle_command(child, source, diags);
                        }
                        _ => self.walk_statement(child, source, diags),
                    }
                }
            }
            "while_statement" | "until_statement" | "for_statement" => {
                // For `for var in words...`, mark the loop variable.
                // If all words are literals, it's clean. If any word
                // contains expansions of tainted vars, the loop var is tainted.
                if node.kind() == "for_statement" {
                    if let Some(var_node) = node.child_by_field_name("variable") {
                        let var_name = var_node.utf8_text(source).unwrap_or("");
                        // Check if the word list is all literals
                        let mut has_tainted_source = false;
                        let mut _has_expansion = false;
                        for child in node.children(&mut node.walk()) {
                            if child.kind() == "word" || child.kind() == "string"
                                || child.kind() == "concatenation"
                                || child.kind() == "simple_expansion"
                                || child.kind() == "expansion" {
                                // Check if this word references tainted vars
                                if self.expr_has_tainted_read(child, source) {
                                    has_tainted_source = true;
                                }
                                if child.kind() == "simple_expansion" || child.kind() == "expansion" {
                                    _has_expansion = true;
                                }
                            }
                            if child.kind() == "command_substitution" {
                                has_tainted_source = true;
                                _has_expansion = true;
                            }
                        }
                        if has_tainted_source {
                            self.mark_tainted(var_name);
                        } else {
                            self.mark_clean(var_name);
                        }
                    }
                }
                for child in node.children(&mut node.walk()) {
                    match child.kind() {
                        "test_command" => {
                            self.check_expr_reads(child, source, diags);
                        }
                        "command" => {
                            self.handle_command(child, source, diags);
                        }
                        _ => self.walk_statement(child, source, diags),
                    }
                }
            }
            "function_definition" => {
                self.handle_function_def(node, source, diags);
            }
            "compound_statement" | "do_group" | "subshell" => {
                for child in node.children(&mut node.walk()) {
                    self.walk_statement(child, source, diags);
                }
            }
            "arithmetic_command" => {
                // (( expr )) — check all variable references for taint
                for child in node.children(&mut node.walk()) {
                    self.check_arith_reads(child, source, diags);
                }
            }
            "pipeline" | "list" | "redirected_statement" => {
                for child in node.children(&mut node.walk()) {
                    self.walk_statement(child, source, diags);
                }
            }
            "case_statement" => {
                for child in node.children(&mut node.walk()) {
                    if child.kind() == "word" || child.kind() == "simple_expansion" || child.kind() == "expansion" || child.kind() == "string" || child.kind() == "concatenation" {
                        // The case value expression — check for tainted reads
                        self.check_expr_reads(child, source, diags);
                    } else {
                        self.walk_statement(child, source, diags);
                    }
                }
            }
            _ => {}
        }
    }

    fn handle_assignment(&mut self, node: Node, source: &[u8], _diags: &mut Vec<Diagnostic>) {
        // variable_assignment: name=value
        let name = node.child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or("");
        self.mark_declared(name);

        // Check if RHS contains tainted expressions.
        // Assignment is a WRITE context — reading tainted data into a var
        // is allowed. The var inherits the taint status. No diagnostic here.
        if let Some(value) = node.child_by_field_name("value") {
            if self.expr_is_tainted(value, source) {
                // Before marking tainted, check if the RHS is a known-safe command.
                if let Some(auto) = self.check_known_external(value, source, name) {
                    // Known command/env var — auto-bless with the known pattern.
                    let mut ab = auto;
                    ab.line = node.start_position().row + 1;
                    self.auto_blessed.push(ab);
                    self.mark_blessed(name);
                } else {
                    self.mark_tainted(name);
                }
            } else {
                self.mark_clean(name);
            }
        } else {
            self.mark_clean(name);
        }
    }

    fn handle_declaration(&mut self, node: Node, source: &[u8], diags: &mut Vec<Diagnostic>) {
        // declaration_command: declare/int/array/map/readonly name[=value]
        for child in node.children(&mut node.walk()) {
            if child.kind() == "variable_assignment" {
                self.handle_assignment(child, source, diags);
            } else if child.kind() == "variable_name" || child.kind() == "word" {
                let name = child.utf8_text(source).unwrap_or("");
                self.mark_declared(name);
                self.mark_clean(name);
            }
        }
    }

    fn handle_command(&mut self, node: Node, source: &[u8], diags: &mut Vec<Diagnostic>) {
        let name_node = node.child_by_field_name("name");
        let cmd_name = name_node
            .and_then(|n| n.child(0))
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or("");

        match cmd_name {
            "bless" => {
                self.handle_bless(node, source, diags);
            }
            "read" | "readarray" | "mapfile" => {
                self.handle_read(node, source);
            }
            _ => {
                // Check all arguments for tainted reads
                for child in node.children(&mut node.walk()) {
                    if child.kind() != "command_name" {
                        self.check_expr_reads(child, source, diags);
                    }
                }
            }
        }
    }

    fn handle_bless(&mut self, node: Node, source: &[u8], diags: &mut Vec<Diagnostic>) {
        // bless VAR EXPR
        // First non-command-name arg is the variable name.
        // Remaining args form the test expression.
        let mut args = Vec::new();
        for child in node.children(&mut node.walk()) {
            if child.kind() != "command_name" {
                args.push(child);
            }
        }

        if args.is_empty() {
            diags.push(Diagnostic::error(
                node,
                "SB4010",
                "bless requires a variable name and test expression".to_string(),
            ));
            return;
        }

        let var_name = args[0].utf8_text(source).unwrap_or("");

        if !self.is_tainted(var_name) && self.vars.contains_key(var_name) {
            diags.push(Diagnostic::error(
                node,
                "SB4011",
                format!("bless: variable '{}' is already clean — bless is unnecessary", var_name),
            ));
            return;
        }

        // Enter bless context — reads of var_name are allowed inside
        let old_ctx = self.bless_context.take();
        self.bless_context = Some(var_name.to_string());

        // Walk the test expression (everything after the var name)
        for arg in &args[1..] {
            self.check_expr_reads(*arg, source, diags);
        }

        self.bless_context = old_ctx;

        // Mark the variable as blessed
        self.mark_blessed(var_name);
    }

    fn handle_read(&mut self, node: Node, source: &[u8]) {
        // read -r var / read -d: -a arr — targets become tainted
        // Exception: read from herestring (<<<) or heredoc (<<) of clean data → clean
        let has_clean_redirect = node.parent().map(|p| {
            if p.kind() == "redirected_statement" {
                p.children(&mut p.walk()).any(|c| {
                    c.kind() == "herestring_redirect" || c.kind() == "heredoc_redirect"
                })
            } else {
                false
            }
        }).unwrap_or(false);
        // Also check if the command itself has a herestring redirect
        let has_herestring = node.children(&mut node.walk()).any(|c| {
            c.kind() == "herestring_redirect" || c.kind() == "heredoc_redirect"
        });
        let is_clean_source = has_clean_redirect || has_herestring;
        for child in node.children(&mut node.walk()) {
            if child.kind() == "word" {
                let text = child.utf8_text(source).unwrap_or("");
                // Skip flags
                if !text.starts_with('-') {
                    self.mark_declared(text);
                    if is_clean_source {
                        self.mark_clean(text);
                    } else {
                        self.mark_tainted(text);
                    }
                }
            }
        }
    }

    fn handle_function_def(&mut self, node: Node, source: &[u8], diags: &mut Vec<Diagnostic>) {
        // Record function name as declared and as a known function
        if let Some(name_node) = node.child_by_field_name("name") {
            let name = name_node.utf8_text(source).unwrap_or("");
            self.mark_declared(name);
            self.functions.insert(name.to_string());
        }

        // DD#24: external data is quarantined at the boundary, regardless
        // of whether the boundary is crossed at the top level or inside a
        // function body. Walk the body with the same analyzer so that
        // getenv/argv/read/$(external) produce the same diagnostics they
        // would at the top level.
        //
        // Inside a function body, positional parameters ($1..$N, $@, $*)
        // refer to the function's call-site arguments — NOT the program's
        // argv. The call site is itself a boundary: handle_command rejects
        // any tainted argument expression at the call. Therefore inside
        // the body these positionals are clean by construction.
        //
        // This is intra-procedural analysis only — no signatures, no
        // propagation across call boundaries. DD#24's quarantine model
        // says: external data is rejected at the boundary it crosses;
        // a sash call site is such a boundary.
        //
        // We snapshot and restore variable state so taint state from
        // inside the body does not leak to siblings.
        let saved_vars = self.vars.clone();
        let saved_declared = self.declared.clone();
        for p in &["1", "2", "3", "4", "5", "6", "7", "8", "9", "@", "*"] {
            self.mark_clean(p);
        }
        if let Some(body) = node.child_by_field_name("body") {
            self.walk_statement(body, source, diags);
        }
        self.vars = saved_vars;
        self.declared = saved_declared;
    }

    /// Check if an expression evaluates tainted data.
    /// Used for RHS of assignments to determine if the target becomes tainted.
    fn expr_is_tainted(&self, node: Node, source: &[u8]) -> bool {
        match node.kind() {
            "simple_expansion" => {
                // $var
                let var_name = self.extract_var_name(node, source);
                self.is_tainted(&var_name)
            }
            "expansion" => {
                // ${var...}
                let var_name = self.extract_expansion_var(node, source);
                self.is_tainted(&var_name)
            }
            "command_substitution" => {
                // $(cmd) — tainted if external command output
                // Clean if: known-safe builtin, or user-defined function
                let inner_cmd = self.get_cmd_subst_name(node, source);
                if matches!(inner_cmd.as_str(),
                    "len" | "keys" | "join" | "epochseconds") {
                    return false; // known safe builtins, clean output
                }
                if matches!(inner_cmd.as_str(), "getenv" | "argv") {
                    return true; // explicitly tainted sources
                }
                if self.functions.contains(&inner_cmd) {
                    return false; // user-defined function — clean by DD#24
                }
                true // unknown external command — tainted
            }
            "string" => {
                // "..." — check all interpolated expansions
                for child in node.children(&mut node.walk()) {
                    if self.expr_is_tainted(child, source) {
                        return true;
                    }
                }
                false
            }
            "concatenation" => {
                for child in node.children(&mut node.walk()) {
                    if self.expr_is_tainted(child, source) {
                        return true;
                    }
                }
                false
            }
            _ => false,
        }
    }

    /// Check all variable reads in an expression, emitting diagnostics
    /// for tainted reads outside of bless context.
    fn check_expr_reads(&self, node: Node, source: &[u8], diags: &mut Vec<Diagnostic>) {
        match node.kind() {
            "simple_expansion" => {
                let var_name = self.extract_var_name(node, source);
                if self.is_tainted(&var_name) && !self.in_bless_for(&var_name) {
                    diags.push(Diagnostic::error(
                        node,
                        "SB4020",
                        format!("use of tainted variable '{}' — validate with `bless {} [[ ... ]]`", var_name, var_name),
                    ));
                }
            }
            "expansion" => {
                let var_name = self.extract_expansion_var(node, source);
                if self.is_tainted(&var_name) && !self.in_bless_for(&var_name) {
                    diags.push(Diagnostic::error(
                        node,
                        "SB4020",
                        format!("use of tainted variable '{}' — validate with `bless {} [[ ... ]]`", var_name, var_name),
                    ));
                }
                // Also recurse into subscript index expressions
                for child in node.children(&mut node.walk()) {
                    if child.kind() == "subscript" {
                        for sc in child.children(&mut child.walk()) {
                            self.check_expr_reads(sc, source, diags);
                        }
                    }
                }
            }
            "arithmetic_expansion" => {
                // $(( expr )) — check all variable references inside
                self.check_arith_reads(node, source, diags);
            }
            _ => {
                // Recurse into children
                for child in node.children(&mut node.walk()) {
                    self.check_expr_reads(child, source, diags);
                }
            }
        }
    }

    /// Check variable reads inside arithmetic contexts.
    /// In arithmetic, variables are referenced by bare name (no $), so
    /// we look for `variable_name` nodes directly.
    fn check_arith_reads(&self, node: Node, source: &[u8], diags: &mut Vec<Diagnostic>) {
        for child in node.children(&mut node.walk()) {
            if child.kind() == "variable_name" {
                let var_name = child.utf8_text(source).unwrap_or("").to_string();
                if self.is_tainted(&var_name) && !self.in_bless_for(&var_name) {
                    diags.push(Diagnostic::error(
                        child,
                        "SB4020",
                        format!("use of tainted variable '{}' in arithmetic — validate with `bless {} [[ ... ]]`", var_name, var_name),
                    ));
                }
            } else {
                self.check_arith_reads(child, source, diags);
            }
        }
    }

    /// Extract variable name from a simple_expansion node ($var).
    fn extract_var_name(&self, node: Node, source: &[u8]) -> String {
        // simple_expansion: $ variable_name
        for child in node.children(&mut node.walk()) {
            if child.kind() == "variable_name" || child.kind() == "special_variable_name" {
                return child.utf8_text(source).unwrap_or("").to_string();
            }
        }
        // Might be $1, $2 etc.
        let text = node.utf8_text(source).unwrap_or("");
        text.trim_start_matches('$').to_string()
    }

    /// Extract variable name from an expansion node (${var...}).
    fn extract_expansion_var(&self, node: Node, source: &[u8]) -> String {
        // Look for variable_name directly, or inside subscript
        for child in node.children(&mut node.walk()) {
            if child.kind() == "variable_name" {
                return child.utf8_text(source).unwrap_or("").to_string();
            }
            if child.kind() == "subscript" {
                // ${arr[0]} — the var name is inside the subscript node
                for sc in child.children(&mut child.walk()) {
                    if sc.kind() == "variable_name" {
                        return sc.utf8_text(source).unwrap_or("").to_string();
                    }
                }
            }
        }
        String::new()
    }

    /// Get the command name inside a command_substitution $(cmd ...).
    fn get_cmd_subst_name(&self, node: Node, source: &[u8]) -> String {
        for child in node.children(&mut node.walk()) {
            if child.kind() == "command" {
                if let Some(name_node) = child.child_by_field_name("name") {
                    if let Some(word) = name_node.child(0) {
                        return word.utf8_text(source).unwrap_or("").to_string();
                    }
                }
            }
        }
        String::new()
    }

    /// Get command arguments inside a command_substitution $(cmd arg1 arg2 ...).
    fn get_cmd_subst_args(&self, node: Node, source: &[u8]) -> Vec<String> {
        let mut args = Vec::new();
        for child in node.children(&mut node.walk()) {
            if child.kind() == "command" {
                let mut skip_name = true;
                for arg in child.children(&mut child.walk()) {
                    if arg.kind() == "command_name" {
                        skip_name = false;
                        continue;
                    }
                    if skip_name { continue; }
                    if arg.kind() == "word" || arg.kind() == "string"
                        || arg.kind() == "raw_string" || arg.kind() == "string_content" {
                        if let Ok(text) = arg.utf8_text(source) {
                            args.push(text.trim_matches('"').trim_matches('\'').to_string());
                        }
                    }
                }
            }
        }
        args
    }

    /// Check if a value node is a known-safe external (getenv of known var,
    /// or command substitution of known command). Returns AutoBless info if so.
    fn check_known_external(&self, node: Node, source: &[u8], var_name: &str) -> Option<AutoBless> {
        use crate::known_externals::{lookup_env_var, lookup_command};

        match node.kind() {
            "command_substitution" => {
                let cmd_name = self.get_cmd_subst_name(node, source);
                let args = self.get_cmd_subst_args(node, source);

                // $(getenv HOME) → check known env vars
                if cmd_name == "getenv" && !args.is_empty() {
                    if let Some(known) = lookup_env_var(&args[0]) {
                        return Some(AutoBless {
                            var_name: var_name.to_string(),
                            pattern: known.pattern.to_string(),
                            description: known.description.to_string(),
                            line: 0,
                        });
                    }
                }

                // $(hostname), $(date +%Y-%m-%d), etc. → check known commands
                if let Some(known) = lookup_command(&cmd_name, &args) {
                    return Some(AutoBless {
                        var_name: var_name.to_string(),
                        pattern: known.pattern.to_string(),
                        description: known.description.to_string(),
                        line: 0,
                    });
                }

                None
            }
            _ => None,
        }
    }

    /// Check if an expression contains reads of tainted variables (no diagnostics).
    fn expr_has_tainted_read(&self, node: Node, source: &[u8]) -> bool {
        match node.kind() {
            "simple_expansion" => {
                let var_name = self.extract_var_name(node, source);
                self.is_tainted(&var_name)
            }
            "expansion" => {
                let var_name = self.extract_expansion_var(node, source);
                self.is_tainted(&var_name)
            }
            "command_substitution" => true, // external command output is tainted
            _ => {
                for child in node.children(&mut node.walk()) {
                    if self.expr_has_tainted_read(child, source) {
                        return true;
                    }
                }
                false
            }
        }
    }
}
