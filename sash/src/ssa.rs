/// SSA (Static Single Assignment) representation for safebash.
///
/// This layer sits above the IR.  It decomposes a program into
/// basic blocks, versions every variable assignment, and inserts
/// phi nodes at control-flow merge points.
///
/// SSA enables:
///   - Precise data-flow analysis (which definition reaches which use)
///   - Resolving SB4004 deferred diagnostics (variable-as-command)
///   - Dead store elimination
///   - Constant propagation
///
/// The SSA form uses the standard algorithm:
///   1. Build a CFG of basic blocks from the IR
///   2. Compute dominance frontiers
///   3. Insert phi nodes at dominance frontiers
///   4. Rename variables with version numbers
use std::collections::{BTreeMap, HashMap};

/// A unique identifier for a basic block.
pub type BlockId = usize;

/// A versioned variable: (name, version).
/// Version 0 means "undefined / initial".
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VarRef {
    pub name: String,
    pub version: u32,
    pub var_type: crate::ir::VarType,
}

impl VarRef {
    pub fn new(name: &str, version: u32) -> Self {
        Self {
            name: name.to_string(),
            version,
            var_type: crate::ir::VarType::String,
        }
    }

    pub fn new_typed(name: &str, version: u32, var_type: crate::ir::VarType) -> Self {
        Self {
            name: name.to_string(),
            version,
            var_type,
        }
    }

    pub fn display(&self) -> String {
        format!("{}_{}", self.name, self.version)
    }
}

/// A phi node: variable = phi(block1: version1, block2: version2, ...)
#[derive(Debug, Clone, PartialEq)]
pub struct PhiNode {
    pub target: VarRef,
    pub sources: Vec<(BlockId, VarRef)>,
}

/// Typed identification for command names appearing in
/// `SsaInst::Command` (and at IR dispatch sites).
///
/// Replaces stringly-typed `if name == "echo"` chains across
/// transform, codegen and emit_janet, and gives compile-time safety
/// for compiler-internal sigils (variants whose canonical name starts
/// with `__`). Use [`BuiltinCmd::from_name`] to classify a raw name;
/// use [`BuiltinCmd::sigil_str`] for the round-trip back to the
/// canonical sigil string used in `SsaInst::Command::name`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinCmd {
    // ─── Compiler-internal sigils (transform → codegen protocol) ───
    /// Flip test_result_var and exit_status_var (post-`!`).
    NegateTestResult,
    /// Run a `let` expression for side-effects, propagate non-zero
    /// exit per DD#55.
    LetExit,
    /// Fork point — used by background `cmd &` (DD#38: `&` is the fork
    /// mechanism). (Subshells `( … )` are rejected; this is not subshell-only.)
    SubshellStart,
    /// Push current fd state before a redirect.
    RedirectPush,
    /// Pop fd state after the redirected scope.
    RedirectPop,
    /// Capture the most recent background pid into a named variable.
    StoreBgPid,
    /// Install signal handler block.
    TrapSignal,
    /// Cooperative signal-flag check inserted at safe points.
    SignalFlag,
    /// `unset arr[key]` — codegen-time table deletion.
    TableDelete,

    // ─── Shell builtins recognized by codegen / emit_janet ───
    Echo,
    /// `[[ ... ]]` — recognized at IR level, becomes `SsaInst::Test`
    /// in SSA. Only appears as a `Command` in legacy / fallback paths.
    Test,
    /// `:` — no-op.
    Noop,
    Exit,
    Exec,
    Cd,
    Wait,
    Shift,
    /// `command` builtin (bypass function lookup).
    CommandBuiltin,
    /// `bless VAR [[ ... ]]` — recognized at IR level, becomes
    /// `SsaInst::Bless` in SSA. Only appears as a `Command` in
    /// fallback paths (no TestExpr arg).
    Bless,
    Read,
    /// `mapfile` / `readarray` (same builtin).
    Mapfile,
    /// DD#60: `argv [N] a` fills array `a` with the program argv slice
    /// (out-param builtin like mapfile / read -a).
    Argv,
    Break,
    Continue,
    Return,
    Signal,
    Trap,
    Unset,

    // ─── Sash-native intrinsics ───
    /// `$(len ...)` — string or array length.
    Len,
    /// `$(keys arr)` — associative-array key list.
    Keys,
    /// `$(join [sep] @arr)` — join elements.
    Join,
    /// `$(sub SUBJECT PATTERN REPL)` — DD#63 regex replace, first match.
    Sub,
    /// `$(gsub SUBJECT PATTERN REPL)` — DD#63 regex replace, all matches.
    Gsub,
    /// `match SUBJECT PATTERN` — DD#63 regex match. As a command: boolean
    /// exit status. As `$(match …)`: prints the whole match (`resub[0]`).
    Match,
    /// `$(toupper s)` / `$(tolower s)` — DD#65 case-conversion builtins
    /// (irreducible: a regex `sub` cannot upper/lowercase).
    Toupper,
    Tolower,
    /// `$(basename p)` / `$(dirname p)` — DD#65 path sugar: rewritten to a
    /// fixed-pattern `sub` (`.*/` / `/[^/]*$` → empty).
    Basename,
    Dirname,

    /// Anything else: external command, user function call, dynamic
    /// expansion. The raw name in `SsaInst::Command::name` is the
    /// source of truth for these.
    Other,
}

impl BuiltinCmd {
    /// Classify a raw command name into a [`BuiltinCmd`] variant.
    /// Unrecognized names map to [`BuiltinCmd::Other`].
    pub fn from_name(name: &str) -> Self {
        match name {
            "__negate_test_result" => Self::NegateTestResult,
            "__let_exit" => Self::LetExit,
            "__subshell" => Self::SubshellStart,
            "__redirect_push" => Self::RedirectPush,
            "__redirect_pop" => Self::RedirectPop,
            "__store_bg_pid" => Self::StoreBgPid,
            "__trap_signal" => Self::TrapSignal,
            "__signal_flag" => Self::SignalFlag,
            "__table_delete" => Self::TableDelete,
            "echo" => Self::Echo,
            "[[" => Self::Test,
            ":" => Self::Noop,
            "exit" => Self::Exit,
            "exec" => Self::Exec,
            "cd" => Self::Cd,
            "wait" => Self::Wait,
            "shift" => Self::Shift,
            "command" => Self::CommandBuiltin,
            "bless" => Self::Bless,
            "read" => Self::Read,
            "mapfile" | "readarray" => Self::Mapfile,
            "argv" => Self::Argv,
            "break" => Self::Break,
            "continue" => Self::Continue,
            "return" => Self::Return,
            "signal" => Self::Signal,
            "trap" => Self::Trap,
            "unset" => Self::Unset,
            "len" => Self::Len,
            "keys" => Self::Keys,
            "join" => Self::Join,
            "sub" => Self::Sub,
            "gsub" => Self::Gsub,
            "match" => Self::Match,
            "toupper" => Self::Toupper,
            "tolower" => Self::Tolower,
            "basename" => Self::Basename,
            "dirname" => Self::Dirname,
            _ => Self::Other,
        }
    }

    /// Canonical sigil string for compiler-internal sigil variants.
    /// Returns `None` for non-sigil variants (the raw name should be
    /// used instead).
    pub fn sigil_str(self) -> Option<&'static str> {
        Some(match self {
            Self::NegateTestResult => "__negate_test_result",
            Self::LetExit => "__let_exit",
            Self::SubshellStart => "__subshell",
            Self::RedirectPush => "__redirect_push",
            Self::RedirectPop => "__redirect_pop",
            Self::StoreBgPid => "__store_bg_pid",
            Self::TrapSignal => "__trap_signal",
            Self::SignalFlag => "__signal_flag",
            Self::TableDelete => "__table_delete",
            _ => return None,
        })
    }
}

/// An SSA instruction — a versioned wrapper around an IR operation.
#[derive(Debug, Clone, PartialEq)]
pub enum SsaInst {
    /// x_n = value
    Assign { target: VarRef, value: SsaValue },
    /// A command (no variable defined)
    Command {
        name: String,
        /// Typed dispatch key. Populated from `name` at construction
        /// time. Codegen and consumers match on this instead of
        /// stringly-typed `name` checks. `name` stays for diagnostics
        /// and printer output (and for the `Other` arm — external
        /// commands and user functions where the name is the actual
        /// program path).
        kind: BuiltinCmd,
        args: Vec<SsaValue>,
        redirects: Vec<crate::ir::Redirect>,
        /// If true, nonzero exit status aborts the program (DD#55).
        /// False for commands in condition position (if/while/&&/||).
        checked: bool,
    },
    /// Test expression from [[ ]]. Preserved through SSA so codegen
    /// can emit inline comparisons instead of shelling out.
    Test(SsaTest),

    /// read -r line: reads one line from fd, assigns to target, sets __read_eof
    ReadLine {
        target: VarRef,
        fd: i64,           // 0 = stdin
        delim: Option<u8>, // None = newline (default), Some(byte) = custom delimiter
        redirects: Vec<crate::ir::Redirect>,
        /// True when read is in a lastpipe position with clean producer stages
        clean_pipe: bool,
    },
    /// read -r var1 var2 ... varN: read line, split on whitespace into N vars
    /// Last var gets the remainder.
    ReadLineMulti {
        targets: Vec<VarRef>,
        fd: i64,
        delim: Option<u8>,
        redirects: Vec<crate::ir::Redirect>,
        /// True when read is in a lastpipe position with clean producer stages
        clean_pipe: bool,
        /// DD#22: field separator byte (-f flag). None = whitespace split.
        field_sep: Option<u8>,
    },
    /// arr[i]=val or map[key]=val — subscript assignment
    SubscriptAssign {
        array: String,
        index: SsaValue,
        value: SsaValue,
        is_integer_key: bool,
    },
    /// arr+=(val) — append to indexed array
    ArrayAppend { array: String, value: SsaValue },
    /// mapfile -t arr — read all lines from fd into indexed array
    Mapfile {
        array: String,
        fd: i64,
        /// DD#36: map mode — split lines on separator, populate as key=value
        map_mode: bool,
        separator: char,
    },
    /// Read a line, split by delimiter, store into indexed array
    ReadArray {
        array: String,
        fd: i64,
        delim: Option<u8>,
    },
    /// DD#63: `match S P ARR` out-array form. Fills `array` with
    /// `[whole, g1..gN]` (bash-parity length = group count + 1), clears it
    /// on entry, and sets the boolean test result / `$?` like 2-arg
    /// `match`. The subject is always clean (a tainted subject is rejected
    /// at the read site), so the array is clean.
    MatchInto {
        subject: SsaValue,
        pattern: SsaValue,
        array: String,
    },
    /// DD#60: `argv [N] a` — fill `array` with the program argv slice
    /// argv[start..argc] (0-indexed). Auto-declares and taints the array
    /// (a bulk-read source like mapfile / read -a; DD#61).
    ArgvFill { array: String, start: i64 },
    /// Start capturing stdout (for $(func) capture)
    CaptureStart,
    /// End capturing, assign captured output to target
    CaptureEnd { target: VarRef },
    /// Multi-stage pipeline: cmd1 | cmd2 | ...
    Pipeline {
        stages: Vec<(String, Vec<SsaValue>, Vec<crate::ir::Redirect>)>,
        /// If true, the last pipe read fd is saved for a subsequent while-read loop
        capture_read_fd: bool,
        /// DD#55: if true, nonzero aggregate status aborts
        checked: bool,
        /// DD#55 per-segment `-` suppression, aligned with `stages`. A
        /// suppressed stage still runs (real pipe stage); only its exit
        /// status is excluded from the pipefail aggregate. Empty ⇒ none.
        suppressed: Vec<bool>,
    },
    /// bless VAR [[ test_expr ]] — runtime validation of tainted data.
    /// If the test fails, abort with nonzero exit.
    Bless {
        target: VarRef,
        source: VarRef,
        test: SsaTest,
    },
    /// DD#61: `bless x in a [[…]]` / `bless k v in m [[…]]` — validate
    /// every element of an array/map against a per-element predicate.
    /// Binds each element (indexed: `[value_binder]`; map: `[key, value]`)
    /// and evaluates `test`. Sets the test-result var to the all-passed
    /// boolean (vacuously true for an empty collection). When `if_form`
    /// is false (statement form) a failing element aborts with exit 1
    /// instead; when true the boolean is left for a surrounding branch.
    BlessAll {
        source: VarRef,
        /// Versioned, globally-unique loop-binder refs (indexed: `[value]`;
        /// map: `[key, value]`). Transform assigns fresh versions so the
        /// predicate's operands resolve to the binder slots codegen creates,
        /// not to an unrelated same-named SSA version (DD#61 shadowing).
        binders: Vec<VarRef>,
        test: SsaTest,
        is_map: bool,
        if_form: bool,
    },
    /// DD#61: retag an array/map as clean after a successful `BlessAll` —
    /// copies the source slot (the collection's backing table) to a new
    /// clean SSA version. No runtime validation (already done by BlessAll).
    BlessRetag { target: VarRef, source: VarRef },
}

impl SsaInst {
    /// Construct an `SsaInst::Command` from a raw name, deriving
    /// `kind` via [`BuiltinCmd::from_name`]. Use this everywhere
    /// `SsaInst::Command { ... }` would otherwise be built so the
    /// `kind` field stays in sync with `name`.
    pub fn command(
        name: impl Into<String>,
        args: Vec<SsaValue>,
        redirects: Vec<crate::ir::Redirect>,
        checked: bool,
    ) -> Self {
        let name = name.into();
        let kind = BuiltinCmd::from_name(&name);
        SsaInst::Command {
            name,
            kind,
            args,
            redirects,
            checked,
        }
    }

    /// Construct an `SsaInst::Command` representing a compiler-internal
    /// sigil. Panics if `kind` is not a sigil variant (one whose
    /// [`BuiltinCmd::sigil_str`] returns `Some`).
    pub fn sigil(kind: BuiltinCmd, args: Vec<SsaValue>, checked: bool) -> Self {
        let name = kind
            .sigil_str()
            .expect("SsaInst::sigil requires a sigil variant")
            .to_string();
        SsaInst::Command {
            name,
            kind,
            args,
            redirects: vec![],
            checked,
        }
    }
}

/// Arithmetic operators for $(( )) and (( )) contexts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArithOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    Shl,
    Shr,
    BitAnd,
    BitOr,
    BitXor,
    Pow,
}

impl ArithOp {
    pub fn from_str(s: &str) -> Option<ArithOp> {
        match s {
            "+" => Some(ArithOp::Add),
            "-" => Some(ArithOp::Sub),
            "*" => Some(ArithOp::Mul),
            "/" => Some(ArithOp::Div),
            "%" => Some(ArithOp::Mod),
            "==" => Some(ArithOp::Eq),
            "!=" => Some(ArithOp::Ne),
            "<" => Some(ArithOp::Lt),
            "<=" => Some(ArithOp::Le),
            ">" => Some(ArithOp::Gt),
            ">=" => Some(ArithOp::Ge),
            "&&" => Some(ArithOp::And),
            "||" => Some(ArithOp::Or),
            "<<" => Some(ArithOp::Shl),
            ">>" => Some(ArithOp::Shr),
            "&" => Some(ArithOp::BitAnd),
            "|" => Some(ArithOp::BitOr),
            "^" => Some(ArithOp::BitXor),
            "**" => Some(ArithOp::Pow),
            _ => None,
        }
    }

    pub fn as_u8(self) -> u8 {
        match self {
            ArithOp::Add => b'+',
            ArithOp::Sub => b'-',
            ArithOp::Mul => b'*',
            ArithOp::Div => b'/',
            ArithOp::Mod => b'%',
            ArithOp::Eq => 1,
            ArithOp::Ne => 2,
            ArithOp::Lt => 3,
            ArithOp::Le => 4,
            ArithOp::Gt => 5,
            ArithOp::Ge => 6,
            ArithOp::And => 7,
            ArithOp::Or => 8,
            ArithOp::Shl => 10,
            ArithOp::Shr => 11,
            ArithOp::BitAnd => 12,
            ArithOp::BitOr => 13,
            ArithOp::BitXor => 14,
            ArithOp::Pow => 15,
        }
    }

    pub fn display(&self) -> &'static str {
        match self {
            ArithOp::Add => "+",
            ArithOp::Sub => "-",
            ArithOp::Mul => "*",
            ArithOp::Div => "/",
            ArithOp::Mod => "%",
            ArithOp::Eq => "==",
            ArithOp::Ne => "!=",
            ArithOp::Lt => "<",
            ArithOp::Le => "<=",
            ArithOp::Gt => ">",
            ArithOp::Ge => ">=",
            ArithOp::And => "&&",
            ArithOp::Or => "||",
            ArithOp::Shl => "<<",
            ArithOp::Shr => ">>",
            ArithOp::BitAnd => "&",
            ArithOp::BitOr => "|",
            ArithOp::BitXor => "^",
            ArithOp::Pow => "**",
        }
    }
}

/// A value in SSA — either a literal or a variable reference.
#[derive(Debug, Clone, PartialEq)]
pub enum SsaValue {
    Literal(String),
    Var(VarRef),
    /// Complex expression (not yet decomposed — placeholder for
    /// quoted strings, parameter expansions, etc.)
    Expr(String),
    /// Call an elidable function, capture its output as a string.
    FuncCall(String),
    /// DD#58: Call a -> int function in arithmetic context.
    /// Produces an integer return value, not stdout.
    IntFuncCall {
        name: String,
        args: Vec<SsaValue>,
    },
    /// Runtime command substitution: fork/exec, capture stdout.
    /// Carries the command name and arguments as expressions.
    CmdSubst {
        argv: Vec<SsaValue>,
        redirects: Vec<crate::ir::Redirect>,
    },
    /// Concatenation of multiple values (from double-quoted strings
    /// with mixed literal and variable parts).
    Concat(Vec<SsaValue>),
    /// Arithmetic expansion: $((expr))
    /// Stores the variable name, operator, and integer operand.
    ArithExpr {
        var: VarRef,
        op: ArithOp,
        operand: i64,
    },
    /// General arithmetic binary op: $((a + b)) where both sides can be vars or expressions
    ArithBinOp {
        left: Box<SsaValue>,
        op: ArithOp,
        right: Box<SsaValue>,
    },
    /// Parameter expansion with operator: ${var%%pat}, ${var#pat},
    /// ${var:-default}, ${var:=default}, ${var:+alt}, etc.
    ParamExpansion {
        var: VarRef,
        operator: String,
        operand: String,
    },
    /// DD#63: `$(sub S P R)` / `$(gsub S P R)` — regex replace with
    /// ordinary value operands (interpolation is normal arg eval).
    /// `global` selects gsub (all matches) over sub (first match).
    /// Distinct from the literal-only `regex_replace` ParamExpansion
    /// operator used by the `${var:s/…}` / `var =~ s/…` sugar today.
    RegexReplace {
        subject: Box<SsaValue>,
        pattern: Box<SsaValue>,
        replacement: Box<SsaValue>,
        global: bool,
    },
    /// DD#63: `$(match S P)` — whole-match (`resub[0]`) capture with
    /// ordinary value operands. Value position only; the boolean
    /// `match S P` command form is an `SsaInst::Command`.
    RegexMatch {
        subject: Box<SsaValue>,
        pattern: Box<SsaValue>,
    },
    /// DD#65: `$(toupper s)` / `$(tolower s)` — ASCII case conversion of an
    /// ordinary value operand (reuses the `sash_toupper_all`/`sash_tolower_all`
    /// runtime the `${var^^}`/`${var,,}` operators used).
    CaseConvert {
        subject: Box<SsaValue>,
        upper: bool,
    },
    /// DD#65: `${var:-DEFAULT}` — `var` if set/non-empty, else the DEFAULT.
    /// The default is a full interpolating value (unlike the literal-only
    /// `ParamExpansion` operand), so `${foo:-$bar}` interpolates `bar`.
    DefaultValue {
        var: VarRef,
        default: Box<SsaValue>,
    },
    /// Associative array literal: [(k1,v1), (k2,v2), ...]
    AssocArray(Vec<(String, SsaValue)>),
    /// Associative array subscript: ${MAP[$KEY]}
    Subscript {
        array: String,
        index: Box<SsaValue>,
        is_integer_key: bool, // true = indexed array (key is raw i64 bytes)
    },
    /// Positional parameter by index: argv[i+1] (for dynamic for loops)
    ArgvIndex(VarRef),
    /// Indexed array literal: (val0, val1, val2, ...)
    /// Creates a hash table with integer keys "0", "1", "2", ...
    IndexedArray(Vec<SsaValue>),
    /// Ternary: cond ? true_val : false_val
    Ternary {
        cond: Box<SsaValue>,
        true_val: Box<SsaValue>,
        false_val: Box<SsaValue>,
    },
    /// Table element by positional index: sash_table_value(table, i)
    /// Used for `for x in "${arr[@]}"` iteration.
    TableValue {
        array: String,
        index: VarRef,
    },
    /// Table key by positional index: sash_table_key(table, i)
    /// Used for `for k in "${!arr[@]}"` iteration.
    TableKey {
        array: String,
        index: VarRef,
    },
    /// DD#20/DD#22: `@a` (= `${a[@]}`) in command-argument position.
    /// Expands to one verbatim argv entry per array element at codegen
    /// time — NO word splitting within an element, empty array → zero
    /// args. Only valid in command-arg position (the backends' argv
    /// builders intercept it); elsewhere `@a` keeps its joined meaning.
    ArraySplat {
        array: String,
        /// DD#69 external-splat empty-disposition (`@{a:-WORD}` /
        /// `@{a:?WORD}`). `None` = bare `@a` (empty → zero args).
        disposition: Option<ArraySplatDisposition>,
    },
}

/// DD#69: what an external-command `@{a:…}` splat does when the array is
/// empty. Non-empty always splats per-element.
#[derive(Debug, Clone, PartialEq)]
pub struct ArraySplatDisposition {
    /// The WORD (interpolating): the single substituted arg on `:-`, or the
    /// abort message on `:?`.
    pub word: Box<SsaValue>,
    /// `@{a:-WORD}` has a WORD → empty array yields the one arg WORD; bare
    /// `@{a:-}` has none → empty array yields zero args.
    pub has_word: bool,
    /// `:?` — on empty, abort (exit 1, `<progname>: WORD`) instead of
    /// substituting.
    pub abort: bool,
}

impl SsaValue {
    /// The nested `SsaValue` operands directly owned by this value, for
    /// passes that recurse uniformly into sub-values (literal interning,
    /// positional scanning). Leaf values (`Literal`, `Var`, …) and the
    /// string-keyed parts of `AssocArray` are returned empty / handled by
    /// the caller. Mirrors the `Statement::child_bodies` pattern.
    pub(crate) fn child_values(&self) -> Vec<&SsaValue> {
        match self {
            SsaValue::Concat(parts) | SsaValue::IndexedArray(parts) => parts.iter().collect(),
            SsaValue::CmdSubst { argv, .. } => argv.iter().collect(),
            SsaValue::AssocArray(pairs) => pairs.iter().map(|(_, v)| v).collect(),
            SsaValue::Subscript { index, .. } => vec![index],
            SsaValue::RegexReplace {
                subject,
                pattern,
                replacement,
                ..
            } => vec![subject, pattern, replacement],
            SsaValue::RegexMatch { subject, pattern } => vec![subject, pattern],
            SsaValue::CaseConvert { subject, .. } => vec![subject],
            SsaValue::DefaultValue { default, .. } => vec![default],
            SsaValue::ArithBinOp { left, right, .. } => vec![left, right],
            SsaValue::Ternary {
                cond,
                true_val,
                false_val,
            } => vec![cond, true_val, false_val],
            _ => Vec::new(),
        }
    }
}

/// (`lower_value`), so subscripts use `assoc_arrays` key-typing. Only
/// `=~` keeps a raw pattern string.
#[derive(Debug, Clone, PartialEq)]
pub enum SsaTest {
    Unary {
        op: String,
        operand: SsaValue,
    },
    Binary {
        left: SsaValue,
        op: String,
        right: SsaTestRhs,
    },
    Not(Box<SsaTest>),
    And(Box<SsaTest>, Box<SsaTest>),
    Or(Box<SsaTest>, Box<SsaTest>),
}

/// The right operand of a `[[ ]]` binary test: a value (`==`/`!=`/`<`/`>`)
/// or a raw pattern (`=~` regex, and the dispatch-arm patterns).
#[derive(Debug, Clone, PartialEq)]
pub enum SsaTestRhs {
    Value(SsaValue),
    Pattern(String),
}

/// A basic block: a straight-line sequence of instructions with
/// a terminator that transfers control.
#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub id: BlockId,
    pub label: String,
    pub phis: Vec<PhiNode>,
    pub insts: Vec<SsaInst>,
    pub terminator: Terminator,
}

/// How a basic block ends.
#[derive(Debug, Clone, PartialEq)]
pub enum Terminator {
    /// Fall through to the next block.
    Goto(BlockId),
    /// Conditional branch: if condition then goto true_block else goto false_block.
    Branch {
        condition: String,
        true_block: BlockId,
        false_block: BlockId,
    },
    /// Return from function / end of script.
    Return,
    /// Loop back to a block (while/for).
    Loop {
        condition: String,
        body_block: BlockId,
        exit_block: BlockId,
    },
}

/// The complete SSA representation of a program.
#[derive(Debug)]
pub struct SsaProgram {
    pub blocks: BTreeMap<BlockId, BasicBlock>,
    pub entry: BlockId,
    /// DD#56: dedicated END block — all exit paths goto here
    pub end_block: Option<BlockId>,
    /// DD#56: signals with EXIT disposition (check points inserted)
    pub exiting_signals: std::collections::HashSet<i64>,
    /// DD#14: compound pipeline stage entry blocks (reachable roots)
    pub compound_pipeline_blocks: Vec<BlockId>,
    /// `--manifest`: absolute paths of every external program this program
    /// execs — direct commands, pipeline stages, and `$( … )` substitutions.
    /// Collected at the single resolution funnel (`resolve_pipeline_command`)
    /// so it cannot miss an external nor leak a builtin/internal sigil.
    pub external_deps: std::collections::BTreeSet<String>,
    next_block_id: usize,
}

impl SsaProgram {
    pub fn new() -> Self {
        Self {
            blocks: BTreeMap::new(),
            entry: 0,
            end_block: None,
            exiting_signals: std::collections::HashSet::new(),
            compound_pipeline_blocks: Vec::new(),
            external_deps: std::collections::BTreeSet::new(),
            next_block_id: 0,
        }
    }

    pub fn new_block(&mut self, label: &str) -> BlockId {
        let id = self.next_block_id;
        self.next_block_id += 1;
        self.blocks.insert(
            id,
            BasicBlock {
                id,
                label: label.to_string(),
                phis: Vec::new(),
                insts: Vec::new(),
                terminator: Terminator::Return,
            },
        );
        id
    }

    pub fn block_mut(&mut self, id: BlockId) -> &mut BasicBlock {
        self.blocks.get_mut(&id).expect("block not found")
    }

    pub fn block(&self, id: BlockId) -> &BasicBlock {
        self.blocks.get(&id).expect("block not found")
    }

    /// Get predecessor blocks for a given block.
    pub fn predecessors(&self, target: BlockId) -> Vec<BlockId> {
        let mut preds = Vec::new();
        for (id, block) in &self.blocks {
            match &block.terminator {
                Terminator::Goto(dest) if *dest == target => preds.push(*id),
                Terminator::Branch {
                    true_block,
                    false_block,
                    ..
                } => {
                    if *true_block == target || *false_block == target {
                        preds.push(*id);
                    }
                }
                Terminator::Loop {
                    body_block,
                    exit_block,
                    ..
                } => {
                    if *body_block == target || *exit_block == target {
                        preds.push(*id);
                    }
                }
                _ => {}
            }
        }
        preds
    }

    /// Return set of block IDs reachable from block 0.
    pub fn reachable(&self) -> std::collections::HashSet<BlockId> {
        let mut visited = std::collections::HashSet::new();
        let mut stack = vec![0usize];
        // Also include compound pipeline blocks as roots
        for &b in &self.compound_pipeline_blocks {
            stack.push(b);
        }
        while let Some(id) = stack.pop() {
            if !visited.insert(id) {
                continue;
            }
            if let Some(block) = self.blocks.get(&id) {
                match &block.terminator {
                    Terminator::Goto(dest) => {
                        stack.push(*dest);
                    }
                    Terminator::Branch {
                        true_block,
                        false_block,
                        ..
                    } => {
                        stack.push(*true_block);
                        stack.push(*false_block);
                    }
                    Terminator::Loop {
                        body_block,
                        exit_block,
                        ..
                    } => {
                        stack.push(*body_block);
                        stack.push(*exit_block);
                    }
                    Terminator::Return => {}
                }
            }
        }
        visited
    }
}

/// Variable versioning state — tracks the current version of each
/// variable during SSA construction.
#[derive(Debug, Clone)]
pub struct VersionState {
    /// Current version for each variable name (reset on restore).
    current: HashMap<String, u32>,
    /// High-water mark — the next version to assign (never rolls back).
    counter: HashMap<String, u32>,
    /// Functions known at this point in the program.
    pub known_functions: std::collections::HashSet<String>,
    /// Function IR bodies for inline expansion at call sites.
    pub function_bodies: HashMap<String, Vec<crate::ir::Statement>>,
    pub function_redirects: HashMap<String, Vec<crate::ir::Redirect>>,
    /// Variable type map from type inference.
    pub type_map: HashMap<String, crate::ir::VarType>,
    /// Variables known to be associative arrays (vs indexed).
    pub assoc_arrays: std::collections::HashSet<String>,
    /// Variables known to be any collection (array or map) — for unset semantics.
    pub collections: std::collections::HashSet<String>,
    /// Variables declared readonly — error on reassignment.
    pub readonly_vars: std::collections::HashSet<String>,
    /// DD#18: local functions currently in scope during inline expansion.
    pub local_fn_scope: std::collections::HashSet<String>,
    /// Loop context stack: (header_block, exit_block) for break/continue.
    pub loop_stack: Vec<(usize, usize)>,
    /// Return target block for `return` in inlined functions.
    pub return_target: Option<usize>,
    /// Functions that have return statements (non-void).
    pub function_non_void: std::collections::HashSet<String>,
    /// Functions declared with `-> int` return type.
    pub function_int_typed: std::collections::HashSet<String>,
    /// Depth counter for condition position (if/while condition, LHS of &&/||).
    pub in_condition: u32,
    /// Depth counter for `-` suppression (DD#55). Set while lowering the
    /// inner statement of `StmtKind::Suppressed`. Distinguishes a bare
    /// must-consume violation (SB4018) from a `-`-suppressed one (SB4019).
    pub suppressed: u32,
    /// EXIT trap body statements (if set via `trap 'body' EXIT`).
    pub exit_trap: Option<Vec<crate::ir::Statement>>,
    /// Signals registered with EXIT disposition (compiler inserts check points).
    pub exiting_signals: std::collections::HashSet<i64>,
    /// Diagnostics collected during SSA transformation.
    pub diagnostics: Vec<String>,
    /// `--manifest` collection: every command name resolved to a real
    /// executable path (contains `/`) at the resolution funnel. Interior-
    /// mutable so the immutable expression-evaluation path (`expr_to_value`
    /// → `build_argv`) can record `$( … )` externals without threading
    /// `&mut` through the whole evaluator. Surfaced onto
    /// `SsaProgram::external_deps` at the end of `ir_to_ssa_typed`.
    pub external_deps: std::cell::RefCell<std::collections::BTreeSet<String>>,
}

impl VersionState {
    pub fn with_types(type_map: HashMap<String, crate::ir::VarType>) -> Self {
        Self {
            current: HashMap::new(),
            counter: HashMap::new(),
            known_functions: std::collections::HashSet::new(),
            function_bodies: HashMap::new(),
            function_non_void: std::collections::HashSet::new(),
            function_int_typed: std::collections::HashSet::new(),
            in_condition: 0,
            suppressed: 0,
            function_redirects: HashMap::new(),
            type_map,
            assoc_arrays: std::collections::HashSet::new(),
            collections: std::collections::HashSet::new(),
            readonly_vars: std::collections::HashSet::new(),
            local_fn_scope: std::collections::HashSet::new(),
            loop_stack: Vec::new(),
            return_target: None,
            exit_trap: None,
            exiting_signals: std::collections::HashSet::new(),
            diagnostics: Vec::new(),
            external_deps: std::cell::RefCell::new(std::collections::BTreeSet::new()),
        }
    }

    /// Define a new version of a variable.
    pub fn define(&mut self, name: &str) -> VarRef {
        // Compile-time readonly enforcement: error if reassigning a readonly variable
        if self.readonly_vars.contains(name) && self.current.contains_key(name) {
            panic!("sash: {}: readonly variable — cannot reassign", name);
        }
        let next = self.counter.entry(name.to_string()).or_insert(0);
        *next += 1;
        let version = *next;
        self.current.insert(name.to_string(), version);
        let vtype = self
            .type_map
            .get(name)
            .copied()
            .unwrap_or(crate::ir::VarType::String);
        VarRef::new_typed(name, version, vtype)
    }

    /// Look up the current version of a variable.
    pub fn lookup(&self, name: &str) -> VarRef {
        let version = self.current.get(name).copied().unwrap_or(0);
        let vtype = self
            .type_map
            .get(name)
            .copied()
            .unwrap_or(crate::ir::VarType::String);
        VarRef::new_typed(name, version, vtype)
    }

    /// Save state for scope entry (if/while branches).
    pub fn snapshot(&self) -> HashMap<String, u32> {
        self.current.clone()
    }

    /// Restore current versions from a snapshot.
    /// The counter (high-water mark) is NOT restored — versions
    /// never repeat across branches.
    pub fn restore(&mut self, snap: &HashMap<String, u32>) {
        self.current = snap.clone();
    }
}
