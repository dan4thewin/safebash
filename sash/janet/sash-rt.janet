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

# integer coercion (leading int; 0x/0o/0b; sign) — the sash arith rule.
(defn sash/-num [v]
  (if (number? v) v (or (scan-number (string v)) 0)))

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

# numeric [[ ]] comparisons ( -eq -ne -lt -le -gt -ge )
(defn sash/-eq [a b] (verdict (= (sash/-num a) (sash/-num b))))
(defn sash/-ne [a b] (verdict (not= (sash/-num a) (sash/-num b))))
(defn sash/-lt [a b] (verdict (< (sash/-num a) (sash/-num b))))
(defn sash/-le [a b] (verdict (<= (sash/-num a) (sash/-num b))))
(defn sash/-gt [a b] (verdict (> (sash/-num a) (sash/-num b))))
(defn sash/-ge [a b] (verdict (>= (sash/-num a) (sash/-num b))))
