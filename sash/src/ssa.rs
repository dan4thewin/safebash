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

use std::collections::{HashMap, HashSet, BTreeMap};

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
        Self { name: name.to_string(), version, var_type: crate::ir::VarType::String }
    }

    pub fn new_typed(name: &str, version: u32, var_type: crate::ir::VarType) -> Self {
        Self { name: name.to_string(), version, var_type }
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

/// An SSA instruction — a versioned wrapper around an IR operation.
#[derive(Debug, Clone, PartialEq)]
pub enum SsaInst {
    /// x_n = value
    Assign {
        target: VarRef,
        value: SsaValue,
    },
    /// A command (no variable defined)
    Command {
        name: String,
        args: Vec<SsaValue>,
        redirects: Vec<crate::ir::Redirect>,
    },
    /// Test expression from [[ ]]. Preserved through SSA so codegen
    /// can emit inline comparisons instead of shelling out.
    Test(crate::ir::TestExpr),
    /// export VAR=value — assign + push to environment
    Export {
        target: VarRef,
        value: SsaValue,
        name: String,  // original bash variable name (for setenv)
    },
    /// phi node
    Phi(PhiNode),
    /// read -r line: reads one line from fd, assigns to target, sets __read_eof
    ReadLine {
        target: VarRef,
        fd: i64,  // 0 = stdin
        delim: Option<u8>,  // None = newline (default), Some(byte) = custom delimiter
    },
    /// read -r var1 var2 ... varN: read line, split on whitespace into N vars
    /// Last var gets the remainder.
    ReadLineMulti {
        targets: Vec<VarRef>,
        fd: i64,
        delim: Option<u8>,
    },
    /// arr[i]=val or map[key]=val — subscript assignment
    SubscriptAssign {
        array: String,
        index: SsaValue,
        value: SsaValue,
        is_integer_key: bool,
    },
    /// arr+=(val) — append to indexed array
    ArrayAppend {
        array: String,
        value: SsaValue,
    },
    /// mapfile -t arr — read all lines from fd into indexed array
    Mapfile {
        array: String,
        fd: i64,
    },
    /// Read a line, split by delimiter, store into indexed array
    ReadArray {
        array: String,
        fd: i64,
        delim: Option<u8>,
    },
    /// Start capturing stdout (for $(func) capture)
    CaptureStart,
    /// End capturing, assign captured output to target
    CaptureEnd {
        target: VarRef,
    },
    /// Multi-stage pipeline: cmd1 | cmd2 | ...
    Pipeline {
        stages: Vec<(String, Vec<SsaValue>, Vec<crate::ir::Redirect>)>,
        /// If true, the last pipe read fd is saved for a subsequent while-read loop
        capture_read_fd: bool,
    },
}

/// Arithmetic operators for $(( )) and (( )) contexts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArithOp {
    Add, Sub, Mul, Div, Mod,
    Eq, Ne, Lt, Le, Gt, Ge,
    And, Or, Not,
    Shl, Shr, BitAnd, BitOr, BitXor,
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
            ArithOp::Add => b'+', ArithOp::Sub => b'-',
            ArithOp::Mul => b'*', ArithOp::Div => b'/',
            ArithOp::Mod => b'%',
            ArithOp::Eq => 1, ArithOp::Ne => 2,
            ArithOp::Lt => 3, ArithOp::Le => 4,
            ArithOp::Gt => 5, ArithOp::Ge => 6,
            ArithOp::And => 7, ArithOp::Or => 8,
            ArithOp::Not => 9,
            ArithOp::Shl => 10, ArithOp::Shr => 11,
            ArithOp::BitAnd => 12, ArithOp::BitOr => 13, ArithOp::BitXor => 14,
            ArithOp::Pow => 15,
        }
    }

    pub fn display(&self) -> &'static str {
        match self {
            ArithOp::Add => "+", ArithOp::Sub => "-",
            ArithOp::Mul => "*", ArithOp::Div => "/", ArithOp::Mod => "%",
            ArithOp::Eq => "==", ArithOp::Ne => "!=",
            ArithOp::Lt => "<", ArithOp::Le => "<=",
            ArithOp::Gt => ">", ArithOp::Ge => ">=",
            ArithOp::And => "&&", ArithOp::Or => "||",
            ArithOp::Not => "!",
            ArithOp::Shl => "<<", ArithOp::Shr => ">>",
            ArithOp::BitAnd => "&", ArithOp::BitOr => "|", ArithOp::BitXor => "^",
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
    /// Runtime command substitution: fork/exec, capture stdout.
    /// Carries the command name and arguments as expressions.
    CmdSubst { argv: Vec<SsaValue>, redirects: Vec<crate::ir::Redirect> },
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
    /// Associative array literal: [(k1,v1), (k2,v2), ...]
    AssocArray(Vec<(String, SsaValue)>),
    /// Associative array subscript: ${MAP[$KEY]}
    Subscript {
        array: String,
        index: Box<SsaValue>,
        is_integer_key: bool,   // true = indexed array (key is raw i64 bytes)
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
    next_block_id: usize,
}

impl SsaProgram {
    pub fn new() -> Self {
        Self {
            blocks: BTreeMap::new(),
            entry: 0,
            next_block_id: 0,
        }
    }

    pub fn new_block(&mut self, label: &str) -> BlockId {
        let id = self.next_block_id;
        self.next_block_id += 1;
        self.blocks.insert(id, BasicBlock {
            id,
            label: label.to_string(),
            phis: Vec::new(),
            insts: Vec::new(),
            terminator: Terminator::Return,
        });
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
                Terminator::Branch { true_block, false_block, .. } => {
                    if *true_block == target || *false_block == target {
                        preds.push(*id);
                    }
                }
                Terminator::Loop { body_block, exit_block, .. } => {
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
        while let Some(id) = stack.pop() {
            if !visited.insert(id) { continue; }
            if let Some(block) = self.blocks.get(&id) {
                match &block.terminator {
                    Terminator::Goto(dest) => { stack.push(*dest); }
                    Terminator::Branch { true_block, false_block, .. } => {
                        stack.push(*true_block);
                        stack.push(*false_block);
                    }
                    Terminator::Loop { body_block, exit_block, .. } => {
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
    /// Variable type map from type inference.
    pub type_map: HashMap<String, crate::ir::VarType>,
    /// Variables known to be associative arrays (vs indexed).
    pub assoc_arrays: std::collections::HashSet<String>,
    /// Variables declared readonly — error on reassignment.
    pub readonly_vars: std::collections::HashSet<String>,
    /// Loop context stack: (header_block, exit_block) for break/continue.
    pub loop_stack: Vec<(usize, usize)>,
    /// Return target block for `return` in inlined functions.
    pub return_target: Option<usize>,
    /// EXIT trap body statements (if set via `trap 'body' EXIT`).
    pub exit_trap: Option<Vec<crate::ir::Statement>>,
}

impl VersionState {
    pub fn new() -> Self {
        Self {
            current: HashMap::new(),
            counter: HashMap::new(),
            known_functions: std::collections::HashSet::new(),
            function_bodies: HashMap::new(),
            type_map: HashMap::new(),
            assoc_arrays: std::collections::HashSet::new(),
            readonly_vars: std::collections::HashSet::new(),
            loop_stack: Vec::new(),
            return_target: None,
            exit_trap: None,
        }
    }

    pub fn with_types(type_map: HashMap<String, crate::ir::VarType>) -> Self {
        Self {
            current: HashMap::new(),
            counter: HashMap::new(),
            known_functions: std::collections::HashSet::new(),
            function_bodies: HashMap::new(),
            type_map,
            assoc_arrays: std::collections::HashSet::new(),
            readonly_vars: std::collections::HashSet::new(),
            loop_stack: Vec::new(),
            return_target: None,
            exit_trap: None,
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
        let vtype = self.type_map.get(name).copied().unwrap_or(crate::ir::VarType::String);
        VarRef::new_typed(name, version, vtype)
    }

    /// Look up the current version of a variable.
    pub fn lookup(&self, name: &str) -> VarRef {
        let version = self.current.get(name).copied().unwrap_or(0);
        let vtype = self.type_map.get(name).copied().unwrap_or(crate::ir::VarType::String);
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

    /// Get all variable names that have been defined.
    pub fn defined_vars(&self) -> HashSet<String> {
        self.current.keys().cloned().collect()
    }
}
