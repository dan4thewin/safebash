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
    /// Skip a lastpipe consumer if its producer pipeline could not be constructed.
    SetTestFromPipe,
    /// Run a `let` expression for side-effects, propagate non-zero
    /// exit per DD#55.
    LetExit,
    /// Capture the most recent background pid into a named variable. The
    /// fork itself is [`Terminator::Fork`], which leaves the pid in
    /// `test_result` for this to read.
    StoreBgPid,
    /// Install signal handler block.
    TrapSignal,
    /// Register a FLAG/EXIT handler: signal number and EXIT bit.
    SignalFlag,
    /// Poll a pending EXIT request at a DD#56 boundary.
    SignalCheckpoint,
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
    /// in SSA. Must not reach codegen as a `Command`.
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
    /// `SsaInst::Attest` in SSA. Must not reach codegen as a `Command`.
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

    /// Whether a trailing redirect may attach to this command (SB4046).
    ///
    /// **Not the authority for a command *name*** — ask
    /// [`crate::resolve::head_may_take_redirect`] instead. `from_name`
    /// collapses every name outside this enum to [`Self::Other`], which
    /// covers externals and user functions but *also* the twelve value
    /// intrinsics in `resolve::INTRINSICS`, so this predicate answers `true`
    /// for `getcwd` and `getenv`. Keying the parse gate here is exactly that
    /// mistake; `head_may_take_redirect` joins the two tables.
    ///
    /// The rule SB4046 states is "redirects attach to a command that produces
    /// output", and this is its closed list. `true` means the command either
    /// writes to a descriptor (`echo`, `write`), reads from one (`read` /
    /// `readline`), replaces the process with something that does (`exec`),
    /// or is not a builtin at all ([`Self::Other`] — see the caveat above).
    /// Everything else produces no output, so a redirect on
    /// it is meaningless; bash creates and truncates the file and writes
    /// nothing into it, which DG ruled is a wart to reject rather than copy.
    ///
    /// [`Self::Noop`] is the deliberate exception: `: >f` produces no output
    /// either, but truncate-a-file is the whole point of the idiom and it is
    /// universal, so DG's call (2026-10-11) was to make it work rather than
    /// reject it.
    ///
    /// This started as a bug rather than a rule. Every `false` arm below was
    /// *silently dropping* its redirect — `cd /tmp >f`, `exit 0 >f`,
    /// `close 9 >f`, `argv 1 a >f`, `unset m[k] >f`, `return 0 >f` and
    /// `if match … >f` all compiled clean and never opened the file. The
    /// drop had two independent causes (a `_ => {}` in
    /// `attach_redirects_to_last_inst`, and codegen arms that took no
    /// `redirects` parameter at all), which is why the fix is a gate at parse
    /// rather than a patch at either site: reject here and neither site can
    /// be reached holding a redirect it would discard.
    ///
    /// **Total over the enum on purpose — do not add a wildcard.** A new
    /// builtin must state whether it can carry a redirect; defaulting either
    /// way is how the silent drop happened in the first place.
    pub fn may_take_redirect(self) -> bool {
        match self {
            // Produce output, consume input, or become a process that does.
            Self::Echo | Self::Write | Self::Exec | Self::Read => true,
            // Not a builtin: an external command or a user function.
            Self::Other => true,
            // DG's exception — `: >f` is the truncate idiom.
            Self::Noop => true,

            // No output: rejected with SB4046.
            Self::Test
            | Self::Exit
            | Self::Cd
            | Self::Wait
            | Self::Close
            | Self::Attest
            | Self::Mapfile
            | Self::Argv
            | Self::Break
            | Self::Continue
            | Self::Return
            | Self::Signal
            | Self::Unset => false,

            // Value-only intrinsics: their result is a value, not a stream.
            Self::Len
            | Self::Strlen
            | Self::Keys
            | Self::Join
            | Self::Sub
            | Self::Gsub
            | Self::Match
            | Self::MatchAll
            | Self::Toupper
            | Self::Tolower
            | Self::Basename
            | Self::Dirname => false,

            // Compiler-internal sigils: never written in source, so no
            // trailing redirect can reach them.
            Self::NegateTestResult
            | Self::SetTestFromStatus
            | Self::SetTestFromPipe
            | Self::LetExit
            | Self::StoreBgPid
            | Self::TrapSignal
            | Self::SignalFlag
            | Self::SignalCheckpoint
            | Self::TableDelete
            | Self::TableDeleteIdx => false,
        }
    }

    /// Whether the command's effect lands on the process that runs it, so a
    /// forked child — a pipeline stage (SB4022) or a `&` job (SB4024) — would
    /// discard it.
    ///
    /// Before these gates every `true` arm compiled as a stage and died at
    /// runtime with `sash: bug: bare command passed to sash_exec`, or ICEd
    /// (`keys m k | cat`), and under `&` lost its write in silence
    /// (`argv a &` left `a` empty) or segfaulted on the never-filled
    /// collection.
    ///
    /// The answer is for the command's *legal statement spellings*. `keys m k`
    /// and `argv a` write a collection, so [`Self::Keys`] and [`Self::Argv`]
    /// answer `true`, and an illegal spelling of either (`keys m`, `argv 1`)
    /// draws the stage error before SB5073 / SB5063 can. A builtin with no
    /// legal statement spelling answers `false`, so its own rule keeps
    /// precedence: the value-only intrinsics are SB5073, and
    /// [`Self::Mapfile`] is SB5060 in every position.
    ///
    /// [`Self::Other`] is `false` for all three populations `from_name`
    /// collapses into it, so unlike [`Self::may_take_redirect`] this needs no
    /// join with `resolve::INTRINSICS`: an external runs in a process of its
    /// own anyway, a user-function stage is DD#122's sanctioned form, and the
    /// twelve value intrinsics are SB5073.
    ///
    /// [`Self::Exec`] is `true`: it replaces the process that runs it, so as a
    /// stage it replaces only the child. That `exec cmd &` stays legal is the
    /// background gate's exemption, not this predicate's answer. A
    /// `read`/`readline` *last* stage is DD#122's pipe reader and runs in the
    /// parent; the stage gate exempts it, also not here. [`Self::Test`] is
    /// `false`: a test's only effect is its status, which a stage still
    /// reports (a `[[ … ]]` stage is `statement_form_label`'s anyway).
    ///
    /// **Total over the enum on purpose — do not add a wildcard.** A new
    /// builtin must say whether a fork loses what it does.
    pub fn acts_on_its_process(self) -> bool {
        match self {
            // Control flow of the process that runs them.
            Self::Exit | Self::Return | Self::Break | Self::Continue => true,
            // Process state: cwd, descriptors, jobs, variables, dispositions.
            Self::Cd | Self::Close | Self::Wait | Self::Unset | Self::Signal => true,
            // Write a variable or collection of the process that runs them.
            Self::Argv | Self::Read | Self::Keys | Self::Match | Self::MatchAll | Self::Attest => {
                true
            }
            // Replaces the process that runs it.
            Self::Exec => true,

            // Their effect is output, which a stage pipes on.
            Self::Echo | Self::Write | Self::Noop => false,
            // Not a builtin, or a value intrinsic `from_name` does not know.
            Self::Other => false,
            // Its effect is its status, which a stage still reports.
            Self::Test => false,
            // Retired: SB5060 in every position.
            Self::Mapfile => false,
            // Value-only: SB5073 in every position but a sole capture.
            Self::Len
            | Self::Strlen
            | Self::Join
            | Self::Sub
            | Self::Gsub
            | Self::Toupper
            | Self::Tolower
            | Self::Basename
            | Self::Dirname => false,

            // Compiler-internal opcodes, never written in source: each sets
            // the test status, the exit path, a job, a signal, or a table of
            // the process that runs it.
            Self::NegateTestResult
            | Self::SetTestFromStatus
            | Self::SetTestFromPipe
            | Self::LetExit
            | Self::StoreBgPid
            | Self::TrapSignal
            | Self::SignalFlag
            | Self::SignalCheckpoint
            | Self::TableDelete
            | Self::TableDeleteIdx => true,
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
            Self::SetTestFromPipe => "__set_test_from_pipe",
            Self::LetExit => "__let_exit",
            Self::StoreBgPid => "__store_bg_pid",
            Self::TrapSignal => "__trap_signal",
            Self::SignalFlag => "__signal_flag",
            Self::SignalCheckpoint => "__signal_checkpoint",
            Self::TableDelete => "__table_delete",
            Self::TableDeleteIdx => "__table_delete_idx",
            _ => return None,
        })
    }

    /// Decode the transform-to-backend protocol, not source argument syntax.
    /// A missing or extra operand must not turn an instruction into a no-op.
    pub(crate) fn expect_ssa_args<const N: usize>(self, args: &[SsaValue]) -> &[SsaValue; N] {
        args.try_into().unwrap_or_else(|_| {
            panic!(
                "ICE: {self:?} needs exactly {N} SSA arguments, got {}",
                args.len()
            )
        })
    }

    pub(crate) fn expect_literal_int(self, arg: &SsaValue) -> i64 {
        let SsaValue::Literal(text) = arg else {
            panic!("ICE: {self:?} needs an integer literal, got {arg:?}")
        };
        text.parse()
            .unwrap_or_else(|_| panic!("ICE: {self:?} has a non-integer literal `{text}`"))
    }

    pub(crate) fn expect_var_arg(self, arg: &SsaValue) -> &VarRef {
        match arg {
            SsaValue::Var(var) => var,
            _ => panic!("ICE: {self:?} needs a variable operand, got {arg:?}"),
        }
    }

    pub(crate) fn expect_signal_flag(args: &[SsaValue]) -> (i64, i64) {
        let [signal, exiting] = Self::SignalFlag.expect_ssa_args(args);
        let signal = Self::SignalFlag.expect_literal_int(signal);
        let exiting = Self::SignalFlag.expect_literal_int(exiting);
        assert!(
            matches!(exiting, 0 | 1),
            "ICE: SignalFlag EXIT bit must be 0 or 1"
        );
        (signal, exiting)
    }

    pub(crate) fn expect_close_fd(args: &[SsaValue]) -> (i64, &str) {
        let [arg] = Self::Close.expect_ssa_args(args);
        let SsaValue::Literal(text) = arg else {
            panic!("ICE: Close needs a literal file descriptor")
        };
        let fd = Self::Close.expect_literal_int(arg);
        assert!(
            (0..=i32::MAX as i64).contains(&fd),
            "ICE: Close file descriptor is outside 0..2147483647"
        );
        (fd, text)
    }
}

/// A command kind whose work belongs to an earlier pass. Shared by backends:
/// silently succeeding here would hide a failed rewrite or instruction lowering.
pub(crate) fn ice_unreachable_command(kind: &BuiltinCmd, name: &str) -> ! {
    match kind {
        BuiltinCmd::Write => {
            panic!("ICE: {kind:?} is rewritten before codegen and must not reach backend command emission (as `{name}`)")
        }
        BuiltinCmd::Len
        | BuiltinCmd::Strlen
        | BuiltinCmd::Keys
        | BuiltinCmd::Join
        | BuiltinCmd::Sub
        | BuiltinCmd::Gsub
        | BuiltinCmd::MatchAll
        | BuiltinCmd::Toupper
        | BuiltinCmd::Tolower
        | BuiltinCmd::Basename
        | BuiltinCmd::Dirname
        | BuiltinCmd::Argv => {
            panic!("ICE: {kind:?} is an expression intrinsic resolved in transform and must not reach backend command emission (as `{name}`)")
        }
        _ => {
            panic!("ICE: {kind:?} is lowered to its own instruction and must not reach backend command emission (as `{name}`)")
        }
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
        capture: PipelineCapture,
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
    /// Close a parent reader and reap its producers after consumption.
    PipelineFinish {
        site: u32,
        checked: bool,
        /// Nonlocal exits must retain their return/status/condition value.
        preserve_status: bool,
        /// Whether the consumer's own verdict joins the producers' status. A
        /// reader's EOF test joins; a completed `while` succeeds, and a
        /// `-`-suppressed consumer's verdict is discarded (DD#55), so neither
        /// of those does.
        verdict_joins: bool,
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
    /// Retag an attest's subject — scalar, array or map — clean once its
    /// predicate has passed: after the statement form's `AttestAll`, and in
    /// the pass block of every caught `if attest`, whose predicate the
    /// condition block has already evaluated exactly once. A compile-time
    /// relabelling: the target is a clean SSA version of the same runtime
    /// value, and the retag validates nothing itself.
    AttestRetag { target: VarRef, source: VarRef },
}

#[derive(Debug, Clone, PartialEq)]
pub enum PipelineCapture {
    None,
    /// Stream to this parent-reader site; its PipelineFinish closes and waits.
    ReadFd(u32),
    /// Drain the final pipe before waiting and define a tainted string value.
    Value(VarRef),
}

impl PipelineCapture {
    pub(crate) fn pipe_to_parent(&self) -> bool {
        !matches!(self, Self::None)
    }

    pub(crate) fn target(&self) -> Option<&VarRef> {
        match self {
            Self::Value(target) => Some(target),
            Self::None | Self::ReadFd(_) => None,
        }
    }
}

/// Negative read fds identify compiler-owned pipes, never process-wide stdin.
pub(crate) fn reader_pipe_fd(site: u32) -> i64 {
    -1 - i64::from(site)
}

pub(crate) fn reader_pipe_site(fd: i64) -> u32 {
    u32::try_from(-1 - fd).expect("ICE: an owned pipe fd must encode a reader site")
}

#[derive(Debug, Clone)]
pub(crate) struct ReaderPipeline {
    pub stages: Vec<String>,
    pub suppressed: Vec<bool>,
    pub negated: Vec<bool>,
}

#[derive(Debug, Clone)]
pub(crate) struct ReaderRegion {
    pub site: u32,
    pub entry: BlockId,
    pub done: BlockId,
    pub candidates: std::collections::HashSet<BlockId>,
    pub checked: bool,
    pub verdict_joins: bool,
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
            | SsaInst::PipelineFinish { .. }
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
    /// `- cmd`). Captures in its arguments are not covered (DD#55 2026-10-30).
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
    /// Synthesized collection/argv loop increment, stored only in integer slots.
    /// Source arithmetic uses `ArithBinOp` instead.
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
    /// DD#38: `cmd &` forks. The parent continues at `parent` with the
    /// child's pid in `test_result`; the child runs `child` in its own
    /// process. The child is not a control-flow successor: no edge leads
    /// from its blocks back into the parent's, so per-process analyses
    /// (reader scopes, phis) never see it. See [`SsaProgram::child_region`].
    Fork { parent: BlockId, child: BlockId },
    /// A forked child (a background job or a forked pipeline stage) is
    /// done: exit with `$?`. Never routed through END — DD#56: END runs
    /// only in the main process.
    ChildExit,
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
/// - the scan stops at **every** instruction that sets the status
///   ([`condition_operand_index`]). It stopped only at those that carry
///   redirects, so `cmd > /bad/f; if [[ … ]]` passed over the `[[ … ]]` and
///   caught `cmd`'s failed open instead of aborting.
///
/// That is also the instruction the transform unchecks, so a status the
/// condition catches and a redirect failure it catches belong to one
/// instruction. Only redirect guards read `in_condition`; selecting one that
/// has none (a `[[ … ]]`, a `$?` assignment) flags nothing, which is right —
/// the command before it is not in condition position.
pub(crate) fn condition_inst_index(block: &BasicBlock) -> Option<usize> {
    if !matches!(
        block.terminator,
        Terminator::Branch { .. } | Terminator::Loop { .. }
    ) {
        return None;
    }
    condition_operand_index(&block.insts)
}

/// DD#55: index of the instruction whose status a condition reads — the
/// **last one in `insts` that sets the status** — or `None` if none does.
///
/// Shared by both backends (through [`condition_inst_index`]) and by the
/// transform, which clears that instruction's abort. Each used to scan for
/// itself and pass over every instruction it did not list, so a `[[ … ]]`, a
/// read or a `match` in condition position was skipped and the scan settled
/// on whatever command preceded it: `cmd; if [[ … ]]` ignored `cmd`'s failure,
/// and a failed capture in the test's operand never aborted.
pub(crate) fn condition_operand_index(insts: &[SsaInst]) -> Option<usize> {
    insts.iter().rposition(sets_condition_status)
}

/// Does `inst` set the status a following condition reads? `true` for exactly
/// the instructions both backends lower to a write of the test result; `false`
/// for those a condition reads straight through.
///
/// **Total over `SsaInst` on purpose — do not add a wildcard.** The two scans
/// this replaced each ended in one, which is how both came to skip the
/// `[[ … ]]` they were looking for.
fn sets_condition_status(inst: &SsaInst) -> bool {
    match inst {
        // DD#70: the `!` sigil follows the operand it negates and only
        // rewrites that operand's status; the operand is in condition position.
        SsaInst::Command { name, .. } if name == "__negate_test_result" => false,
        SsaInst::Command { .. }
        | SsaInst::Pipeline { .. }
        | SsaInst::PipelineFinish { .. }
        | SsaInst::Test(..)
        | SsaInst::ReadLine { .. }
        | SsaInst::ReadArray { .. }
        | SsaInst::MatchInto { .. }
        | SsaInst::AttestAll { .. }
        // A redirect scope (`f > out.txt`) ends its block with the Branch
        // that skips the body when the open fails, so the scope — not some
        // earlier command that happens to share the block — is the
        // instruction in condition position.
        | SsaInst::RedirectPush { .. } => true,
        // DD#74: `return N` is `Assign(?, N)`, and both backends keep the test
        // result in step with `$?`.
        SsaInst::Assign { target, .. } => target.name == "?",
        SsaInst::SubscriptAssign { .. }
        | SsaInst::ArrayAppend { .. }
        | SsaInst::MapFromArray { .. }
        | SsaInst::SplitValue { .. }
        | SsaInst::ArgvFill { .. }
        | SsaInst::KeysFill { .. }
        | SsaInst::CaptureStart { .. }
        // A capture condition (`if x=$(cmd)`, DD#88) reads the body's status
        // through the capture's close and the assignment of its target.
        | SsaInst::CaptureEnd { .. }
        | SsaInst::RedirectPop { .. }
        | SsaInst::Attest { .. }
        | SsaInst::AttestRetag { .. } => false,
    }
}

/// The complete SSA representation of a program.
#[derive(Debug)]
pub struct SsaProgram {
    pub(crate) source_names: crate::bindings::SourceNames,
    pub blocks: BTreeMap<BlockId, BasicBlock>,
    pub entry: BlockId,
    /// DD#56: dedicated END block — all exit paths goto here
    pub end_block: Option<BlockId>,
    /// DD#56: signals with EXIT disposition (check points inserted)
    pub exiting_signals: std::collections::HashSet<i64>,
    /// DD#14: forked pipeline stages — each stage's entry block (run by a
    /// forked child) and what it runs: the function it calls, or the command
    /// whose arguments the child lifts.
    pub(crate) compound_stages: CompoundStages,
    /// `--manifest`: absolute paths of every external program this program
    /// execs — direct commands, pipeline stages, and `$( … )` substitutions.
    /// Collected at the single resolution funnel (`resolve_pipeline_command`)
    /// so it cannot miss an external nor leak a builtin/internal sigil.
    pub external_deps: std::collections::BTreeSet<String>,
    /// Runtime-owned descriptors must not occupy a number named by source.
    pub(crate) user_fds: Vec<i64>,
    checkpoint_blocks: std::collections::HashSet<BlockId>,
    next_block_id: usize,
}

impl Terminator {
    /// The blocks control can transfer to from this terminator, within the
    /// same process. A fork's child is deliberately absent: it runs in
    /// another process and never returns here (see [`Self::forked_child`]).
    pub(crate) fn successors(&self) -> Vec<BlockId> {
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
            Terminator::Fork { parent, .. } => vec![*parent],
            Terminator::Return | Terminator::ChildExit => Vec::new(),
        }
    }

    /// The entry block of the child process this terminator forks, if any.
    pub(crate) fn forked_child(&self) -> Option<BlockId> {
        match self {
            Terminator::Fork { child, .. } => Some(*child),
            Terminator::Goto(_)
            | Terminator::Branch { .. }
            | Terminator::Loop { .. }
            | Terminator::Return
            | Terminator::ChildExit => None,
        }
    }
}

impl SsaProgram {
    pub(crate) fn reader_pipelines(&self) -> BTreeMap<u32, ReaderPipeline> {
        self.blocks
            .values()
            .flat_map(|block| &block.insts)
            .filter_map(|inst| match inst {
                SsaInst::Pipeline {
                    stages,
                    capture: PipelineCapture::ReadFd(site),
                    suppressed,
                    negated,
                    ..
                } => Some((
                    *site,
                    ReaderPipeline {
                        stages: self.compound_stages.labels(stages),
                        suppressed: suppressed.clone(),
                        negated: negated.clone(),
                    },
                )),
                _ => None,
            })
            .collect()
    }

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
            source_names: crate::bindings::SourceNames::default(),
            blocks: BTreeMap::new(),
            entry: 0,
            end_block: None,
            exiting_signals: std::collections::HashSet::new(),
            compound_stages: CompoundStages::default(),
            external_deps: std::collections::BTreeSet::new(),
            user_fds: Vec::new(),
            checkpoint_blocks: std::collections::HashSet::new(),
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

    pub(crate) fn new_checkpoint_block(&mut self, label: &str) -> BlockId {
        let id = self.new_block(label);
        self.checkpoint_blocks.insert(id);
        id
    }

    pub(crate) fn install_signal_checkpoints(&mut self) {
        let blocks = std::mem::take(&mut self.checkpoint_blocks);
        if self.exiting_signals.is_empty() {
            return;
        }
        // Two-pass loop lowering clears its discovery instructions.
        for id in blocks {
            self.block_mut(id).insts.insert(
                0,
                SsaInst::internal(BuiltinCmd::SignalCheckpoint, Vec::new(), true),
            );
        }
    }

    pub fn block_mut(&mut self, id: BlockId) -> &mut BasicBlock {
        self.blocks.get_mut(&id).expect("block not found")
    }

    pub fn block(&self, id: BlockId) -> &BasicBlock {
        self.blocks.get(&id).expect("block not found")
    }

    /// Every block that can run, in this process or a forked child: block 0
    /// and everything reachable from it (see [`Self::reachable_from`]).
    pub fn reachable(&self) -> std::collections::HashSet<BlockId> {
        self.reachable_from(&[0])
    }

    /// The blocks that run in a forked child — a background job or a
    /// forked pipeline stage, and anything those fork in turn. A block
    /// here must not exit through END (DD#56: END runs only in the main
    /// process), so an abort or `exit` from it exits the child directly.
    pub fn child_region(&self) -> std::collections::HashSet<BlockId> {
        let roots: Vec<BlockId> = self
            .blocks
            .values()
            .flat_map(|block| self.spawned_by(block))
            .collect();
        self.reachable_from(&roots)
    }

    /// ICE unless no block is shared between the main process and a forked
    /// child, and every child ends in [`Terminator::ChildExit`]. A child that
    /// jumps into the parent's blocks — a `return` target, a loop exit, END —
    /// would run the parent's code a second time in the wrong process, and a
    /// child that reaches `Return` would exit as though it were the program;
    /// nothing downstream could tell either from correct code.
    pub(crate) fn assert_children_disjoint(&self) {
        let mut roots = vec![0];
        roots.extend(self.end_block);
        let main = self.same_process_from(&roots);
        let children = self.child_region();
        let mut shared: Vec<BlockId> = children.intersection(&main).copied().collect();
        shared.sort_unstable();
        assert!(
            shared.is_empty(),
            "ICE: blocks {shared:?} run in both the main process and a forked child"
        );
        let mut returning: Vec<BlockId> = children
            .into_iter()
            .filter(|&id| self.block(id).terminator == Terminator::Return)
            .collect();
        returning.sort_unstable();
        assert!(
            returning.is_empty(),
            "ICE: forked-child blocks {returning:?} end in Return — a child ends in ChildExit"
        );
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

    /// Blocks reachable from `roots` by every route a block has to run
    /// another: its terminator's successors, the child a
    /// [`Terminator::Fork`] starts, and the forked stages its pipelines
    /// fork. An END handler that backgrounds a job therefore owns the job's
    /// blocks too, and follows END's rules in them.
    pub fn reachable_from(&self, roots: &[BlockId]) -> std::collections::HashSet<BlockId> {
        self.closure(roots, true)
    }

    /// Blocks that run in the same process as `roots`: terminator
    /// successors only, never into a forked child.
    fn same_process_from(&self, roots: &[BlockId]) -> std::collections::HashSet<BlockId> {
        self.closure(roots, false)
    }

    fn closure(&self, roots: &[BlockId], across_forks: bool) -> std::collections::HashSet<BlockId> {
        let mut visited = std::collections::HashSet::new();
        let mut stack: Vec<BlockId> = roots.to_vec();
        while let Some(id) = stack.pop() {
            if visited.insert(id) {
                stack.extend(self.next_blocks(id, across_forks));
            }
        }
        visited
    }

    /// The blocks [`Self::closure`] visits after `id`: its terminator's
    /// successors and, across forks, the children it spawns. Empty for an
    /// unknown id.
    fn next_blocks(&self, id: BlockId, across_forks: bool) -> Vec<BlockId> {
        let Some(block) = self.blocks.get(&id) else {
            return Vec::new();
        };
        let mut next = block.terminator.successors();
        if across_forks {
            next.extend(self.spawned_by(block));
        }
        next
    }

    /// The entry blocks of the child processes `block` forks: a background
    /// job's, and each forked stage of its pipelines.
    pub(crate) fn spawned_by(&self, block: &BasicBlock) -> Vec<BlockId> {
        let stages = block.insts.iter().flat_map(|inst| match inst {
            SsaInst::Pipeline { stages, .. } => stages.as_slice(),
            _ => &[],
        });
        block
            .terminator
            .forked_child()
            .into_iter()
            .chain(stages.filter_map(|stage| self.compound_stages.entry(&stage.0)))
            .collect()
    }
}

const COMPOUND_STAGE_PREFIX: &str = "__compound_";

/// DD#14: the forked pipeline stages. A stage that runs a function, or a
/// command whose arguments need lifting (`command_pipeline_stage`), is
/// named `__compound_<entry>` in [`SsaInst::Pipeline`], where `<entry>` is the
/// block its forked child starts at; this records each such entry and what
/// the child runs, so the backends can find the child's code and a failure
/// names the function, or the command's resolved path as a direct stage's
/// failure does.
#[derive(Debug, Clone, Default)]
pub(crate) struct CompoundStages(BTreeMap<BlockId, String>);

impl CompoundStages {
    /// Record a forked stage that runs `label` — a function, or a command's
    /// resolved path — from `entry`; returns its stage name.
    pub(crate) fn record(&mut self, entry: BlockId, label: &str) -> String {
        self.0.insert(entry, label.to_string());
        format!("{COMPOUND_STAGE_PREFIX}{entry}")
    }

    /// The entry block of the forked stage `name` denotes; `None` for a
    /// direct stage. No direct stage carries the prefix — an external is
    /// resolved to an absolute path (DD#23) and no builtin is spelled so —
    /// so a prefixed name that was never recorded is an ICE, not a command.
    pub(crate) fn entry(&self, name: &str) -> Option<BlockId> {
        let id = name.strip_prefix(COMPOUND_STAGE_PREFIX)?;
        let entry = id.parse().ok().filter(|entry| self.0.contains_key(entry));
        Some(entry.unwrap_or_else(|| {
            panic!("ICE: pipeline stage `{name}` names no recorded forked stage")
        }))
    }

    /// What a diagnostic calls stage `name`: what a forked stage runs (the
    /// function, or the command's resolved path), otherwise the command itself.
    pub(crate) fn label<'a>(&'a self, name: &'a str) -> &'a str {
        match self.entry(name) {
            Some(entry) => &self.0[&entry],
            None => name,
        }
    }

    /// What a diagnostic calls each stage of a pipeline.
    pub(crate) fn labels(
        &self,
        stages: &[(String, Vec<SsaValue>, Vec<crate::ir::Redirect>)],
    ) -> Vec<String> {
        stages
            .iter()
            .map(|stage| self.label(&stage.0).to_string())
            .collect()
    }

    /// What each forked stage runs, for literal interning.
    pub(crate) fn forked_labels(&self) -> impl Iterator<Item = &str> {
        self.0.values().map(String::as_str)
    }
}

/// DD#27/§5: the self tail-call target of the function body being lowered.
/// A `return F …` naming `func` rebinds `args_name` and branches to `header`
/// instead of inlining `func` again — turning tail recursion into a loop.
#[derive(Debug, Clone)]
pub(crate) struct TailLoop {
    pub(crate) func: String,
    pub(crate) header: usize,
    pub(crate) args_name: String,
}

/// DD#55: what a call of a user function hands back to its caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FnClass {
    /// Nothing: every exit yields 0.
    Void,
    /// A status the caller must handle or discard.
    Status,
    /// An `-> int` function's int.
    Int,
}

/// DD#55: what one function definition's own exits decide about its class.
#[derive(Debug, Clone)]
pub(crate) enum ExitSummary {
    /// An `-> int` definition.
    Int,
    /// Some exit hands back a status of its own.
    Status,
    /// Every other exit yields 0, so the class is that of the functions these
    /// `return g` exits hand the result to: void when there are none.
    Returns(Vec<String>),
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
    /// Source names/literal operands and compiler temporaries already allocated.
    pub reserved_names: std::collections::HashSet<String>,
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
    /// DD#55: the exits of each function definition in force — the last one
    /// lowered for its name — updated at each `fn` statement.
    pub(crate) exits_in_force: HashMap<String, ExitSummary>,
    /// DD#55: the class of each function in force, recomputed from
    /// `exits_in_force` at each `fn` statement: what a call lowered here runs.
    pub(crate) class_in_force: HashMap<String, FnClass>,
    /// DD#55: the class of each function whatever the call site
    /// (`functions::static_classes`), for a `fn_NAME` template, which has no
    /// call site. A redefined name is absent, as is any function whose result
    /// it decides.
    pub(crate) class_static: HashMap<String, FnClass>,
    /// Functions declared with `-> int` return type.
    pub function_int_typed: std::collections::HashSet<String>,
    /// Depth counter for condition position (if/while condition, LHS of &&/||).
    pub in_condition: u32,
    /// Depth counter for `-` suppression (DD#55). Raised while lowering the
    /// inner statement of `StmtKind::Suppressed`, a `-`-marked final pipe
    /// consumer, a `$(- cmd)` body and a capture condition. Each check reads
    /// it where it is constructed and is emitted only at zero. Operand lifts
    /// and inlined function bodies clear it, so `-` discards one operation's
    /// own status (DD#55 2026-10-30). A must-consume result (SB4016/SB4018) inside
    /// it is discarded rather than an error.
    pub suppressed: u32,
    /// EXIT trap body statements (if set via `trap 'body' EXIT`).
    pub exit_trap: Option<Vec<crate::ir::Statement>>,
    /// Signals registered with EXIT disposition (compiler inserts check points).
    pub exiting_signals: std::collections::HashSet<i64>,
    /// Diagnostics collected during SSA transformation.
    pub diagnostics: Vec<String>,
    /// Diagnostics raised on the immutable expression-evaluation path (a
    /// `$( … )` capture of a function, DD#55), keyed by the capture's span so
    /// a later verdict on the same capture can supersede them. Interior-mutable
    /// for the same reason as `external_deps`; moved into `diagnostics` after
    /// each statement and at the end of `ir_to_ssa_typed`.
    pub expr_diagnostics: std::cell::RefCell<Vec<(crate::ir::Span, String)>>,
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
    /// already on the stack is a recursive edge — rejected (SB5037) unless it is
    /// a direct self tail-call (a `return F …`), which lowers to a loop.
    pub inlining: Vec<String>,
    /// DD#27/§5: the self tail-call target of the function body being
    /// lowered (`enter_tail_loop`). `None` outside a function body.
    pub(crate) tail_loop: Option<TailLoop>,
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
    pub(crate) next_reader_site: u32,
    pub(crate) reader_regions: Vec<ReaderRegion>,
    /// Lexical pipe binding, suspended across function/capture/redirect scopes.
    pub(crate) pipe_reader: Option<(i64, bool)>,
    /// The statement being lowered, which locates the transform's own errors
    /// (`error_at_stmt`). Call arity, recursion and `exec` resolution are
    /// raised far below the statement dispatcher, where no span is threaded,
    /// and every one of them used to report `1:1`.
    /// `lower_one` sets and restores it, so an inlined body or a `$( )` body
    /// reports its own statement and the enclosing one is back on return.
    pub(crate) stmt_span: crate::ir::Span,
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
        //
        // `getcpid` was the one name this list missed, and the omission was
        // invisible: `parse::expr::parse_getcpid_word` rebuilt the command
        // with `args: vec![]`, so no argument survived for anyone to count.
        // Routing that spelling through the ordinary parse is what exposed
        // it — `$(getcpid x)` then reached the int-context lowering and hit
        // its `unreachable!()`.
        "status" | "getpid" | "getppid" | "getcwd" | "epochseconds" | "argc" | "signum"
        | "random" | "waitstatus" | "getcpid" => Some(0..=0),
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
    pub(crate) fn reserve_temp_name(
        &mut self,
        mut name: String,
        var_type: crate::ir::VarType,
    ) -> String {
        while self.reserved_names.contains(&name) {
            name.push('_');
        }
        self.reserved_names.insert(name.clone());
        self.type_map.insert(name.clone(), var_type);
        name
    }

    pub fn with_types(type_map: HashMap<String, crate::ir::VarType>) -> Self {
        let reserved_names = type_map.keys().cloned().collect();
        Self {
            current: HashMap::new(),
            counter: HashMap::new(),
            known_functions: std::collections::HashSet::new(),
            function_bodies: HashMap::new(),
            exits_in_force: HashMap::new(),
            class_in_force: HashMap::new(),
            class_static: HashMap::new(),
            function_int_typed: std::collections::HashSet::new(),
            in_condition: 0,
            suppressed: 0,
            type_map,
            reserved_names,
            assoc_arrays: std::collections::HashSet::new(),
            collections: std::collections::HashSet::new(),
            readonly_vars: std::collections::HashSet::new(),
            loop_stack: Vec::new(),
            return_target: None,
            exit_trap: None,
            exiting_signals: std::collections::HashSet::new(),
            diagnostics: Vec::new(),
            expr_diagnostics: std::cell::RefCell::new(Vec::new()),
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
            next_reader_site: 0,
            reader_regions: Vec::new(),
            pipe_reader: None,
            stmt_span: crate::ir::Span::new(1, 1),
        }
    }

    /// Record an error at the statement being lowered. `msg` begins with the
    /// bracketed code (`[SB5042] function …`); the location comes from
    /// `stmt_span`, so no site formats one of its own.
    pub(crate) fn error_at_stmt(&mut self, msg: impl std::fmt::Display) {
        let crate::ir::Span { line, col } = self.stmt_span;
        self.diagnostics.push(format!("{line}:{col}: error {msg}"));
    }

    /// Move the diagnostics raised while lowering an expression, which sees
    /// the state only through `&self`, into `diagnostics`.
    pub(crate) fn flush_expr_diagnostics(&mut self) {
        let raised = self.expr_diagnostics.get_mut().drain(..);
        self.diagnostics.extend(raised.map(|(_, d)| d));
    }

    /// DD#27/§5: make the body of `func`, entered at `header`, the target of
    /// its own self tail-calls — never its caller's. Call after the callee's
    /// `args` are bound and before the body is lowered; returns the enclosing
    /// target, which the caller restores once the body is lowered.
    pub(crate) fn enter_tail_loop(&mut self, func: &str, header: usize) -> Option<TailLoop> {
        self.tail_loop.replace(TailLoop {
            func: func.to_string(),
            header,
            args_name: self.current_args_name.clone(),
        })
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
    ///
    /// A `const` is defined once per entry to its scope. validate's SB4044
    /// rejects every statement that writes one, and SB5072 keeps one out of
    /// the bodies lowered more than once, so a second definition means
    /// lowering bound the name twice by itself — and the const would silently
    /// gain a second value. Merge phis use [`Self::define_merge`].
    pub fn define(&mut self, name: &str) -> VarRef {
        if self.readonly_vars.contains(name) && self.current.contains_key(name) {
            panic!(
                "readonly `{name}` defined twice in lowering; validate's SB4044 rejects \
                 every write to a const, so a second version would change one"
            );
        }
        self.next_version(name)
    }

    /// Define the version a phi joins at a merge. Each version it joins was
    /// bound by a statement that was itself checked, so a phi is not a
    /// reassignment: a `const` bound on one path of an `if` gets its phi at
    /// the merge like any other name.
    pub fn define_merge(&mut self, name: &str) -> VarRef {
        self.next_version(name)
    }

    fn next_version(&mut self, name: &str) -> VarRef {
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

    /// DD#55: the class of the function a call of `name` lowered here runs —
    /// the definition in force, or in a `fn_NAME` template, which has no call
    /// site, the name's only definition. `None` for a name with neither.
    pub(crate) fn call_class(&self, name: &str) -> Option<FnClass> {
        let classes = if self.lowering_fn_template {
            &self.class_static
        } else {
            &self.class_in_force
        };
        classes.get(name).copied()
    }

    /// Enter the lowering of a forked child (`cmd &`, a forked pipeline
    /// stage). Pair with [`Self::exit_forked_child`].
    pub(crate) fn enter_forked_child(&mut self) -> ParentContext {
        ParentContext {
            versions: self.snapshot(),
            pipe_reader: self.pipe_reader.take(),
            return_target: self.return_target.take(),
            loop_stack: std::mem::take(&mut self.loop_stack),
            tail_loop: self.tail_loop.take(),
        }
    }

    /// Leave a forked child: the parent resumes with exactly the context it
    /// had at the fork and none of the child's variable versions.
    pub(crate) fn exit_forked_child(&mut self, parent: ParentContext) {
        self.restore(&parent.versions);
        self.pipe_reader = parent.pipe_reader;
        self.return_target = parent.return_target;
        self.loop_stack = parent.loop_stack;
        self.tail_loop = parent.tail_loop;
    }
}

/// What a forked child must not inherit from its parent's lowering: the
/// parent's variable versions (the child's assignments die with the child),
/// its lexical pipe reader (`sash_fork` discards inherited readers), and
/// every jump target into parent code — no child block may branch to a
/// parent block, which is what [`SsaProgram::assert_children_disjoint`]
/// checks after the fact.
pub(crate) struct ParentContext {
    versions: HashMap<String, u32>,
    pipe_reader: Option<(i64, bool)>,
    return_target: Option<usize>,
    loop_stack: Vec<(usize, usize)>,
    tail_loop: Option<TailLoop>,
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

/// The `SsaValue` variants that cannot appear in **string/scalar lowering**,
/// and the invariant that rules each one out.
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
    // Deliberately *not* total over `SsaValue`: the variants named here are the
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
        // Only lower_for_array / lower_for_argv_slice build this specialized
        // form, twice each; every target is a synthesized integer counter.
        SsaValue::ArithExpr { .. } => {
            "ArithExpr loop increments must stay in integer slots".to_string()
        }
        other => panic!(
            "ICE: {backend} called unlowerable_scalar_value on {:?}, which is \
             not one of the unlowerable variants — the caller's arm and \
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
///   parent — `n >= 1`;
/// * an external value capture contributes one producer and a private parent
///   reader — `n == 1`.
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
             stages. Lastpipe takes `commands[..len - 1]` from a multi-command \
             pipeline; an external value capture contributes one command. \
             At least one producer must survive."
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
mod api_contract_tests {
    use super::*;

    /// The compiler-internal opcodes: no source spelling reaches them, so a
    /// predicate's arm for them is pinned here or nowhere.
    const INTERNAL_OPCODES: [BuiltinCmd; 10] = [
        BuiltinCmd::NegateTestResult,
        BuiltinCmd::SetTestFromStatus,
        BuiltinCmd::SetTestFromPipe,
        BuiltinCmd::LetExit,
        BuiltinCmd::StoreBgPid,
        BuiltinCmd::TrapSignal,
        BuiltinCmd::SignalFlag,
        BuiltinCmd::SignalCheckpoint,
        BuiltinCmd::TableDelete,
        BuiltinCmd::TableDeleteIdx,
    ];

    #[test]
    fn only_authorial_commands_may_take_redirects() {
        for kind in INTERNAL_OPCODES {
            assert!(!kind.may_take_redirect(), "{kind:?}");
        }
        assert!(BuiltinCmd::Other.may_take_redirect());
    }

    /// Each internal opcode sets state of the process that runs it — the test
    /// status, the exit path, a job, a signal, or a table — so a fork would
    /// lose it.
    #[test]
    fn a_fork_loses_what_an_internal_opcode_does() {
        for kind in INTERNAL_OPCODES {
            assert!(kind.acts_on_its_process(), "{kind:?}");
        }
    }

    #[test]
    #[should_panic(expected = "SsaInst::internal requires a compiler-internal variant")]
    fn authorial_builtin_cannot_use_the_internal_constructor() {
        SsaInst::internal(BuiltinCmd::Echo, vec![], false);
    }

    #[test]
    #[should_panic(
        expected = "non-integer substring part `1x` reached codegen; type_check (SB4030) should have rejected it"
    )]
    fn malformed_rendered_substring_part_is_an_invariant_failure() {
        substring_part_int("1x");
    }

    /// No source program can reach this: SB4044 rejects every write to a
    /// const. The panic is the claim that lowering never binds one twice.
    #[test]
    #[should_panic(expected = "readonly `k` defined twice in lowering; validate's SB4044")]
    fn defining_a_const_twice_is_an_invariant_failure() {
        let mut state = VersionState::with_types(HashMap::new());
        state.define("k");
        state.readonly_vars.insert("k".to_string());
        state.define("k");
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

    /// Each is unreachable in this context from source — that is what the ICE
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
            (
                SsaValue::ArithExpr {
                    var: super::VarRef::new_typed("counter", 0, crate::ir::VarType::Int),
                    op: super::ArithOp::Add,
                    operand: 1,
                },
                "ArithExpr loop increments must stay in integer slots",
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

    /// The callers select by variant; this function spells the variants out
    /// again. If the two lists drift, a lowerable value arrives here — and that
    /// must be told apart from a genuine refusal, not reported as one.
    #[test]
    fn a_lowerable_value_reports_the_drift_not_a_refusal() {
        let msg = ice_message(&SsaValue::Literal("x".into()));
        assert!(msg.contains("not one of the unlowerable variants"), "{msg}");
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
            assert!(
                msg.contains(if capture {
                    "external value capture"
                } else {
                    "lower_pipeline_body"
                }),
                "{msg}"
            );
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

#[cfg(test)]
mod checked_arith_op_tests {
    use super::{ArithOp, CheckedArithOp};

    /// `CheckedArithOp::from_arith` answers `Some` for exactly the three ops
    /// that can overflow an `i64`, and `None` for the other thirteen.
    ///
    /// Both callers — `codegen::arithmetic::emit_arith_op` and
    /// `emit_c::emit_arith_op_c` — call it *inside* an `Add | Sub | Mul` arm
    /// and immediately `.expect("checked op by arm guard")`, so the `None`
    /// arms never run in a compile and the `.expect()` is load-bearing on a
    /// classification nothing re-checks. This asserts it from the other side:
    /// the arm guard is codegen's claim about this function, and the table
    /// below is the function's own answer. If the two ever disagree, a build
    /// panics in `expect` — but only for a program that reaches that arm.
    ///
    /// The `None` answers are the documented contract (see `from_arith`'s doc
    /// comment for why each op cannot overflow), not a placeholder, which is
    /// why they are pinned rather than turned into an ICE.
    #[test]
    fn from_arith_is_some_for_exactly_the_overflowing_ops() {
        let checked = [
            (ArithOp::Add, CheckedArithOp::Add),
            (ArithOp::Sub, CheckedArithOp::Sub),
            (ArithOp::Mul, CheckedArithOp::Mul),
        ];
        for (op, want) in checked {
            assert_eq!(
                CheckedArithOp::from_arith(op),
                Some(want),
                "{op:?} overflows and must have a checked form"
            );
        }

        let unchecked = [
            ArithOp::Div,
            ArithOp::Mod,
            ArithOp::Pow,
            ArithOp::Shl,
            ArithOp::Shr,
            ArithOp::Eq,
            ArithOp::Ne,
            ArithOp::Lt,
            ArithOp::Le,
            ArithOp::Gt,
            ArithOp::Ge,
            ArithOp::And,
            ArithOp::Or,
            ArithOp::BitAnd,
            ArithOp::BitOr,
            ArithOp::BitXor,
        ];
        for op in unchecked {
            assert_eq!(
                CheckedArithOp::from_arith(op),
                None,
                "{op:?} cannot overflow — a checked form would be a second, \
                 unreachable overflow path"
            );
        }
    }
}

#[cfg(test)]
mod condition_operand_tests {
    use super::{condition_operand_index, SsaInst, SsaValue, VarRef};
    use crate::ir::VarType;

    /// DD#74: `return N` is `Assign(?, N)`, so an assignment to `?` sets the
    /// status a following condition reads and any other assignment is read
    /// straight through. No program can tell the two answers apart, so the
    /// spec suite cannot pin this arm. Every producer of `Assign(?, …)` —
    /// `return N`, an int call's restore, the caught `||` reset, a `case`
    /// with no match — either ends its block in a `Goto`/`Return`, which no
    /// condition scans, or starts a fresh block with nothing before it that
    /// sets the status. Selecting the assignment, or passing over it to
    /// nothing, then unchecks and guards the same thing: nothing.
    #[test]
    fn an_assignment_to_status_is_the_condition_operand() {
        let cmd = SsaInst::command("/bin/true", vec![], vec![], true);
        let assign = |name: &str| SsaInst::Assign {
            target: VarRef::new_typed(name, 1, VarType::Int),
            value: SsaValue::Literal("3".into()),
        };
        assert_eq!(
            condition_operand_index(&[cmd.clone(), assign("?")]),
            Some(1)
        );
        assert_eq!(condition_operand_index(&[cmd, assign("n")]), Some(0));
    }
}
