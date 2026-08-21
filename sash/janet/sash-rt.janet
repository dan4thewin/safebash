# ═══════════════════════════════════════════════════════════════════════
# sash-rt.janet — the sash runtime primitive surface for the janet backend.
#
# The janet backend renders the sash AST 1-to-1 into the `sash/*` vocabulary
# below, so the emitted program reads as a direct transliteration of how the
# compiler parsed the source (DD#15, amended 2026-08-20). Semantics live
# here; the emitted map stays structural. Taint is compile-time only — a
# `bless` gate runs and aborts/branches, but no clean/tainted flag is carried.
# ═══════════════════════════════════════════════════════════════════════

(var *status* 0)   # $?  — last command's exit status

# Any sash construct not yet mapped by the current stage renders as a
# `(sash/TODO …)` form. Defined here so a partial program still COMPILES under
# janet; if control actually reaches one it aborts with a clear message naming
# the construct — so nothing is ever silently wrong.
(defmacro sash/TODO [& what]
  (def desc (string/join (map |(string $) what) " "))
  ~(error (string "sash/TODO — not yet mapped by this stage: " ,desc)))

# $name — variable read. Special vars map to their runtime slot; a plain var
# nil-coalesces to "" (unset / not-yet-assigned reads as empty).
(defmacro $ [name]
  (case name
    '? '*status*
    ~(or ,name "")))

# ── output ──────────────────────────────────────────────────────────────
(defn sash/echo [& args]
  (if (and (> (length args) 0) (= "-n" (first args)))
    (prin (string/join (map string (drop 1 args)) " "))
    (print (string/join (map string args) " ")))
  (set *status* 0)
  0)

(defn sash/noop [& _] (set *status* 0) 0)   # the `:` builtin

# ── external commands ─────────────────────────────────────────────────────
# A bare (checked) external: run, record $?, and abort on nonzero (DD#55 A).
(defn sash/run [& argv]
  (def p (os/spawn (map string argv) :p))
  (def s (os/proc-wait p))
  (set *status* s)
  (when (not= 0 s) (os/exit s))
  s)

# An external in condition position (unchecked): returns success, never aborts.
(defn sash/ok? [& argv]
  (def p (os/spawn (map string argv) :p))
  (def s (os/proc-wait p))
  (set *status* s)
  (zero? s))

# $(cmd …) — capture a single external command's stdout (trailing \n stripped).
(defn sash/capture [& argv]
  (def p (os/spawn (map string argv) :p {:out :pipe}))
  (def out (or (:read (p :out) :all) ""))
  (os/proc-wait p)
  (set *status* (p :return-code))
  (string/trimr out "\n"))

# ── [[ ]] string/file tests — return bool, set $? ──────────────────────────
(defn- verdict [b] (set *status* (if b 0 1)) b)
(defn sash/= [a b] (verdict (= (string a) (string b))))
(defn sash/not= [a b] (verdict (not= (string a) (string b))))
(defn sash/-n [a] (verdict (> (length (string a)) 0)))
(defn sash/-z [a] (verdict (= 0 (length (string a)))))

# ── bless — run the gate; taint is compile-time so no flag is carried ──────
# Checked (bare) form: a failed gate aborts (like a failed syscall).
(defn sash/bless-checked [test]
  (unless test (eprintf "%s: bless: failed the gate" ((dyn :args) 0)) (os/exit 1))
  (set *status* 0)
  0)
# Condition form (`if bless …`): returns the gate result, never aborts.
(defn sash/bless [test] (verdict test))

# ── control flow — thin macros: emit_janet_map still supplies the structure,
#    these only give it a readable, sash-hugging name (independence kept) ──
(defmacro sash/if [cond then & else] ~(if ,cond ,then ,;else))
(defmacro sash/while [cond body] ~(while ,cond ,body))

# string concatenation (a double-quoted "$x/y" or adjacent parts)
(defn sash/str [& parts] (string/join (map string parts) ""))

# `- cmd` — suppressed: evaluate the (unchecked) inner form; never abort.
(defmacro sash/dash [form] form)

# until — the negated while
(defmacro sash/until [cond body] ~(while (not ,cond) ,body))

# ── user functions (Stage 2) ───────────────────────────────────────────────
# A sash function is variadic — bash positional args ($1..$N / $@ / $#). A
# CALL renders as a direct `(name args…)`; this macro binds the call's args as
# the positional context and wraps the body in a `sash-ret` label so `return`
# unwinds it. Structure is still supplied by emit; this is just the frame.
(defmacro sash/defn [name & body]
  ~(defn ,name [& sash-argv]
     (with-dyns [:sash-argv sash-argv]
       (label sash-ret ,;body))))

# ── function parameters ($1..$N / $@ / $#) ─────────────────────────────────
# A function's OWN arguments — never the process argv. `$1` is sugar for
# `$args[1]` (1-based), so these read the call frame bound by `sash/defn`. The
# front-end bars $1/$@/$# at top level (SB5024); a top-level script reaches the
# process command line through the `arg`/`argc`/`argv` family below instead.
(defn sash/param [n] (or (get (dyn :sash-argv) (- n 1)) ""))  # $1 $2 … (1-based)
(defn sash/nparams [] (length (dyn :sash-argv)))              # $#
(defn sash/params [] (dyn :sash-argv))                        # $@ (spliced at use)

# ── process argv ($(arg N) / $(argc) / $(argv)) ────────────────────────────
# The script's command line — TAINTED input (needs `bless`), and a DIFFERENT
# thing from the function parameters above. Indices are 0-based; index 0 is the
# program name, so $(arg 1) is the first user argument.
(defn sash/argc [] (length (dyn :args)))   # C argc (program slot + user args)
(defn sash/argv [] (dyn :args))            # the whole argv (splat context)
(defn sash/arg [n & default]               # $(arg N [DEFAULT]) — argv[N]
  (if (and (>= n 0) (< n (length (dyn :args))))
    (get (dyn :args) n)
    (if (empty? default)
      (do (eprint (get (dyn :args) 0) ": arg: index out of range") (os/exit 1))
      (first default))))

# exit-code coercion — a return code / $? is numeric (leading int, else 0).
# NOT a [[ ]] numeric-compare (that is a bashism sash rejects); this is only
# the status channel.
(defn sash/-code [v] (if (number? v) v (or (scan-number (string v)) 0)))

# return N — set $? and unwind the enclosing function. Bare `return` re-returns
# the current $?. The unwound value is $? so an `-> int` value function (S4)
# yields it.
(defmacro sash/return [n] ~(do (set *status* (sash/-code ,n)) (return sash-ret *status*)))

# a user-function CALL used as a condition: run it, then read its $? as a
# boolean — the trailing `?` marks the boolean result. Distinct from the
# external `sash/ok?` so the reader sees a sash function, not a spawned program.
(defmacro sash/true? [call] ~(do ,call (zero? *status*)))
