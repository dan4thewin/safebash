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
use std::ops::RangeInclusive;

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
/// use [`BuiltinCmd::internal_name`] for the round-trip back to the
/// canonical sigil string used in `SsaInst::Command::name`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinCmd {
    /// DD#99 `write` — `echo` without the trailing newline.
    Write,
    // ─── Compiler-internal sigils (transform → codegen protocol) ───
    /// Flip test_result_var and exit_status_var (post-`!`).
    NegateTestResult,
    /// Set test_result_var from the current exit status (`test = $? == 0`).
    /// Emitted after a capture-condition (`if x=$(cmd)`): the suppressed
    /// capture leaves the command's status in `$?` but does not set the branch
    /// truth value, so this syncs it (mirrors how a plain command condition
    /// sets `test_result` from its own status).
    SetTestFromStatus,
    /// Run a `let` expression for side-effects, propagate non-zero
    /// exit per DD#55.
    LetExit,
    /// Fork point — used by background `cmd &` (DD#38: `&` is the fork
    /// mechanism). (Subshells `( … )` are rejected; this is not subshell-only.)
    ForkBackground,
    /// Capture the most recent background pid into a named variable.
    StoreBgPid,
    /// Install signal handler block.
    TrapSignal,
    /// Cooperative signal-flag check inserted at safe points.
    SignalFlag,
    /// `unset arr[key]` on a **map** — the key is the literal string, which is
    /// how a map keys its entries.
    TableDelete,
    /// `unset arr[i]` on an **indexed array**. An indexed array keys its
    /// entries by the 8 raw bytes of an i64 (`lower_subscript_raw`,
    /// codegen/values.rs), so deleting with a string key finds nothing and
    /// silently reports success. This variant carries that encoding.
    TableDeleteIdx,

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
    /// `close N` — close a file descriptor on the running process (DD#14
    /// fd-manipulation family; the sanctioned form of bash's `N>&-`, which
    /// sash rejects as redirect syntax — SB4048). Void; a failed close aborts
    /// per DD#55 unless caught/suppressed.
    Close,
    /// `attest VAR [[ ... ]]` — recognized at IR level, becomes
    /// `SsaInst::Attest` in SSA. Only appears as a `Command` in
    /// fallback paths (no TestExpr arg).
    Attest,
    Read,
    /// `mapfile` / `readarray` — RECOGNITION ONLY. DD#99 retired both and
    /// `retired_bulk_read_diag` rejects them (SB5060); this variant exists so
    /// `is_resolvable_external` does not treat the names as external programs
    /// and preempt that diagnostic with SB5011 "command not found". There is
    /// deliberately no lowering, no `SsaInst`, and no dispatch arm behind it.
    Mapfile,
    /// DD#60: `argv [N] a` fills array `a` with the program argv slice.
    Argv,
    Break,
    Continue,
    Return,
    Signal,
    Unset,

    // ─── Sash-native intrinsics ───
    /// `$(len ...)` — string or array length.
    Len,
    /// `$(strlen s)` — byte length of a scalar (DD#75 §2 amendment).
    Strlen,
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
    /// `matchall SUBJECT PATTERN ARR` — DD#63 every match. Command-only: the
    /// out-array is mandatory, since "all matches" has no single value to be.
    MatchAll,
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
    ///
    /// **Only names a user can write.** The nine compiler-internal opcodes
    /// ([`BuiltinCmd::internal_name`]) are deliberately absent. They used to
    /// be listed here, which made the compiler's private vocabulary a parse
    /// target: `__table_delete() { … }` in a script went through this
    /// classifier. Nothing needed that direction — [`SsaInst::internal`]
    /// sets `kind` directly and every backend dispatches on `kind`, so the
    /// internal spelling is write-only and all nine arms were cold. Removing
    /// them makes a `__`-prefixed name an ordinary unknown command (SB5011),
    /// which is what it always should have been.
    pub fn from_name(name: &str) -> Self {
        match name {
            "echo" => Self::Echo,
            // DD#99: `write` is `echo` minus the trailing newline.
            "write" => Self::Write,
            "[[" => Self::Test,
            ":" => Self::Noop,
            "exit" => Self::Exit,
            "exec" => Self::Exec,
            "cd" => Self::Cd,
            "wait" => Self::Wait,
            "close" => Self::Close,
            "attest" => Self::Attest,
            // DD#99: `readline` is the line-terminated scalar read. It shares
            // the scalar lowering; stage 3 gives `read` the array-only form.
            "read" | "readline" => Self::Read,
            // Recognition only — see `BuiltinCmd::Mapfile`.
            "mapfile" | "readarray" => Self::Mapfile,
            "argv" => Self::Argv,
            "break" => Self::Break,
            "continue" => Self::Continue,
            "return" => Self::Return,
            "signal" => Self::Signal,
            "unset" => Self::Unset,
            "len" => Self::Len,
            "strlen" => Self::Strlen,
            "keys" => Self::Keys,
            "join" => Self::Join,
            "sub" => Self::Sub,
            "gsub" => Self::Gsub,
            "match" => Self::Match,
            "matchall" => Self::MatchAll,
            "toupper" => Self::Toupper,
            "tolower" => Self::Tolower,
            "basename" => Self::Basename,
            "dirname" => Self::Dirname,
            _ => Self::Other,
        }
    }

    /// Display spelling for a compiler-internal opcode. Returns `None` for
    /// variants that come from source (their raw name should be used).
    ///
    /// These strings exist only because [`SsaInst::Command`] carries a
    /// `name: String` beside its `kind`, and a synthesised command has no
    /// source text to put there. They are for display — SSA printing and
    /// `--emit-ir` — and are **not** parsed back by
    /// [`BuiltinCmd::from_name`].
    pub fn internal_name(self) -> Option<&'static str> {
        Some(match self {
            Self::NegateTestResult => "__negate_test_result",
            Self::SetTestFromStatus => "__set_test_from_status",
            Self::LetExit => "__let_exit",
            Self::ForkBackground => "__fork_background",
            Self::StoreBgPid => "__store_bg_pid",
            Self::TrapSignal => "__trap_signal",
            Self::SignalFlag => "__signal_flag",
            Self::TableDelete => "__table_delete",
            Self::TableDeleteIdx => "__table_delete_idx",
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
    /// can emit inline comparisons instead of shelling out. The
    /// [`TestOrigin`] distinguishes a user-written test from an `attest`
    /// predicate, which the taint backstop must not treat as a sink.
    Test(SsaTest, TestOrigin),

    /// `readline VAR`: read one newline-terminated record from `fd` into
    /// `target` and set `__test_result` (0 at EOF). DD#99 retired the
    /// delimiter flag — a `readline` is always newline-terminated.
    ReadLine {
        target: VarRef,
        fd: i64, // 0 = stdin
        redirects: Vec<crate::ir::Redirect>,
        /// True when read is in a lastpipe position with clean producer stages
        clean_pipe: bool,
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
    /// DD#99: `map m = @{a:DISP}` — fill a map from an array, consumed
    /// PAIRWISE. `mode` is the odd-count disposition: 0 `:-KEY` (the trailing
    /// element becomes the value of `key`), 1 `:?` (abort), 2 `:~`/`:!`
    /// (discard). An empty array is a valid empty map.
    ///
    /// DD#91: `taint_off` is the `!` vouch, and it needs its own field because
    /// `mode` cannot carry it — `:~` and `:!` share mode 2 precisely because
    /// they share the *odd-count* behaviour, which is orthogonal to the taint
    /// bypass. Dropping it here made the SSA taint pass disagree with
    /// `validate` about `map m = @{a:!~}`, which DD#91 forbids.
    MapFromArray {
        source: String,
        target: VarRef,
        mode: u8,
        key: SsaValue,
        message: String,
        taint_off: bool,
    },
    /// DD#99: `read $SRC ARRAY` — split a scalar VALUE on NUL into an
    /// indexed array. The stdin form is `ReadArray`; this is the same split
    /// with the bytes coming from a value rather than a fd.
    SplitValue { source: SsaValue, target: VarRef },
    /// DD#99 `read ARRAY`: slurp `fd` and split it on NUL into an indexed
    /// array. NUL is the only separator — there is no delimiter flag.
    ///
    /// Carries its own `redirects` (as `ReadLine` does) so the DD#55
    /// open(2) guard can wrap the read itself. Bracketing it with
    /// `__redirect_push`/`__redirect_pop` instead would apply the fds but
    /// leave a failed open unchecked — a missing file read as an empty array.
    ReadArray {
        target: VarRef,
        fd: i64,
        redirects: Vec<crate::ir::Redirect>,
        /// As `ReadLine`'s: true when every producer of the pipe that feeds
        /// this read is compiler-computed, so the bytes never came from
        /// outside. Governs the SSA taint backstop's classification, and is
        /// computed by the same `validate::is_external_source` the front-end
        /// taint pass uses — a divergence here is a laundering hole.
        clean_pipe: bool,
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
        /// DD#63 `matchall`: every match rather than the first. One flag on
        /// one instruction rather than a parallel variant — the two forms
        /// differ only in which runtime symbol is called, so splitting them
        /// would mean two of every downstream arm to keep in step.
        all: bool,
    },
    /// DD#60: `argv [N] a` — fill `array` with the program argv slice
    /// argv[start..argc] (0-indexed). Auto-declares and taints the array
    /// (a bulk-read source, like `read`; DD#61).
    ArgvFill { target: VarRef, start: i64 },
    /// DD#75 §3: `keys m k` — snapshot map `source`'s string keys into the
    /// indexed array `target`. Auto-declares the array, exactly as `ArgvFill`
    /// does.
    ///
    /// Unlike `ArgvFill` the result is **clean**, and that is not an oversight.
    /// `argv` is a bulk read of bytes from outside the program (DD#61); a map's
    /// keys were all put there by the program itself, and DD#93 closes every
    /// route to a tainted map at construction — a tainted key reaches a
    /// subscript only through `attest`, which cleans it. So there is no
    /// tainted key to carry over.
    ///
    /// `source` is a bare name rather than a `VarRef` because codegen resolves
    /// a collection by name (`find_array_slot`), as every other
    /// collection-operand instruction here does.
    KeysFill { source: String, target: VarRef },
    /// Start capturing stdout (for $(func) capture). `site` identifies this
    /// capture uniquely within the program, so a `CaptureEnd` pairs with its
    /// own start rather than with whichever start ran most recently. Captures
    /// nest (`o=$(h)` where `h` contains `i=$(g)`), and a single shared pair of
    /// handle slots let the inner capture clobber the outer's — the outer then
    /// restored a closed fd (`bug-nested-function-capture-clobbers-handles`).
    /// Depth is always static: sash inlines functions and rejects recursion
    /// (SB5037), so numbering the sites at lowering time is sufficient and no
    /// runtime stack is needed.
    ///
    /// `buffered` selects the zero-syscall path: when the body provably never
    /// forks onto this capture's stdout and never rebinds fd 1
    /// (`transform::capture_purity`), the output is collected in a userspace
    /// buffer and no kernel object is created at all. Otherwise stdout is
    /// pointed at a memfd, as before.
    CaptureStart { site: u32, buffered: bool },
    /// End capturing, assign captured output to target. `site` matches the
    /// paired `CaptureStart`. `tainted` marks a capture of *external* output
    /// (a multi-statement/pipeline `$(…)` body) — its target is tainted, unlike
    /// a `$(user_fn)` capture (clean by DD#24).
    CaptureEnd {
        site: u32,
        /// Mirrors the paired `CaptureStart` — see there.
        buffered: bool,
        target: VarRef,
        tainted: bool,
    },
    /// Open a redirect *scope* around an inlined user-function body
    /// (`f > out.txt`). Saves each affected fd, applies the redirects, and is
    /// closed by the paired `RedirectPop` in the call's restore block.
    ///
    /// A plain command carries its redirects on the instruction itself, which
    /// applies and restores them in one place. A function call cannot: the
    /// body is inlined across several blocks, so there is no single
    /// instruction to hang them on. Attaching them to each command in the
    /// body instead would be wrong — `fn f { echo a; echo b }; f > out.txt`
    /// must open `out.txt` (and truncate it) **once**, not once per command.
    ///
    /// `site` numbers the scope so the backends can give each one its own
    /// saved-fd slots; scopes nest (a redirected call inside a redirected
    /// call) exactly as captures do, and for the same reason the depth is
    /// static — sash inlines functions and rejects recursion (SB5037).
    ///
    /// `checked` is DD#55: when true an `open(2)` failure writes the
    /// `<argv0>: <path>: <reason>` diagnostic and aborts. When false the call
    /// is in condition position, so the failure is *caught* — the scope sets
    /// a false test result and the block branches past the body to the
    /// `RedirectPop`, exactly as a failed open on a plain command in the same
    /// position skips that command.
    RedirectPush {
        site: u32,
        redirects: Vec<crate::ir::Redirect>,
        checked: bool,
    },
    /// Close the redirect scope opened by `RedirectPush { site }`: restore
    /// each saved fd, in reverse. `fds` mirrors the pushed redirects (the
    /// resolved fd per saved slot), so the pop needs nothing from the push
    /// beyond the site number.
    RedirectPop { site: u32, fds: Vec<i64> },
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
        /// DD#70 per-segment `!` negation, aligned with `stages`. A negated
        /// stage still runs; its exit status is inverted (0↔nonzero) before it
        /// joins the DD#55 worst-of aggregate. Only the terminal stage may be
        /// negated (enforced at parse). Empty ⇒ none.
        negated: Vec<bool>,
    },
    /// attest VAR [[ test_expr ]] — runtime validation of tainted data.
    /// If the test fails, abort with nonzero exit.
    Attest {
        target: VarRef,
        source: VarRef,
        test: SsaTest,
    },
    /// DD#61: `attest x in a [[…]]` / `attest k v in m [[…]]` — validate
    /// every element of an array/map against a per-element predicate.
    /// Binds each element (indexed: `[value_binder]`; map: `[key, value]`)
    /// and evaluates `test`. Sets the test-result var to the all-passed
    /// boolean (vacuously true for an empty collection). When `if_form`
    /// is false (statement form) a failing element aborts with exit 1
    /// instead; when true the boolean is left for a surrounding branch.
    AttestAll {
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
    /// DD#61: retag an array/map as clean after a successful `AttestAll` —
    /// copies the source slot (the collection's backing table) to a new
    /// clean SSA version. No runtime validation (already done by AttestAll).
    AttestRetag { target: VarRef, source: VarRef },
}

/// Why a [`SsaInst::Test`] exists in SSA.
///
/// The `if attest` / `attest &&` / `attest ||` forms reach SSA as branch
/// machinery — a bare `SsaInst::Test` in a condition block — which is
/// structurally identical to a user-written `[[ ]]`. They are not the same
/// thing to the taint backstop: reading the tainted subject is the entire
/// point of an attest, and is exactly the exemption `SsaInst::Attest` gets on
/// the bare form. Without this tag, turning `Test` into a sink rejects every
/// caught-attest program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestOrigin {
    /// A user-written `[[ ]]`, a `case`/dispatch pattern test, or compiler
    /// machinery implementing a disposition.
    Plain,
    /// The predicate of an `attest`.
    AttestPredicate,
}

/// One operand read by an instruction, as seen by the taint backstop.
///
/// Two shapes, because redirects ride through SSA **unlowered**: a
/// [`crate::ir::Redirect`]'s target is still an `ir::Expr` and never becomes
/// an [`SsaValue`]. That is the structural reason a value-oriented walk could
/// not see `echo pwned > $h` — there was no SSA value to look at.
pub(crate) enum TaintRead<'a> {
    Value(&'a SsaValue),
    RedirectTarget(&'a crate::ir::Expr),
}

fn value_reads<'a>(values: &'a [SsaValue], ctx: String) -> Vec<(TaintRead<'a>, String)> {
    values
        .iter()
        .map(|v| (TaintRead::Value(v), ctx.clone()))
        .collect()
}

fn redirect_reads<'a>(
    redirects: &'a [crate::ir::Redirect],
    ctx: String,
) -> Vec<(TaintRead<'a>, String)> {
    redirects
        .iter()
        .map(|r| (TaintRead::RedirectTarget(&r.target), ctx.clone()))
        .collect()
}

/// **Total over [`SsaTest`] on purpose — do not add a wildcard.**
fn push_test_reads<'a>(t: &'a SsaTest, out: &mut Vec<(TaintRead<'a>, String)>) {
    let ctx = || "a test operand".to_string();
    match t {
        SsaTest::Unary { operand, .. } => out.push((TaintRead::Value(operand), ctx())),
        SsaTest::Binary { left, right, .. } => {
            out.push((TaintRead::Value(left), ctx()));
            match right {
                SsaTestRhs::Value(v) => out.push((TaintRead::Value(v), ctx())),
                SsaTestRhs::Pattern(_) => {}
            }
        }
        SsaTest::Not(inner) => push_test_reads(inner, out),
        SsaTest::And(a, b) | SsaTest::Or(a, b) => {
            push_test_reads(a, out);
            push_test_reads(b, out);
        }
    }
}

impl SsaInst {
    /// Every operand this instruction reads in a position where a tainted
    /// value would **escape** — to an external process, to output, to a file
    /// descriptor, to a subscript index, or into a collection whose taint the
    /// backstop does not re-derive — paired with a description of the sink
    /// for the diagnostic.
    ///
    /// **Total over [`SsaInst`] on purpose — do not add a wildcard.** This is
    /// the one place that enumerates read positions, so rustc's exhaustiveness
    /// check is the enforcement: adding a variant has to be a compile error
    /// here rather than a silent gap in `ssa_taint`. The four-arm match with a
    /// `_ => {}` that this replaced is exactly how `Test` and `RedirectPush`
    /// escaped — both are listed in `classify_inst_def` (the write side, which
    /// has always been exhaustive) and neither was listed on the read side.
    ///
    /// The positions deliberately absent are *propagation*, not escape:
    /// `Assign`'s value flows into its target, `Attest`/`AttestAll` read their
    /// source because validating it is the entire point of the form, and
    /// `CaptureEnd` defines rather than reads. `classify_inst_def` owns those.
    pub(crate) fn taint_reads(&self) -> Vec<(TaintRead<'_>, String)> {
        match self {
            SsaInst::Command {
                name,
                args,
                redirects,
                ..
            } => {
                let mut v = value_reads(args, format!("an argument to '{}'", name));
                v.extend(redirect_reads(
                    redirects,
                    format!("a redirect target of '{}'", name),
                ));
                v
            }
            SsaInst::Pipeline { stages, .. } => stages
                .iter()
                .flat_map(|(name, args, redirects)| {
                    let mut v =
                        value_reads(args, format!("an argument to '{}' in a pipeline", name));
                    v.extend(redirect_reads(
                        redirects,
                        format!("a redirect target of '{}' in a pipeline", name),
                    ));
                    v
                })
                .collect(),
            // An attest predicate reads its tainted subject by design; see
            // `TestOrigin`. The same exemption the bare form gets below.
            SsaInst::Test(_, TestOrigin::AttestPredicate) => vec![],
            SsaInst::Test(t, TestOrigin::Plain) => {
                let mut v = Vec::new();
                push_test_reads(t, &mut v);
                v
            }
            SsaInst::ReadLine { redirects, .. } => {
                redirect_reads(redirects, "a redirect target of `readline`".to_string())
            }
            SsaInst::ReadArray { redirects, .. } => {
                redirect_reads(redirects, "a redirect target of `read`".to_string())
            }
            SsaInst::RedirectPush { redirects, .. } => {
                redirect_reads(redirects, "a redirect target".to_string())
            }
            SsaInst::SubscriptAssign {
                array,
                index,
                value,
                ..
            } => vec![
                (
                    TaintRead::Value(index),
                    format!("a subscript index of '{}'", array),
                ),
                (
                    TaintRead::Value(value),
                    format!("a subscript value of '{}'", array),
                ),
            ],
            SsaInst::ArrayAppend { array, value } => vec![(
                TaintRead::Value(value),
                format!("an appended element of '{}'", array),
            )],
            SsaInst::MapFromArray { target, key, .. } => vec![(
                TaintRead::Value(key),
                format!("a map key filling '{}'", target.name),
            )],
            SsaInst::SplitValue { source, target } => vec![(
                TaintRead::Value(source),
                format!("the split source for '{}'", target.name),
            )],
            SsaInst::MatchInto {
                subject,
                pattern,
                array,
                ..
            } => vec![
                (
                    TaintRead::Value(subject),
                    format!("the match subject for '{}'", array),
                ),
                (
                    TaintRead::Value(pattern),
                    format!("the match pattern for '{}'", array),
                ),
            ],
            // Propagation and definition sites. A tainted value here flows
            // into a target whose own taint is derived from it by
            // `classify_inst_def`; it does not escape. The last three read no
            // `SsaValue` at all.
            SsaInst::Assign { .. }
            | SsaInst::Attest { .. }
            | SsaInst::AttestAll { .. }
            | SsaInst::AttestRetag { .. }
            | SsaInst::CaptureEnd { .. }
            | SsaInst::ArgvFill { .. }
            | SsaInst::KeysFill { .. }
            | SsaInst::CaptureStart { .. }
            | SsaInst::RedirectPop { .. } => vec![],
        }
    }

    /// `Some((site, slot count))` when this is a redirect-scope push.
    /// Takes `&Self` by reference so it can be passed to `filter_map`
    /// directly over an iterator of instruction references.
    fn redirect_scope_width(inst: &Self) -> Option<(usize, usize)> {
        match inst {
            SsaInst::RedirectPush {
                site, redirects, ..
            } => Some((*site as usize, redirects.len())),
            _ => None,
        }
    }

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

    /// Construct an `SsaInst::Command` for a compiler-internal opcode —
    /// one the compiler synthesises rather than reading from source (the
    /// `&` fork, a table delete, a trap install).
    ///
    /// # Panics
    ///
    /// If `kind` has no internal spelling, i.e. it is a variant that comes
    /// from source. Such a command must be built with
    /// [`SsaInst::command`], which classifies the name it was given.
    pub fn internal(kind: BuiltinCmd, args: Vec<SsaValue>, checked: bool) -> Self {
        let name = kind
            .internal_name()
            .expect("SsaInst::internal requires a compiler-internal variant")
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

/// The three overflow-checked binary ops (DD#25) — a narrowing of `ArithOp` so
/// the checked-arith emitters (native + C) are total by construction, with no
/// "only handles Add/Sub/Mul" ICE arm. The caller maps each `ArithOp` variant
/// to this at the dispatch site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckedArithOp {
    Add,
    Sub,
    Mul,
}

impl CheckedArithOp {
    /// The overflow-checked ops (`+`/`-`/`*`), `None` for the rest.
    ///
    /// **Total over `ArithOp` on purpose — do not add a wildcard.** This read
    /// `_ => None` until 2026-10-04, which meant a new `ArithOp` would be
    /// silently *unchecked*: it would compile, run, and wrap, and nothing in
    /// the tree would say so. DG's rule is that a new op must stop the build
    /// until someone has explicitly given it a check or an exemption, so every
    /// variant is named and the exemption for each is on the record below.
    ///
    /// The `None` arms are not "not yet done" — each is overflow-free, or is
    /// checked somewhere other than `CheckedArithOp` (verified by probe
    /// 2026-10-04):
    ///
    /// - `Div` traps on `i64::MIN / -1` in `emit_div_or_mod_with_zero_check`,
    ///   which also carries bash's div-by-zero semantics; `Mod` cannot
    ///   overflow (`i64::MIN % -1` is 0).
    /// - `Pow` routes through the `sash_pow` runtime helper, which does its
    ///   own check — `2 ** 70` aborts.
    /// - `Shl`/`Shr` have no overflow to detect: DD#25's 2026-09-20 addendum
    ///   made the count unbounded, so a count >= 64 shifts every bit out.
    /// - `Eq`/`Ne`/`Lt`/`Le`/`Gt`/`Ge`/`And`/`Or` yield 0 or 1.
    /// - `BitAnd`/`BitOr`/`BitXor` are bitwise and cannot leave the range.
    pub fn from_arith(op: ArithOp) -> Option<Self> {
        match op {
            ArithOp::Add => Some(Self::Add),
            ArithOp::Sub => Some(Self::Sub),
            ArithOp::Mul => Some(Self::Mul),
            ArithOp::Div | ArithOp::Mod => None,
            ArithOp::Pow => None,
            ArithOp::Shl | ArithOp::Shr => None,
            ArithOp::Eq
            | ArithOp::Ne
            | ArithOp::Lt
            | ArithOp::Le
            | ArithOp::Gt
            | ArithOp::Ge
            | ArithOp::And
            | ArithOp::Or => None,
            ArithOp::BitAnd | ArithOp::BitOr | ArithOp::BitXor => None,
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
    /// DD#81: call a user function in `$( )`, capturing its stdout as a
    /// string. Carries its argument values so the capture sets up the callee's
    /// `args` array (like `IntFuncCall`).
    ///
    /// DD#100: `colls` are the `=name` collection arguments, which bind slots
    /// (`A`..`E`) rather than contributing to `args`. They must ride on the
    /// value because the capture is materialized later, by `lift_str_func_call`
    /// — which has no access to the original `Expr` list. Dropping them left
    /// the slot unbound, which segfaulted at runtime (`$(f =q)`).
    FuncCall {
        name: String,
        args: Vec<SsaValue>,
        colls: Vec<String>,
    },
    /// DD#58: Call a -> int function in arithmetic context.
    /// Produces an integer return value, not stdout.
    ///
    /// DD#100: `colls` as for `FuncCall` — an unbound slot here ICEd in
    /// codegen ("undefined arrlen: A").
    IntFuncCall {
        name: String,
        args: Vec<SsaValue>,
        colls: Vec<String>,
    },
    /// Runtime command substitution: fork/exec, capture stdout.
    /// Carries the command name and arguments as expressions. `suppressed` is
    /// set for `$(- cmd)` (DD#55): the captured command's exit status is
    /// discarded and a nonzero exit does NOT abort (mirrors statement-position
    /// `- cmd`).
    ///
    /// Carries **no redirects**: a `$(cmd > f)` is routed to `CmdSubstBody`
    /// by `single_command_subst`, so every redirected capture is lowered by
    /// the ordinary statement machinery. This variant once had a `redirects`
    /// field that only the native backend honoured — the C backend dropped it
    /// — which is how `$(/bin/echo hi > f)` captured `hi` and never wrote `f`.
    CmdSubst {
        argv: Vec<SsaValue>,
        suppressed: bool,
    },
    /// A **multi-statement / pipeline** command substitution body
    /// (`$(x=1; echo $x)`, `$(a | b)`). A transform-only intermediate: it is
    /// lifted out of value position into a `capture_body_into` temp by
    /// `lift_int_func_calls` (exactly like `IntFuncCall`), so it must never
    /// reach codegen (ICE if it does). The body captures external output, so
    /// the lifted temp is tainted.
    /// Any redirects live on the body's own statements, where both backends
    /// already lower them; this variant carries none of its own.
    CmdSubstBody {
        body: Vec<crate::ir::Statement>,
    },
    /// `$(<file)` — slurp a file's contents. The inner value is the **path**.
    ///
    /// Not a `CmdSubst`: it forks nothing and execs nothing. It lowers to a
    /// single `sash_slurp_file` call, which opens/reads/closes and strips
    /// trailing newlines through the same drain the general capture uses, so
    /// the two cannot drift on that rule.
    ///
    /// **Tainted**, and deliberately so: file contents are external input. The
    /// old `$(cat file)` desugar got that for free by being an external
    /// capture; here it must be asserted (`ssa_taint.rs`).
    SlurpFile(Box<SsaValue>),
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
    /// DD#102: `$(strlen X)` — byte length of an ordinary value operand,
    /// exactly like `$(toupper X)` takes one. `strlen` used to resolve its
    /// operand as a *name* while every other non-collection intrinsic
    /// resolved a value, so `strlen s` asked about the variable `s` while the
    /// adjacent `toupper s` asked about the two-letter string `"s"`.
    StrLen {
        subject: Box<SsaValue>,
    },
    /// DD#65: `${var:-DEFAULT}` — `source` if set/non-empty, else the DEFAULT.
    /// The default is a full interpolating value (unlike the literal-only
    /// `ParamExpansion` operand), so `${foo:-$bar}` interpolates `bar`.
    /// `source` is a plain `Var` for a scalar `${s:-…}`, or a `Subscript` for
    /// an array/map cell `${a[i]:-…}` (DG: a cell behaves like a variable).
    DefaultValue {
        source: Box<SsaValue>,
        default: Box<SsaValue>,
    },
    /// DD#77: `${var:?MESSAGE}` — `source` if set/non-empty, else abort
    /// (`<prog>: MESSAGE` on stderr, exit 1). The `:-`/`:?` abort/default pair;
    /// the message interpolates like the `:-` default. `source` is a `Var`
    /// (scalar) or `Subscript` (cell), as for `DefaultValue`.
    RequireValue {
        source: Box<SsaValue>,
        message: Box<SsaValue>,
    },
    /// DD#84: `${x:~}` / `${args[i]:~}` (elide) — in an elidable position (an
    /// external/builtin arg list or an array literal), contributes *zero*
    /// tokens when `source` is the identity element (empty string) and one
    /// token (the value) otherwise. Only valid where a token is one of a
    /// sequence; a non-elidable position is rejected at validate (SB5046), so
    /// this never reaches a scalar `lower_value`.
    ///
    /// DD#91: `taint_off` is set for the `:!~` spelling — the elide behaviour
    /// is identical, but the value is treated as **clean** (a predicate-less
    /// attest, a sanctioned per-use taint bypass). Only the taint passes read
    /// the flag; codegen/backends ignore it. Bare `:!` is not an elide at all
    /// and uses `Trusted` instead.
    ElideEmpty {
        source: Box<SsaValue>,
        taint_off: bool,
    },
    /// DD#91 (2026-09-09 addendum): `${x:!}` — the value, trusted. The `!`
    /// modifier with no disposition.
    ///
    /// Purely a taint annotation: codegen lowers it as its `source` and the
    /// emitted code is identical. It exists because the CST is not the only
    /// taint layer — without a node here the SSA backstop would see a bare
    /// tainted read and reject a bypass the author explicitly asked for. The
    /// combinations (`:!~`, `:!-W`, `:!?M`) carry the flag on their own node
    /// instead; only the bare spelling needs a wrapper.
    Trusted {
        source: Box<SsaValue>,
    },
    /// `${a[i]:off:len}` — substring of a value `source` (a `Subscript` cell
    /// read; the scalar `${s:o:l}` uses `ParamExpansion` "substring"). `spec` is
    /// the `off` / `off:len` operand. Mirrors `DefaultValue`'s source carrying,
    /// so a cell behaves like a string variable (DG, edge 4).
    Substring {
        source: Box<SsaValue>,
        spec: String,
    },
    /// Associative array literal: [(k1,v1), (k2,v2), ...]
    AssocArray(Vec<(String, SsaValue)>),
    /// Associative array subscript: ${MAP[$KEY]}
    Subscript {
        array: String,
        index: Box<SsaValue>,
        is_integer_key: bool,   // true = indexed array (key is raw i64 bytes)
        is_integer_value: bool, // DD#76: true = int-valued cell (val_len IS the i64)
    },
    /// Positional parameter by index: argv[i+1] (for dynamic for loops)
    ArgvIndex(VarRef),
    /// Indexed array literal: (val0, val1, val2, ...)
    /// Creates a hash table with integer keys "0", "1", "2", ...
    IndexedArray(Vec<SsaValue>),
    /// DD#76: store the inner value (evaluated as an i64) as an *int cell* —
    /// `(val_ptr = null, val_len = the i64)`. Wraps each element of an
    /// `int array`/`int map` literal and the RHS of an `int`-collection
    /// element assignment, so the generic table-store path (`lower_value`)
    /// emits the int-cell encoding with no per-site plumbing.
    IntCell(Box<SsaValue>),
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
    /// `has_word = operand.is_some()`, which is what distinguishes `:-` from
    /// `:~`. Per DD#84's 2026-09-04 addendum `:-` ALWAYS carries an operand —
    /// bare `@{a:-}` parses as `Some("")` — so an empty array yields exactly
    /// one arg (WORD, or the empty string), matching scalar `${x:-}`. Only
    /// `@{a:~}` has none, and only it yields zero args. Do not read this as
    /// "bare `:-` elides"; that was true before 2026-09-04 and is the reason
    /// `call_arg_bounds` gives `@{a:-}` a minimum of 1, not 0.
    pub has_word: bool,
    /// `:?` — on empty, abort (exit 1, `<progname>: WORD`) instead of
    /// substituting.
    pub abort: bool,
    /// DD#91: `@{a:!DISP}` (`taint_off`) — the splatted
    /// elements are trusted clean (a per-use attest-all bypass). Only the taint
    /// passes read this; codegen ignores it (the runtime splat is identical to
    /// a wordless `arrvals_default`).
    pub taint_off: bool,
}

impl SsaValue {
    /// The nested `SsaValue` operands directly owned by this value, for passes
    /// that recurse uniformly into sub-values (literal interning, capture
    /// detection, positional scanning). Mirrors the `Statement::child_bodies`
    /// pattern.
    ///
    /// **Total over `SsaValue` on purpose — do not add a wildcard.** Every
    /// leaf spells out its own empty arm so a new variant is a compile error
    /// here rather than a silently dropped child. The wildcard this replaced
    /// dropped four children, and one of them was a live taint bug:
    /// `ArraySplat`'s `:-WORD` never reached `value_is_tainted`, so
    /// `array b=( @{a:-$tainted} )` derived `b` CLEAN.
    ///
    /// Note what is NOT a child: the string-keyed halves of `AssocArray` and
    /// `ParamExpansion`'s `operator`/`operand` are `String`s, not values, and
    /// are interned by their own callers; `CmdSubstBody`'s body is a
    /// statement list, walked as IR.
    pub(crate) fn child_values(&self) -> Vec<&SsaValue> {
        match self {
            // Leaves: nothing nested to recurse into.
            SsaValue::Literal(_)
            | SsaValue::Var(_)
            | SsaValue::Expr(_)
            | SsaValue::CmdSubstBody { .. }
            | SsaValue::ArithExpr { .. }
            | SsaValue::ParamExpansion { .. }
            | SsaValue::ArgvIndex(_)
            | SsaValue::TableValue { .. }
            | SsaValue::TableKey { .. } => Vec::new(),

            // Sequences.
            SsaValue::Concat(parts) | SsaValue::IndexedArray(parts) => parts.iter().collect(),
            SsaValue::CmdSubst { argv, .. } => argv.iter().collect(),
            SsaValue::FuncCall { args, .. } | SsaValue::IntFuncCall { args, .. } => {
                args.iter().collect()
            }
            SsaValue::AssocArray(pairs) => pairs.iter().map(|(_, v)| v).collect(),

            // Single-subject wrappers.
            SsaValue::Subscript { index, .. } => vec![index],
            SsaValue::CaseConvert { subject, .. } => vec![subject],
            SsaValue::StrLen { subject } => vec![subject],
            SsaValue::ElideEmpty { source, .. } => vec![source],
            SsaValue::Trusted { source } => vec![source],
            SsaValue::Substring { source, .. } => vec![source],
            SsaValue::IntCell(inner) => vec![inner],
            // The child is the *path*. It must be reachable here so the literal
            // interner sees it (a `$(<f.txt)` path that is never interned
            // panics in codegen), even though taint deliberately does NOT
            // inherit from it — see `ssa_taint.rs`.
            SsaValue::SlurpFile(path) => vec![path],

            // Multi-operand.
            SsaValue::RegexReplace {
                subject,
                pattern,
                replacement,
                ..
            } => vec![subject, pattern, replacement],
            SsaValue::RegexMatch { subject, pattern } => vec![subject, pattern],
            SsaValue::DefaultValue { source, default } => vec![source, default],
            SsaValue::RequireValue { source, message } => vec![source, message],
            SsaValue::ArithBinOp { left, right, .. } => vec![left, right],
            SsaValue::Ternary {
                cond,
                true_val,
                false_val,
            } => vec![cond, true_val, false_val],

            // DD#69: the splat's `:-WORD` / `:?WORD` default.
            SsaValue::ArraySplat { disposition, .. } => disposition
                .as_ref()
                .map(|d| vec![&*d.word])
                .unwrap_or_default(),
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

/// DD#55: index of the instruction sitting in **condition position**, or
/// `None` if this block has none.
///
/// A redirect `open(2)` failure on that instruction is *caught* by the
/// enclosing construct rather than aborting the program, so both backends
/// must agree on which instruction it is. They previously did not, and each
/// disagreement was a bug:
///
/// - the terminator must be `Branch` **or** `Loop`. `if`/`&&`/`||` end in a
///   `Branch`, `while`/`until` in a `Loop`; DD#55 lists them together as
///   condition position. Native tested `Branch` alone, so a failed open in a
///   `while` condition aborted instead of ending the loop.
/// - the scan runs in reverse and **skips `__negate_test_result`**. DD#70
///   places that sigil *after* the operand it negates, and DD#55 counts the
///   operand of `!` as condition position. Both backends took the sigil for
///   the condition, so `if ! cmd > /bad/f` aborted in both.
///
/// The reads are included alongside Command/Pipeline because they carry
/// their own redirects and run the same open(2) guard: `if read a < f` must
/// catch exactly as `if echo > f` does. This is deliberately a wider set
/// than transform's `uncheck_last_condition_inst`, which answers a different
/// question — whether a nonzero *status* aborts — and so covers only the
/// status-bearing Command/Pipeline.
pub(crate) fn condition_inst_index(block: &BasicBlock) -> Option<usize> {
    if !matches!(
        block.terminator,
        Terminator::Branch { .. } | Terminator::Loop { .. }
    ) {
        return None;
    }
    block.insts.iter().rposition(|i| match i {
        SsaInst::Command { name, .. } if name == "__negate_test_result" => false,
        SsaInst::Command { .. }
        | SsaInst::Pipeline { .. }
        | SsaInst::ReadLine { .. }
        | SsaInst::ReadArray { .. }
        // A redirect scope (`f > out.txt`) ends its block with the Branch
        // that skips the body when the open fails, so the scope — not some
        // earlier command that happens to share the block — is the
        // instruction in condition position.
        | SsaInst::RedirectPush { .. } => true,
        _ => false,
    })
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

impl Terminator {
    /// The blocks control can transfer to from this terminator.
    fn successors(&self) -> Vec<BlockId> {
        match self {
            Terminator::Goto(dest) => vec![*dest],
            Terminator::Branch {
                true_block,
                false_block,
                ..
            } => vec![*true_block, *false_block],
            Terminator::Loop {
                body_block,
                exit_block,
                ..
            } => vec![*body_block, *exit_block],
            Terminator::Return => Vec::new(),
        }
    }
}

impl SsaProgram {
    /// Highest `CaptureStart`/`CaptureEnd` site number in the program, plus
    /// one — i.e. how many capture slot pairs a backend must allocate. Zero
    /// when the program captures nothing.
    pub fn capture_site_count(&self) -> usize {
        let mut max: Option<u32> = None;
        for block in self.blocks.values() {
            for inst in &block.insts {
                if let SsaInst::CaptureStart { site, .. } | SsaInst::CaptureEnd { site, .. } = inst
                {
                    max = Some(max.map_or(*site, |m| m.max(*site)));
                }
            }
        }
        max.map_or(0, |m| m as usize + 1)
    }
    /// How many saved-fd slots each `RedirectPush` site needs, indexed by
    /// site. A backend allocates one slot per redirect in the scope, so the
    /// width varies per site (unlike a capture's fixed pair). Empty when the
    /// program opens no redirect scope.
    pub fn redirect_site_widths(&self) -> Vec<usize> {
        let scopes = self
            .blocks
            .values()
            .flat_map(|block| &block.insts)
            .filter_map(SsaInst::redirect_scope_width);
        let mut widths: Vec<usize> = Vec::new();
        for (site, width) in scopes {
            if widths.len() <= site {
                widths.resize(site + 1, 0);
            }
            widths[site] = width;
        }
        widths
    }
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

    /// Return set of block IDs reachable from block 0.
    pub fn reachable(&self) -> std::collections::HashSet<BlockId> {
        let mut roots = vec![0usize];
        // Also include compound pipeline blocks as roots
        roots.extend(self.compound_pipeline_blocks.iter().copied());
        self.reachable_from(&roots)
    }

    /// The blocks that make up the END handler (DD#56): `end_block` and
    /// everything reachable from it. Empty when the program has no END.
    ///
    /// A label test is *not* equivalent. `install_end_block` lowers the
    /// trap body with `lower_stmts`, so only the entry block is named
    /// `__end__` — an `if`/`for` inside END produces ordinary block
    /// labels. Both the transform's unchecked marking and codegen's abort
    /// routing need the whole region, and they share this so the two
    /// cannot disagree about where END ends.
    pub fn end_region(&self) -> std::collections::HashSet<BlockId> {
        match self.end_block {
            Some(b) => self.reachable_from(&[b]),
            None => std::collections::HashSet::new(),
        }
    }

    /// Blocks reachable from `roots` by following terminators.
    pub fn reachable_from(&self, roots: &[BlockId]) -> std::collections::HashSet<BlockId> {
        let mut visited = std::collections::HashSet::new();
        let mut stack: Vec<BlockId> = roots.to_vec();
        while let Some(id) = stack.pop() {
            if !visited.insert(id) {
                continue;
            }
            stack.extend(self.successors_of(id));
        }
        visited
    }

    /// The blocks control can reach from `id`. Empty for an unknown id.
    fn successors_of(&self, id: BlockId) -> Vec<BlockId> {
        self.blocks
            .get(&id)
            .map(|b| b.terminator.successors())
            .unwrap_or_default()
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
    /// Variables known to be any collection (array or map) — for unset semantics.
    pub collections: std::collections::HashSet<String>,
    /// Variables declared readonly — error on reassignment.
    pub readonly_vars: std::collections::HashSet<String>,
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
    /// DD#75 §5: the unique array name backing the current function's `args`
    /// (e.g. `__args3`). Because `args` is compiler-managed, each inline gets a
    /// fresh name — nested/recursive calls never collide, so no SSA save/restore
    /// is needed (the callee's array is a different name). Empty at top level.
    pub current_args_name: String,
    /// Monotonic counter minting the unique `args` array names above.
    pub args_counter: u32,
    /// DD#100: the collection-parameter slots in scope — `A`..`E` mapped to
    /// the caller's actual collection name. Empty at top level. Every call is
    /// inlined, so a slot is a pure compile-time name binding with no runtime
    /// representation: `resolve_args_name` substitutes the bound name, which
    /// is why the callee's writes land on the caller's collection (Part E).
    pub collection_slots: HashMap<String, String>,
    /// DD#27/§5: the stack of function names currently being inlined on the
    /// active path (push on inline-enter, pop on inline-exit). A call to a name
    /// already on the stack is a recursive edge — rejected (SB5033) unless it is
    /// a direct self tail-call (a `return F …`), which lowers to a loop.
    pub inlining: Vec<String>,
    /// DD#27/§5: when inlining a function, the target for a self tail-call
    /// (`return F …`): (function name, loop-header block, its `args` array
    /// name). A `return <call to F>` rebinds `args` and branches to the header
    /// instead of inlining again — turning tail recursion into a loop. `None`
    /// outside a function body.
    pub tail_loop: Option<(String, usize, String)>,
    /// `--manifest` collection: every command name resolved to a real
    /// executable path (contains `/`) at the resolution funnel. Interior-
    /// mutable so the immutable expression-evaluation path (`expr_to_value`
    /// → `build_argv`) can record `$( … )` externals without threading
    /// `&mut` through the whole evaluator. Surfaced onto
    /// `SsaProgram::external_deps` at the end of `ir_to_ssa_typed`.
    pub external_deps: std::cell::RefCell<std::collections::BTreeSet<String>>,

    /// DD#81 Part A: bareword command names resolved as an *external* command
    /// (not a defined-so-far function). Interior-mutable so the immutable
    /// `$( … )` expression path can record. On `fn NAME`, a name already here is
    /// a command/function shadowing collision (SB5041) — one name, one meaning.
    /// DD#81: true while lowering a function's standalone `fn_` *template*
    /// block (a dead template — every real call inlines). External command
    /// resolutions here are not real execution order, so they must not feed the
    /// SB5041 collision set (else a forward-referenced callee in a body would
    /// look like an external use before its definition).
    pub lowering_fn_template: bool,
    pub externally_used: std::cell::RefCell<std::collections::HashSet<String>>,

    /// Next unused `SsaInst::CaptureStart`/`CaptureEnd` site number. Handed out
    /// once per `$( … )` capture that is lowered, so each capture owns its own
    /// handle slots instead of sharing one program-wide pair.
    pub next_capture_site: u32,
    /// Next unused `RedirectPush`/`RedirectPop` scope number, handed out by
    /// `alloc_redirect_site`. Same rationale as `next_capture_site`.
    pub next_redirect_site: u32,
    /// A redirect waiting for the user-function call it was written on
    /// (`f > out.txt`) to be inlined. `lower_redirected_stmt` parks it here
    /// and `inline_function_call` takes it, because the two are separated by
    /// the whole command-dispatch path and only the inliner holds the blocks
    /// the scope has to wrap. Always `None` outside that hand-off: the
    /// inliner takes it before lowering the body, so a nested call inside the
    /// body cannot pick it up.
    pub pending_redirect_scope: Option<Vec<crate::ir::Redirect>>,
}

/// Permitted argument counts for a value-producing intrinsic (`len`, `sub`, …)
/// when used as a `$( )` head or an arith sexpr callee. `None` for genuinely
/// variadic intrinsics (`join`, `echo`) and non-intrinsics. Shared by validate
/// (SB4045 arity check, both spellings) and the transform backstop that keeps
/// a malformed intrinsic from falling through to a bare external command.
///
/// A *range* rather than a count because `wait` takes 0 or 1 (DD#126) and the
/// alternative was a second table for the one name that needed it — which is
/// the shape that let `$(( (strlen a b) ))` stay silent while `$(strlen a b)`
/// reported SB4045.
pub(crate) fn value_intrinsic_arity(name: &str) -> Option<RangeInclusive<usize>> {
    match name {
        // Nullary: the value comes from a global or a syscall, never an
        // argument. Without an entry here an extra argument was silently
        // discarded (`$(getpid X)` yielded the pid).
        "status" | "getpid" | "getppid" | "getcwd" | "epochseconds" | "argc" | "signum"
        | "random" => Some(0..=0),
        // DD#126: `wait` reaps any child, `wait PID` reaps that one.
        "wait" => Some(0..=1),
        "len" | "strlen" | "keys" | "toupper" | "tolower" | "basename" | "dirname" | "getenv" => {
            Some(1..=1)
        }
        "match" => Some(2..=2),
        "sub" | "gsub" => Some(3..=3),
        // `arg` is genuinely variadic (DD#60: `$(arg N)` / `$(arg N DEFAULT)`),
        // as are `join` and `echo`, so they have no arity to check.
        _ => None,
    }
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
            type_map,
            assoc_arrays: std::collections::HashSet::new(),
            collections: std::collections::HashSet::new(),
            readonly_vars: std::collections::HashSet::new(),
            loop_stack: Vec::new(),
            return_target: None,
            exit_trap: None,
            exiting_signals: std::collections::HashSet::new(),
            diagnostics: Vec::new(),
            current_args_name: String::new(),
            args_counter: 0,
            collection_slots: HashMap::new(),
            inlining: Vec::new(),
            tail_loop: None,
            external_deps: std::cell::RefCell::new(std::collections::BTreeSet::new()),
            lowering_fn_template: false,
            externally_used: std::cell::RefCell::new(std::collections::HashSet::new()),
            next_capture_site: 0,
            next_redirect_site: 0,
            pending_redirect_scope: None,
        }
    }

    /// Reserve a capture site number for one `$( … )` capture.
    pub fn alloc_capture_site(&mut self) -> u32 {
        let site = self.next_capture_site;
        self.next_capture_site += 1;
        site
    }

    /// Reserve a scope number for one `f > file` redirect bracket.
    pub fn alloc_redirect_site(&mut self) -> u32 {
        let site = self.next_redirect_site;
        self.next_redirect_site += 1;
        site
    }

    /// Define a new version of a variable.
    pub fn define(&mut self, name: &str) -> VarRef {
        // Readonly enforcement backstop. validate.rs (SB4044) is the spanned,
        // user-facing check; this catches any mutation channel it misses so a
        // readonly reassignment can never reach a panic. Spanless — record the
        // diagnostic and fall through, letting lowering continue to surface any
        // further errors.
        if self.readonly_vars.contains(name) && self.current.contains_key(name) {
            self.diagnostics.push(format!(
                "1:1: error [SB4044] readonly variable `{name}` cannot be reassigned — \
                 a `const`/`readonly` binding is immutable"
            ));
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

/// `IntCell` is a **storage** annotation, never an expression.
///
/// It is built at exactly four sites — the two `int`-collection literal
/// wrappers and the two `int`-cell subscript-assignment RHS arms — and each
/// produces a value on its way *into* a table cell. Both string-context
/// lowerers (`codegen::values::lower_value`, `emit_c::lower_value`) strip the
/// wrapper and recurse on the inner value, and no site wraps a value that is
/// already an `IntCell`, so nesting cannot occur either. An `IntCell` can
/// therefore never arrive in **int** context.
///
/// The arms that used to handle it there simply unwrapped and recursed, which
/// is the right answer and is exactly what made them worth removing: a correct
/// silent unwrap would paper over a new construction site putting a storage
/// annotation in an expression position, rather than reporting it. Same shape
/// as the `intern_or_declare` fallback (journal 375).
pub(crate) fn int_cell_in_int_context(backend: &str) -> ! {
    panic!(
        "ICE: {backend} reached int context with an SsaValue::IntCell still \
         wrapped. IntCell is a storage annotation applied on the way into a \
         table cell; every consumer strips it first. A new construction site \
         has put one in an expression position."
    )
}

/// One part of a substring operand, as an integer constant.
///
/// An empty part is the omitted length of `${s:1:}`, which bash also reads as
/// 0. Anything else non-numeric is rejected by `type_check` (SB4030) — it used
/// to become 0 here, so `${s:1x}` silently yielded the whole string. A silent
/// 0 would hide a missed check, so this is loud (DD#116, property 1).
pub fn substring_part_int(part: &str) -> i64 {
    if part.is_empty() {
        return 0;
    }
    part.parse::<i64>().unwrap_or_else(|_| {
        panic!("non-integer substring part `{part}` reached codegen; type_check (SB4030) should have rejected it")
    })
}

/// The five `SsaValue` variants that cannot appear in **scalar value** position,
/// with the pass that was supposed to have removed each one.
///
/// Both backends used to spell these five arms out individually, with a comment
/// in `emit_c` reading "native ICEs on exactly these (codegen/values.rs:81-115);
/// c mirrors it arm for arm" — and that line reference had already drifted off
/// the arms it named. Hand-mirroring the same claim in two places is the fork
/// this repo has been bitten by three times; saying it once is the fix.
///
/// Each claim is **orthogonal** to the gate that guarantees it. `validate` and
/// `transform` decide these questions about the AST; this function is the only
/// site that can say anything about the *lowered* `SsaValue` that came out the
/// other side. A lifting pass that missed a case, or a list builder that failed
/// to intercept a splat, is invisible upstream and lands exactly here.
///
/// # Panics
///
/// Always — that is the entire contract.
pub(crate) fn unlowerable_scalar_value(backend: &str, value: &SsaValue) -> ! {
    // Deliberately *not* total over `SsaValue`: the five named here are the
    // contract, and the final arm exists to tell a caller/callee drift apart
    // from a genuine refusal rather than mislabel it as one.
    let why = match value {
        // DD#81: a `$(f)` on a defined-so-far function is always lifted to a
        // captured temp during transform (a forward/undefined name resolves
        // external instead), so a bare FuncCall never reaches codegen.
        SsaValue::FuncCall { name, .. } => {
            format!("FuncCall ({name}) should have been lifted during transform")
        }
        SsaValue::IntFuncCall { name, .. } => {
            format!("IntFuncCall ({name}) should have been lifted during transform")
        }
        SsaValue::CmdSubstBody { .. } => {
            "CmdSubstBody should have been lifted during transform".to_string()
        }
        // DD#20/#22: a splat is one value per element, so it only has a meaning
        // in a list position, where the argv / array-literal builder expands it.
        SsaValue::ArraySplat { array, .. } => {
            format!("ArraySplat (@{array}) must be expanded by the argv builder")
        }
        // DD#84: `:~` is legal only in an elidable list position, where the list
        // builder intercepts it. Reaching a scalar lowering means validate's
        // SB5046 guard was bypassed.
        SsaValue::ElideEmpty { .. } => {
            "ElideEmpty (:~) must be expanded by a list builder".to_string()
        }
        other => panic!(
            "ICE: {backend} called unlowerable_scalar_value on {:?}, which is \
             not one of the five unlowerable variants — the caller's arm and \
             this match have drifted apart.",
            std::mem::discriminant(other)
        ),
    };
    panic!("ICE: {backend} lowered a value as a scalar, but {why}.")
}

/// How many pipes an `SsaInst::Pipeline` of `n` stages needs.
///
/// Shared by both backends so the one expression cannot drift. With a capture
/// there is an extra pipe (index `n-1`) carrying last-stage → parent.
///
/// # Panics
///
/// The floor differs by form, because the two forms count different things.
/// `lower_pipeline_body` routes only `commands.len() > 1` to the stage builders
/// and sends a lone command to `lower_one`, so:
///
/// * a plain pipeline carries **every** command as a stage — `n >= 2`;
/// * a capture (`cmd | readline x`, `cmd | while readline …`) carries only the
///   **producers**, `&p.commands[..len - 1]`, with the reader left to the
///   parent — `n >= 1`.
///
/// The ICE is not a restatement of that `> 1` gate: it is the only site that
/// can say anything about `stages.len()`, a quantity derived from
/// `commands.len()` through `push_command_stage` and the compound arm of
/// `lower_one_pipeline_stage`. A pusher that dropped a stage would be invisible
/// to the gate and land here.
///
/// It is also what makes the violation *visible*. `n - 1` on a `usize` wraps,
/// so a zero-stage pipeline used to ask the native backend to create
/// `usize::MAX` pipes, while the C backend quietly rewrote a one-stage pipeline
/// as a plain command. Since `emit_c` is the differential oracle, that silent
/// normalisation was the worse of the two — the backend whose job is to
/// disagree out loud was absorbing the difference.
pub(crate) fn pipeline_num_pipes(n: usize, capture_read_fd: bool) -> usize {
    if capture_read_fd {
        assert!(
            n >= 1,
            "ICE: a capturing SsaInst::Pipeline reached codegen with {n} producer \
             stages. lower_pipeline_body only builds one for `commands.len() > 1` \
             and the capture builders take `commands[..len - 1]`, so at least one \
             producer must survive."
        );
        n
    } else {
        assert!(
            n >= 2,
            "ICE: a plain SsaInst::Pipeline reached codegen with {n} stage(s). \
             lower_pipeline_body only builds one for `commands.len() > 1`, and \
             each pusher contributes exactly one stage per command, so a stage \
             was dropped between the two."
        );
        n - 1
    }
}

#[cfg(test)]
mod unlowerable_value_tests {
    use super::{unlowerable_scalar_value, SsaValue};

    fn ice_message(value: &SsaValue) -> String {
        let err = std::panic::catch_unwind(|| unlowerable_scalar_value("native", value))
            .expect_err("must panic");
        err.downcast_ref::<String>()
            .expect("ICE panics carry a formatted String")
            .clone()
    }

    /// Each of the five is unreachable from source — that is what the ICE
    /// asserts — so only a direct call can reach it. The claims differ, so the
    /// test checks that the message names the *right* one: an arm that fell
    /// through to a neighbour would still panic and would still look fine.
    #[test]
    fn every_unlowerable_variant_names_its_own_claim() {
        let cases: Vec<(SsaValue, &str)> = vec![
            (
                SsaValue::FuncCall {
                    name: "f".into(),
                    args: vec![],
                    colls: vec![],
                },
                "FuncCall (f) should have been lifted during transform",
            ),
            (
                SsaValue::IntFuncCall {
                    name: "g".into(),
                    args: vec![],
                    colls: vec![],
                },
                "IntFuncCall (g) should have been lifted during transform",
            ),
            (
                SsaValue::CmdSubstBody { body: vec![] },
                "CmdSubstBody should have been lifted during transform",
            ),
            (
                SsaValue::ArraySplat {
                    array: "a".into(),
                    disposition: None,
                },
                "ArraySplat (@a) must be expanded by the argv builder",
            ),
            (
                SsaValue::ElideEmpty {
                    source: Box::new(SsaValue::Literal("x".into())),
                    taint_off: false,
                },
                "ElideEmpty (:~) must be expanded by a list builder",
            ),
        ];
        for (value, claim) in cases {
            let msg = ice_message(&value);
            assert!(
                msg.starts_with("ICE: native lowered a value as a scalar"),
                "{msg}"
            );
            assert!(msg.contains(claim), "wrong claim for {value:?}: {msg}");
        }
    }

    /// The callers select the five by variant; this function spells them out
    /// again. If the two lists drift, a lowerable value arrives here — and that
    /// must be told apart from a genuine refusal, not reported as one.
    #[test]
    fn a_lowerable_value_reports_the_drift_not_a_refusal() {
        let msg = ice_message(&SsaValue::Literal("x".into()));
        assert!(
            msg.contains("not one of the five unlowerable variants"),
            "{msg}"
        );
        assert!(msg.contains("drifted apart"), "{msg}");
    }
}

#[cfg(test)]
mod pipeline_tests {
    use super::pipeline_num_pipes;

    /// The floor is per-form, and getting that wrong is not hypothetical: an
    /// earlier version of this asserted `n > 1` for both, which rejected every
    /// `cmd | readline x` in the suite. A capture carries producers only.
    #[test]
    fn a_degenerate_pipeline_is_a_loud_ice() {
        for (n, capture) in [(0usize, false), (1, false), (0, true)] {
            let err = std::panic::catch_unwind(|| pipeline_num_pipes(n, capture))
                .expect_err("must panic");
            let msg = err
                .downcast_ref::<String>()
                .expect("ICE panics carry a formatted String");
            assert!(msg.contains("reached codegen with"), "{msg}");
            assert!(msg.contains("lower_pipeline_body"), "{msg}");
        }
    }

    #[test]
    fn a_real_pipeline_counts_its_pipes() {
        // Plain: every command is a stage, and n-1 pipes join them.
        assert_eq!(pipeline_num_pipes(2, false), 1);
        assert_eq!(pipeline_num_pipes(5, false), 4);
        // Capture: producers only, plus one pipe carrying the last to the parent.
        // `echo hi | readline x` is the one-producer case.
        assert_eq!(pipeline_num_pipes(1, true), 1);
        assert_eq!(pipeline_num_pipes(2, true), 2);
    }
}

#[cfg(test)]
mod int_cell_tests {
    /// The guard must be loud. Unit tests reach what the spec suite cannot:
    /// this state is unreachable from source, which is the point of the ICE.
    #[test]
    fn int_cell_in_int_context_is_loud_ice() {
        let err = std::panic::catch_unwind(|| super::int_cell_in_int_context("native"))
            .expect_err("must panic");
        let msg = err
            .downcast_ref::<String>()
            .expect("ICE panics carry a formatted String");
        assert!(msg.starts_with("ICE: native reached int context"), "{msg}");
        assert!(msg.contains("storage annotation"), "{msg}");
    }
}
