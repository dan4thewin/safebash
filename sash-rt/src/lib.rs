//! sash-rt: pure Rust runtime for compiled safebash binaries.
//!
//! No C, no std, no libc. Uses `rustix` for all syscalls.
//! The only `asm!` is _start (~8 instructions per arch).
//!
//! DD#34: unsafe is confined to boundaries (syscalls, extern "C" entry points,
//! sash_alloc, _start). All pure data manipulation lives in safe inner functions.

#![no_std]
#![no_builtins]
#![allow(improper_ctypes_definitions)]
#![allow(static_mut_refs)]

use rustix::fd::{AsRawFd, BorrowedFd};
use rustix::fs::{memfd_create, MemfdFlags};

#[cfg(feature = "coverage")]
mod coverage;

// ═══════════════════════════════════════════════════════════════
// Libc stubs — required in no_std without libc.
// These are called by compiler-generated code for memcpy etc.
// ═══════════════════════════════════════════════════════════════

#[no_mangle]
pub unsafe extern "C" fn strlen(s: *const u8) -> usize {
    let mut n = 0;
    while *s.add(n) != 0 {
        n += 1;
    }
    n
}

#[no_mangle]
pub unsafe extern "C" fn memcpy(dst: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    let mut i = 0;
    while i < n {
        *dst.add(i) = *src.add(i);
        i += 1;
    }
    dst
}

#[no_mangle]
pub unsafe extern "C" fn memset(dst: *mut u8, c: i32, n: usize) -> *mut u8 {
    let mut i = 0;
    while i < n {
        *dst.add(i) = c as u8;
        i += 1;
    }
    dst
}

#[no_mangle]
pub unsafe extern "C" fn memcmp(a: *const u8, b: *const u8, n: usize) -> i32 {
    let mut i = 0;
    while i < n {
        let diff = *a.add(i) as i32 - *b.add(i) as i32;
        if diff != 0 {
            return diff;
        }
        i += 1;
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn bcmp(a: *const u8, b: *const u8, n: usize) -> i32 {
    memcmp(a, b, n)
}

// ═══════════════════════════════════════════════════════════════
// Panic handler — required for no_std.
// ═══════════════════════════════════════════════════════════════

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    let fd = unsafe { BorrowedFd::borrow_raw(2) };
    // Try to print the panic message if available
    if let Some(msg) = info.message().as_str() {
        let _ = rustix::io::write(fd, msg.as_bytes());
        let _ = rustix::io::write(fd, b"\n");
    } else {
        let _ = rustix::io::write(fd, b"sash: panic\n");
    }
    sash_shutdown(99)
}

// ═══════════════════════════════════════════════════════════════
// Global: envp pointer, set by _start.
// ═══════════════════════════════════════════════════════════════

#[no_mangle]
pub static mut sash_envp: *const *const u8 = core::ptr::null();
#[no_mangle]
pub static mut sash_argc: i64 = 0;
#[no_mangle]
pub static mut sash_argv: *const *const u8 = core::ptr::null();
#[no_mangle]
pub static sash_signum: core::sync::atomic::AtomicI64 = core::sync::atomic::AtomicI64::new(0);
#[no_mangle]
pub static sash_exit_signum: core::sync::atomic::AtomicI64 = core::sync::atomic::AtomicI64::new(0);
#[no_mangle]
pub static mut sash_trace_flag: i64 = 0;

// ═══════════════════════════════════════════════════════════════
// Safe inner functions — no raw pointers, no unsafe
// Defined in inner.rs, shared with tests via include!
// ═══════════════════════════════════════════════════════════════

include!("inner.rs");

// ═══════════════════════════════════════════════════════════════
// sash_write
// ═══════════════════════════════════════════════════════════════

/// DD#120: the invoker's own standard fds.
///
/// A redirect written on a command (`cmd > f`) establishes a channel for
/// *that command's* output. It is not the invoking program's channel, and
/// nothing the program itself emits may land in it — neither a diagnostic
/// about the command (if the command could not be set up or exec'd, it
/// produced no output at all) nor the output of an END block that runs after
/// the command aborted.
///
/// Index `i` holds a high-numbered `F_DUPFD_CLOEXEC` copy of fd `i`, or `-1`
/// if it could not be taken — in which case the affected fd falls back to its
/// pre-DD#120 behaviour, which is never worse.
static mut SASH_SCOPE_SAVE: [i64; 3] = [-1, -1, -1];

/// `F_DUPFD_CLOEXEC` targets start high to stay clear of anything a program
/// redirects by hand.
const SCOPE_MIN_FD: i64 = 100;

static mut SASH_USER_FDS: &[i64] = &[];

/// The compiler supplies static, sorted metadata before any source code runs.
#[no_mangle]
pub extern "C" fn sash_reserve_fds(fds: *const i64, count: i64) {
    unsafe {
        SASH_USER_FDS = core::slice::from_raw_parts(fds, count as usize);
    }
    sash_scope_bind();
}

/// Bind the invoker's scope fds to whatever 0/1/2 mean *right now*.
///
/// Called twice, for the two moments at which 0/1/2 are by definition the
/// program's own: from the prologue before any user code runs, and after a
/// persistent (`exec`-scope) redirect is applied. `exec 2> f` rebinds the
/// **invoker's own** stderr so the scope follows it; `cmd 2> f` does not, and
/// must not. The two are therefore in lockstep in the invoker, diverging only
/// for the window in which fd 2 does not mean "the invoker's stderr".
///
/// `F_DUPFD_CLOEXEC` is load-bearing rather than hygiene: it is exactly what
/// makes the forked child's exec-failure diagnostic land on the invoker's
/// channel. The fd is **still open when `execve` fails**, so the child can
/// report through it, and is **closed automatically when `execve` succeeds**,
/// so a real command never inherits a stray descriptor.
#[no_mangle]
pub extern "C" fn sash_scope_bind() {
    for (idx, slot) in unsafe { (&raw mut SASH_SCOPE_SAVE).as_mut() }
        .expect("scope save table")
        .iter_mut()
        .enumerate()
    {
        // The index IS the fd number: slot 0 saves fd 0, and so on.
        let fd = dup_unreserved(idx as i64, SCOPE_MIN_FD, &[]);
        let old = *slot;
        *slot = if fd >= 0 { fd } else { -1 };
        if old >= 0 {
            sash_close(old);
        }
    }
}

/// Put the invoker's fds back (DD#120). Called on entry to the END block.
///
/// An abort inside a redirected command terminates the block before its
/// restore can run, so without this END would inherit the failed command's
/// redirects and write `END { echo cleanup }` into that command's output
/// file. Restoring here makes END's channel correct by construction rather
/// than by remembering to restore at each abort site.
///
/// The saves stay open: END may itself abort, and the process exit closes
/// them.
#[no_mangle]
pub extern "C" fn sash_scope_restore() {
    for (idx, &slot) in unsafe { (&raw const SASH_SCOPE_SAVE).as_ref() }
        .expect("scope save table")
        .iter()
        .enumerate()
    {
        if slot >= 0 {
            sash_dup2(slot, idx as i64);
        }
    }
}

/// `fcntl(fd, F_DUPFD_CLOEXEC, min)`. Raw syscall, matching [`sash_dup`] —
/// rustix is used where it reads well, but the fd-plumbing helpers in this
/// file are deliberately syscall-level.
fn sash_fcntl_dupfd_cloexec(fd: i64, min: i64) -> i64 {
    const SYS_FCNTL: i64 = 72;
    const F_DUPFD_CLOEXEC: i64 = 1030;
    unsafe {
        let result: i64;
        core::arch::asm!(
            "syscall",
            in("rax") SYS_FCNTL,
            in("rdi") fd,
            in("rsi") F_DUPFD_CLOEXEC,
            in("rdx") min,
            lateout("rax") result,
            lateout("rcx") _,
            lateout("r11") _,
        );
        result
    }
}

fn user_fd(fd: i64) -> bool {
    unsafe { SASH_USER_FDS }.binary_search(&fd).is_ok()
}

fn dup_unreserved(fd: i64, mut min: i64, protected: &[i64]) -> i64 {
    loop {
        let saved = sash_fcntl_dupfd_cloexec(fd, min);
        if saved < 0 || (!user_fd(saved) && !protected.contains(&saved)) {
            return saved;
        }
        sash_close(saved);
        min = saved + 1;
    }
}

fn protect_owned_fd(fd: i64) -> i64 {
    let moved = try_protect_owned_fd(fd);
    if moved < 0 && moved != -9 {
        set_last_errno_raw(-moved);
        fd_backup_abort(b"save fd");
    }
    moved
}

fn try_protect_owned_fd(fd: i64) -> i64 {
    if fd >= 3 && !user_fd(fd) {
        return fd;
    }
    let moved = dup_unreserved(fd, 3, &[]);
    sash_close(fd);
    moved
}

/// Write one diagnostic segment to the invoker's channel (DD#120).
/// Every `sash_write(2, …)` in this file used to do this directly; routing
/// them through one helper is what makes the channel a single object.
fn diag_write(buf: *const u8, len: i64) -> i64 {
    let fd = unsafe { SASH_SCOPE_SAVE[2] };
    sash_write(if fd >= 0 { fd } else { 2 }, buf, len)
}

#[no_mangle]
pub extern "C" fn sash_write(fd: i64, buf: *const u8, len: i64) -> i64 {
    // A *pure* `$( … )` captures into memory, not a kernel object. Every
    // in-process write to stdout funnels through here, so this one branch is
    // the whole diversion. See the buffered-capture block below.
    if fd == 1 {
        if let Some(n) = capture_buf_append(buf, len as usize) {
            return n;
        }
    }
    let slice = unsafe { core::slice::from_raw_parts(buf, len as usize) };
    let fd = unsafe { BorrowedFd::borrow_raw(fd as i32) };
    match rustix::io::write(fd, slice) {
        Ok(n) => n as i64,
        Err(e) => {
            set_last_errno(e);
            -1
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// DD#55 abort diagnostics
// ═══════════════════════════════════════════════════════════════

/// errno of the most recent failed syscall wrapper (chdir/open/write).
/// Set on the failure path; read by `sash_emsg` to render the reason.
static mut SASH_LAST_ERRNO: i64 = 0;

#[inline]
fn set_last_errno(e: rustix::io::Errno) {
    unsafe {
        SASH_LAST_ERRNO = e.raw_os_error() as i64;
    }
}

/// Record a raw (positive) errno for wrappers that issue the syscall directly
/// (e.g. `sash_close`) and thus have no `rustix::io::Errno` in hand.
#[inline]
fn set_last_errno_raw(errno: i64) {
    unsafe {
        SASH_LAST_ERRNO = errno;
    }
}

/// Standard message for the common file/IO errnos. Unknown values fall
/// back to a numeric form in `sash_emsg`.
fn errno_str(errno: i64) -> Option<&'static [u8]> {
    Some(match errno {
        1 => b"Operation not permitted" as &[u8],
        2 => b"No such file or directory",
        5 => b"Input/output error",
        6 => b"No such device or address",
        9 => b"Bad file descriptor",
        13 => b"Permission denied",
        17 => b"File exists",
        19 => b"No such device",
        20 => b"Not a directory",
        21 => b"Is a directory",
        22 => b"Invalid argument",
        23 => b"Too many open files in system",
        24 => b"Too many open files",
        27 => b"File too large",
        28 => b"No space left on device",
        30 => b"Read-only file system",
        32 => b"Broken pipe",
        36 => b"File name too long",
        40 => b"Too many levels of symbolic links",
        122 => b"Disk quota exceeded",
        _ => return None,
    })
}

/// Write a DD#55 abort diagnostic to stderr (no exit):
///   `<argv0>: <seg1>[: <seg2>]: <reason>\n`
/// where `<reason>` = strerror(SASH_LAST_ERRNO). `seg2` is skipped when
/// `seg2_len == 0`. Called at the codegen abort site (so caught/`-`
/// suppressed failures stay silent). `argv0` is the program name.
#[no_mangle]
pub extern "C" fn sash_emsg(
    seg1_ptr: *const u8,
    seg1_len: i64,
    seg2_ptr: *const u8,
    seg2_len: i64,
) {
    let sep: &[u8] = b": ";
    // <argv0>
    let argv = unsafe { sash_argv };
    if !argv.is_null() {
        let a0 = unsafe { *argv };
        if !a0.is_null() {
            let l = unsafe { strlen(a0) };
            diag_write(a0, l as i64);
        }
    }
    diag_write(sep.as_ptr(), sep.len() as i64);
    // <seg1>
    if !seg1_ptr.is_null() && seg1_len > 0 {
        diag_write(seg1_ptr, seg1_len);
    }
    // [: <seg2>]
    if !seg2_ptr.is_null() && seg2_len > 0 {
        diag_write(sep.as_ptr(), sep.len() as i64);
        diag_write(seg2_ptr, seg2_len);
    }
    diag_write(sep.as_ptr(), sep.len() as i64);
    // <reason>
    let errno = unsafe { SASH_LAST_ERRNO };
    match errno_str(errno) {
        Some(s) => {
            diag_write(s.as_ptr(), s.len() as i64);
        }
        None => {
            let prefix: &[u8] = b"error ";
            diag_write(prefix.as_ptr(), prefix.len() as i64);
            let mut buf = [0u8; 24];
            let n = write_i64(&mut buf, errno);
            diag_write(buf.as_ptr(), n as i64);
        }
    }
    let nl: &[u8] = b"\n";
    diag_write(nl.as_ptr(), nl.len() as i64);
}

/// Write a DD#55 status-based abort diagnostic to stderr (no exit):
///   `<argv0>: <name>: command failed (exit <status>)\n`
/// Used for external / pipeline / command-substitution aborts, where the
/// failure is a nonzero child exit status (there is no errno). `argv0` is
/// the program name; `name` is the command that failed.
#[no_mangle]
pub extern "C" fn sash_emsg_status(name_ptr: *const u8, name_len: i64, status: i64) {
    let sep: &[u8] = b": ";
    let argv = unsafe { sash_argv };
    if !argv.is_null() {
        let a0 = unsafe { *argv };
        if !a0.is_null() {
            let l = unsafe { strlen(a0) };
            diag_write(a0, l as i64);
        }
    }
    diag_write(sep.as_ptr(), sep.len() as i64);
    if !name_ptr.is_null() && name_len > 0 {
        diag_write(name_ptr, name_len);
        diag_write(sep.as_ptr(), sep.len() as i64);
    }
    let head: &[u8] = b"command failed (exit ";
    diag_write(head.as_ptr(), head.len() as i64);
    let mut buf = [0u8; 24];
    let n = write_i64(&mut buf, status);
    diag_write(buf.as_ptr(), n as i64);
    let tail: &[u8] = b")\n";
    diag_write(tail.as_ptr(), tail.len() as i64);
}

// ═══════════════════════════════════════════════════════════════
// sash_exit
// ═══════════════════════════════════════════════════════════════

/// The teardown sequence — the mirror of `sash_startup`, and the single
/// place any sash program leaves the process.
///
/// It exists for the reason `sash_startup` does. `_start` used to inline the
/// `exit` syscall after `call main`, so the *normal* exit path was two
/// instructions of per-architecture assembly that no Rust code could hook,
/// while the seven abnormal paths went through `exit_group` directly. Any
/// at-exit work therefore had eight sites to add and two of them were in
/// assembly, written twice. Adding a teardown step now means editing this
/// function.
#[no_mangle]
pub extern "C" fn sash_shutdown(status: i64) -> ! {
    #[cfg(feature = "coverage")]
    coverage::dump();
    rustix::runtime::exit_group(status as i32)
}

#[no_mangle]
pub extern "C" fn sash_exit(status: i64) -> ! {
    sash_shutdown(status)
}

/// Attest-gate failure diagnostic, then exit(1):
///   `<argv0>: attest: <name> = '<val>' failed the gate\n`
/// The ` = '<val>'` clause is omitted when `val_len == 0` (e.g. a attest-all
/// over a collection, where there is no single scalar value). Naming the
/// variable and showing its value tells the user *which* gate failed and
/// *why* (the value that did not satisfy the predicate).
#[no_mangle]
pub extern "C" fn sash_attest_fail(
    name_ptr: *const u8,
    name_len: i64,
    val_ptr: *const u8,
    val_len: i64,
) -> ! {
    let argv = unsafe { sash_argv };
    if !argv.is_null() {
        let a0 = unsafe { *argv };
        if !a0.is_null() {
            let l = unsafe { strlen(a0) };
            diag_write(a0, l as i64);
        }
    }
    let head: &[u8] = b": attest: ";
    diag_write(head.as_ptr(), head.len() as i64);
    if !name_ptr.is_null() && name_len > 0 {
        diag_write(name_ptr, name_len);
    }
    if !val_ptr.is_null() && val_len > 0 {
        let open: &[u8] = b" = '";
        diag_write(open.as_ptr(), open.len() as i64);
        diag_write(val_ptr, val_len);
        let close: &[u8] = b"'";
        diag_write(close.as_ptr(), close.len() as i64);
    }
    let tail: &[u8] = b" failed the gate\n";
    diag_write(tail.as_ptr(), tail.len() as i64);
    sash_exit(1)
}

/// DD#25: integer overflow trap. Writes `<argv0>: arithmetic overflow\n`
/// to stderr and exits non-zero. Called from the codegen overflow-check
/// abort blocks (native + C backends) when a runtime +, -, or * over- or
/// underflows. bug-runtime-arith-overflow-wraps.
#[no_mangle]
pub extern "C" fn sash_arith_overflow() -> ! {
    arith_abort(b"arithmetic overflow\n")
}

/// DD#25 (amended): division or modulo by zero. Same disposition as
/// `sash_arith_overflow` — every arithmetic fault aborts with `<argv0>: …`
/// and exit 2.
///
/// This used to write bash's "division by 0 (error token is …)" to stderr,
/// yield 0, and CONTINUE, which made it the only arithmetic fault in the
/// tree that produced a value and kept running. Its sibling in the very same
/// codegen function — `i64::MIN / -1`, the other x86 #DE cause — already
/// aborted here.
///
/// The check that calls this is deliberate, not a fallback: relying on the
/// hardware #DE instead would report `Floating point exception` with no
/// argv0 and a misleading "floating point" for an integer divide.
#[no_mangle]
pub extern "C" fn sash_arith_divzero() -> ! {
    arith_abort(b"division by zero\n")
}

/// DD#25: a NEGATIVE shift count aborts. Shifts take an unbounded
/// non-negative count (see `emit_shift`); a negative one is meaningless in
/// any model and, left to the hardware, would be silently reinterpreted as a
/// large positive count by two's-complement masking (`8 >> -64` == `8`).
#[no_mangle]
pub extern "C" fn sash_arith_negshift() -> ! {
    arith_abort(b"negative shift count\n")
}

/// `<argv0>: <msg>` to stderr, then exit 2. Shared by the arithmetic traps
/// so they cannot drift apart in prefix, stream, or exit code.
fn arith_abort(msg: &[u8]) -> ! {
    let sep: &[u8] = b": ";
    let argv = unsafe { sash_argv };
    if !argv.is_null() {
        let a0 = unsafe { *argv };
        if !a0.is_null() {
            let l = unsafe { strlen(a0) };
            diag_write(a0, l as i64);
            diag_write(sep.as_ptr(), sep.len() as i64);
        }
    }
    diag_write(msg.as_ptr(), msg.len() as i64);
    sash_exit(2)
}

// ═══════════════════════════════════════════════════════════════
// Composable process primitives (Design Decision #14)
// ═══════════════════════════════════════════════════════════════

/// Compute length of a null-terminated byte string.
unsafe fn cstr_len(p: *const u8) -> usize {
    let mut n = 0;
    while *p.add(n) != 0 {
        n += 1;
    }
    n
}

/// Wrap a null-terminated pointer as a CStr-like byte slice (without the null).
unsafe fn cstr_bytes(p: *const u8) -> &'static [u8] {
    core::slice::from_raw_parts(p, cstr_len(p))
}

/// Fork. Returns child pid in parent, 0 in child, -1 on error.
#[no_mangle]
pub extern "C" fn sash_fork() -> i64 {
    match unsafe { rustix::runtime::kernel_fork() } {
        Ok(rustix::runtime::Fork::Child(_)) => 0,
        Ok(rustix::runtime::Fork::ParentOf(pid)) => pid.as_raw_nonzero().get() as i64,
        Err(e) => {
            set_last_errno(e);
            -1
        }
    }
}

/// Exec with absolute path only (DD#23). Does not return on success.
#[no_mangle]
pub extern "C" fn sash_exec(argv: *const *const u8, envp: *const *const u8) -> ! {
    unsafe {
        let cmd = *argv;
        if cmd.is_null() {
            sash_shutdown(127);
        }

        let cmd_bytes = cstr_bytes(cmd);
        let has_slash = cmd_bytes.iter().any(|&b| b == b'/');
        let cmd_cstr = core::ffi::CStr::from_ptr(cmd as *const i8);

        if !has_slash {
            // DD#23: sash never walks PATH. The compiler resolves all
            // commands to absolute paths. A bare name here is a bug.
            let msg = b"sash: bug: bare command passed to sash_exec (expected absolute path)\n";
            let fd = BorrowedFd::borrow_raw(2);
            let _ = rustix::io::write(fd, msg);
            sash_shutdown(127);
        }

        // execve only returns on failure — it yields the `Errno` directly.
        // DD#55: the child owns exec errors — report `<argv0>: <cmd>:
        // <errno>` at the point of failure (covers fg via sash_run, bg, and
        // pipeline stages), then exit 127. The parent's `command failed
        // (exit N)` is separate and fires only when the failure is uncaught.
        let e = rustix::runtime::execve(cmd_cstr, argv, envp);
        set_last_errno(e);
        sash_emsg(cmd, cmd_bytes.len() as i64, core::ptr::null(), 0);
        sash_shutdown(127);
    }
}

/// Wait for a child process. Returns exit status, or 127 on error.
///
/// A child killed by a signal yields `128 + signum`, the convention bash uses
/// and the one `emit_signal_exit_check` already applies to the program's own
/// signal deaths. Returning a bare `128` lost the signal number, so every
/// signal death reported the same status: a SIGPIPE stage read as 128 where
/// bash's pipefail aggregate gives 141, and SIGTERM as 128 against bash's 143.
#[no_mangle]
pub extern "C" fn sash_wait(pid: i64) -> i64 {
    let pid = unsafe { rustix::process::Pid::from_raw_unchecked(pid as i32) };
    loop {
        match rustix::process::waitpid(Some(pid), rustix::process::WaitOptions::empty()) {
            Ok(Some((_pid, status))) => return reaped_status(status),
            Ok(None) => return 0,
            Err(e) if e == rustix::io::Errno::INTR => continue,
            Err(_) => return 127,
        }
    }
}

/// The exit status of a reaped child, in the one encoding sash uses.
///
/// Shared by [`sash_wait`] and [`sash_wait_any`] rather than written twice:
/// the `128 + signum` convention is the kind of detail that drifts when it is
/// duplicated, and a disagreement between the pipeline waiter and the `wait`
/// builtin about what a SIGPIPE death reports would be invisible.
fn reaped_status(status: rustix::process::WaitStatus) -> i64 {
    if let Some(code) = status.exit_status() {
        return code as i64;
    }
    match status.terminating_signal() {
        Some(sig) => 128 + sig as i64,
        None => 128,
    }
}

/// DD#126: reap one child and report **which** one.
///
/// `pid <= 0` is `wait(2)` — reap any child. A positive `pid` is
/// `waitpid(2)` on that child. Returns the reaped pid, or `-1` when there is
/// nothing to reap (ECHILD) or the call fails. It returns a *pid*, never a
/// status, and it never aborts: "no children" is a value the program can test,
/// which is what makes `while (( (wait) > 0 )) { … }` a drain loop.
///
/// The reaped child's status goes into [`SASH_WAIT_STATUS`], which
/// `$(waitstatus)` reads; it does **not** touch `$?`. A latch maintained by
/// two code generators is a latch they will eventually disagree about —
/// literally how the childless-`wait` divergence happened — so it lives here,
/// written on every call, in one place.
///
/// `sash_wait` is deliberately left alone: the pipeline waiter wants the
/// status of a *named* stage and has no use for the pid.
#[no_mangle]
pub extern "C" fn sash_wait_any(pid: i64) -> i64 {
    let opts = rustix::process::WaitOptions::empty();
    let reaped = if pid > 0 {
        let target = unsafe { rustix::process::Pid::from_raw_unchecked(pid as i32) };
        rustix::process::waitpid(Some(target), opts)
    } else {
        // `waitpid(None, …)` is `waitpid(0, …)` in rustix — "any child in my
        // process group", not "any child". `wait` is the one that is `-1`.
        rustix::process::wait(opts)
    };
    match reaped {
        Ok(Some((child, status))) => {
            unsafe { SASH_WAIT_STATUS = reaped_status(status) };
            child.as_raw_nonzero().get() as i64
        }
        _ => {
            // Nothing reaped, so there is no child's status to report. -1 is
            // the same "no child" answer the return value gives, and a real
            // exit status can never be negative, so it cannot be mistaken for
            // one. Written rather than left alone: a stale status from an
            // earlier reap would be a wrong answer, where this is a knowable
            // one.
            unsafe { SASH_WAIT_STATUS = -1 };
            -1
        }
    }
}

/// DD#126: the latch `$(waitstatus)` reads — the status of the child the last
/// `wait` reaped, or `-1` if it reaped none. Process-global, so a background
/// child that waits latches its own, which is what a separate process should
/// see.
static mut SASH_WAIT_STATUS: i64 = -1;

/// Read the latch. `validate` only admits a read in the statement immediately
/// after the `wait` that wrote it, so the `-1` initial value is unobservable;
/// it is there because an uninitialised static is not a thing this crate has.
#[no_mangle]
pub extern "C" fn sash_wait_status() -> i64 {
    unsafe { SASH_WAIT_STATUS }
}

/// Redirect fd to a file.
#[no_mangle]
pub extern "C" fn sash_redirect(fd: i64, path: *const u8, flags: i64) -> i64 {
    unsafe {
        let path_cstr = core::ffi::CStr::from_ptr(path as *const i8);
        let oflags = match flags {
            0 => rustix::fs::OFlags::RDONLY,
            1 => rustix::fs::OFlags::WRONLY
                .union(rustix::fs::OFlags::CREATE)
                .union(rustix::fs::OFlags::TRUNC),
            2 => rustix::fs::OFlags::WRONLY
                .union(rustix::fs::OFlags::CREATE)
                .union(rustix::fs::OFlags::APPEND),
            // `<>` (RedirectOp::ReadWrite): read/write, create if absent, and
            // do NOT truncate — bash's semantics for the form.
            3 => rustix::fs::OFlags::RDWR.union(rustix::fs::OFlags::CREATE),
            _ => return -1,
        };
        let mode = rustix::fs::Mode::from_raw_mode(0o666);
        let opened = match rustix::fs::open(path_cstr, oflags, mode) {
            Ok(f) => f,
            Err(e) => {
                set_last_errno(e);
                return -1;
            }
        };
        let src = rustix::fd::IntoRawFd::into_raw_fd(opened) as i64;
        let result = sash_dup2(src, fd);
        // `dup2(src, fd)` with `src == fd` is a POSIX no-op that closes
        // nothing, so an unconditional close here destroys the redirect we
        // just installed. `open` hands back the LOWEST free fd, so this fires
        // exactly when the target fd is itself the lowest free one — e.g.
        // `exec 3> f` with only 0-2 open. fd 3 was left closed and every later
        // `>&3` silently fell through to the real stdout.
        if src != fd {
            sash_close(src);
        }
        result
    }
}

/// dup2(src_fd, dst_fd).
#[no_mangle]
pub extern "C" fn sash_dup2(src_fd: i64, dst_fd: i64) -> i64 {
    unsafe {
        let result: i64;
        core::arch::asm!(
            "syscall",
            in("rax") 33_i64,
            in("rdi") src_fd,
            in("rsi") dst_fd,
            lateout("rax") result,
            lateout("rcx") _,
            lateout("r11") _,
        );
        if result < 0 {
            set_last_errno_raw(-result);
            -1
        } else {
            0
        }
    }
}

/// Create a pipe.
#[no_mangle]
pub extern "C" fn sash_pipe(out_ptr: *mut i64) -> i64 {
    match rustix::pipe::pipe_with(rustix::pipe::PipeFlags::CLOEXEC) {
        Ok((r, w)) => {
            unsafe {
                let r_raw = rustix::fd::IntoRawFd::into_raw_fd(r) as i64;
                let w_raw = rustix::fd::IntoRawFd::into_raw_fd(w) as i64;
                let read = try_protect_owned_fd(r_raw);
                if read < 0 {
                    sash_close(w_raw);
                    set_last_errno_raw(-read);
                    return -1;
                }
                let write = try_protect_owned_fd(w_raw);
                if write < 0 {
                    sash_close(read);
                    set_last_errno_raw(-write);
                    return -1;
                }
                *out_ptr = read;
                *out_ptr.add(1) = write;
            }
            0
        }
        Err(e) => {
            set_last_errno(e);
            -1
        }
    }
}

/// Close a file descriptor.
#[no_mangle]
pub extern "C" fn sash_close(fd: i64) -> i64 {
    unsafe {
        let result: i64;
        core::arch::asm!(
            "syscall",
            in("rax") 3_i64,
            in("rdi") fd,
            lateout("rax") result,
            lateout("rcx") _,
            lateout("r11") _,
        );
        if result < 0 {
            // Raw syscall: the negative return is `-errno`. Record it so a
            // DD#55 `close` abort renders the real reason (e.g. EBADF).
            set_last_errno_raw(-result);
            -1
        } else {
            0
        }
    }
}

/// Read from a file descriptor.
#[no_mangle]
pub extern "C" fn sash_read(fd: i64, buf: *mut u8, len: i64) -> i64 {
    let fd = unsafe { BorrowedFd::borrow_raw(fd as i32) };
    let slice = unsafe { core::slice::from_raw_parts_mut(buf, len as usize) };
    match rustix::io::read(fd, slice) {
        Ok(n) => n as i64,
        Err(_) => -1,
    }
}

// ═══════════════════════════════════════════════════════════════
// Legacy wrappers
// ═══════════════════════════════════════════════════════════════

#[no_mangle]
pub extern "C" fn sash_run(argv: *const *const u8, envp: *const *const u8) -> i64 {
    let pid = sash_fork();
    if pid == 0 {
        sash_exec(argv, envp);
    } else if pid > 0 {
        sash_wait(pid)
    } else {
        // fork(2) failed — the parent owns fork errors. Report
        // `<argv0>: <cmd>: <errno>` (errno set by sash_fork); the caller's
        // uncaught-abort check aborts the program unless it is caught.
        let cmd = unsafe { *argv };
        if !cmd.is_null() {
            let cmd_len = unsafe { strlen(cmd) };
            sash_emsg(cmd, cmd_len as i64, core::ptr::null(), 0);
        }
        127
    }
}

// ═══════════════════════════════════════════════════════════════
// sash_getenv
// ═══════════════════════════════════════════════════════════════

#[no_mangle]
pub extern "C" fn sash_getenv(name: *const u8, name_len: i64, out_len: *mut i64) -> *const u8 {
    let envp = unsafe { sash_envp };
    if envp.is_null() {
        unsafe { *out_len = 0 };
        return core::ptr::null();
    }
    let name_slice = unsafe { core::slice::from_raw_parts(name, name_len as usize) };
    let mut i = 0;
    loop {
        let entry = unsafe { *envp.add(i) };
        if entry.is_null() {
            break;
        }
        let entry_len = unsafe { strlen(entry) };
        let entry_slice = unsafe { core::slice::from_raw_parts(entry, entry_len) };
        if entry_len > name_slice.len()
            && entry_slice[name_slice.len()] == b'='
            && &entry_slice[..name_slice.len()] == name_slice
        {
            let val_start = name_slice.len() + 1;
            let val_len = entry_len - val_start;
            unsafe { *out_len = val_len as i64 };
            return unsafe { entry.add(val_start) };
        }
        i += 1;
    }
    unsafe { *out_len = 0 };
    core::ptr::null()
}

// ═══════════════════════════════════════════════════════════════
// sash_argv_get
// ═══════════════════════════════════════════════════════════════

/// Get program argument by 0-based index (DD#60 `$(arg N)`).
/// `arg 0` is the program name (argv[0]); `arg 1` the first user arg.
/// Panics (aborts) if index is out of range.
#[no_mangle]
pub extern "C" fn sash_argv_get(idx: i64, out_len: *mut i64) -> *const u8 {
    let argc = unsafe { sash_argc };
    // idx is 0-based C argv: argv[0] is the program name, argv[1..] args.
    if idx < 0 || idx >= argc {
        sash_panic_argv(idx, argc);
    }
    let argv = unsafe { sash_argv };
    let ptr = unsafe { *argv.add(idx as usize) };
    let len = unsafe { strlen(ptr) };
    unsafe { *out_len = len as i64 };
    ptr
}

/// DD#60 addendum: `$(arg N DEFAULT)` — like `sash_argv_get`, but an
/// out-of-range index returns the (already-evaluated) default `(d_ptr, d_len)`
/// instead of aborting. Any invalid index (negative or `>= argc`) falls back to
/// the default. The default is constrained at compile time (DD#78: clean,
/// side-effect-free), so evaluating it eagerly at the call site is safe.
#[no_mangle]
pub extern "C" fn sash_argv_get_or(
    idx: i64,
    d_ptr: *const u8,
    d_len: i64,
    out_len: *mut i64,
) -> *const u8 {
    let argc = unsafe { sash_argc };
    if idx < 0 || idx >= argc {
        unsafe { *out_len = d_len };
        return d_ptr;
    }
    let argv = unsafe { sash_argv };
    let ptr = unsafe { *argv.add(idx as usize) };
    let len = unsafe { strlen(ptr) };
    unsafe { *out_len = len as i64 };
    ptr
}

// ═══════════════════════════════════════════════════════════════
// sash_argv_snapshot — DD#60: array a=$(argv N)
// ═══════════════════════════════════════════════════════════════

/// Snapshot the program argv slice argv[start..argc] into a fresh indexed
/// table (DD#60 `array a=$(argv N)`). The snapshot is 0-indexed: element 0
/// is argv[start]. argv pointers reference static program memory, so only
/// the keys are heap-copied (by sash_table_set). `start >= argc` yields an
/// empty table. The result is a tainted indexed array (DD#61 bulk read).
#[no_mangle]
pub unsafe extern "C" fn sash_argv_snapshot(start: i64) -> *mut SashTable {
    let argc = sash_argc;
    let count = if start >= argc { 0 } else { argc - start };
    let t = sash_table_new(if count < 4 { 4 } else { count });
    let argv = sash_argv;
    let mut i: i64 = 0;
    while start + i < argc {
        let key = i.to_ne_bytes();
        let ptr = *argv.add((start + i) as usize);
        let len = strlen(ptr) as i64;
        sash_table_set(t, key.as_ptr(), 8, ptr, len);
        i += 1;
    }
    t
}

// ═══════════════════════════════════════════════════════════════
// sash_keys_snapshot — DD#75: keys m k
// ═══════════════════════════════════════════════════════════════

/// Snapshot a map's string keys into a fresh indexed table (DD#75 `keys m k`),
/// the keys counterpart of [`sash_argv_snapshot`]. Element `i` is the key of
/// the map's `i`th live entry, so the snapshot is in the same insertion order
/// `$(keys m)` iterates and `sash_table_key` reports.
///
/// Both the keys and the values of the result are heap-copied by
/// `sash_table_set`, so the array outlives any later `unset`/overwrite of the
/// source map. That is the point of a *snapshot*: `$(keys m)` iterates the
/// live map, this does not.
///
/// `sash_table_key` is itself O(live entries), so this is O(n²) in the map
/// size — the same cost the `$(keys m)` for-in loop already pays per
/// iteration, and not a new one.
#[no_mangle]
pub unsafe extern "C" fn sash_keys_snapshot(src: *const SashTable) -> *mut SashTable {
    let n = sash_table_len(src);
    let t = sash_table_new(if n < 4 { 4 } else { n });
    let mut i: i64 = 0;
    while i < n {
        let mut klen: i64 = 0;
        let kptr = sash_table_key(src, i, &mut klen);
        let idx = i.to_ne_bytes();
        sash_table_set(t, idx.as_ptr(), 8, kptr, klen);
        i += 1;
    }
    t
}

/// DD#69: resolve an external `@{a:…}` splat to the table to expand.
/// Non-empty array → the array itself. Empty array: `:?` (abort=1) aborts
/// (exit 1, `<argv0>: WORD` on stderr); `:-` with a WORD (has_word=1) → a
/// fresh 1-element table `[WORD]`; bare `@{a:-}` (has_word=0) → the empty
/// array (zero args).
#[no_mangle]
pub unsafe extern "C" fn sash_splat_disposition(
    tbl: *mut SashTable,
    word_ptr: *const u8,
    word_len: i64,
    has_word: i64,
    abort: i64,
) -> *mut SashTable {
    if sash_table_len(tbl) > 0 {
        return tbl;
    }
    if abort != 0 {
        let argv = sash_argv;
        if !argv.is_null() {
            let a0 = *argv;
            if !a0.is_null() {
                diag_write(a0, strlen(a0) as i64);
            }
        }
        diag_write(b": ".as_ptr(), 2);
        if !word_ptr.is_null() && word_len > 0 {
            diag_write(word_ptr, word_len);
        }
        diag_write(b"\n".as_ptr(), 1);
        sash_exit(1);
    }
    if has_word != 0 {
        let t = sash_table_new(4);
        let key = 0i64.to_ne_bytes();
        sash_table_set(t, key.as_ptr(), 8, word_ptr, word_len);
        return t;
    }
    tbl
}

/// DD#99 stage 7: an argv entry may not contain an interior NUL.
///
/// argv crosses `execve(2)` as C strings, so a value holding a NUL is
/// truncated there — silently, and with a zero exit. sash scalars are
/// byte-transparent and DD#99 makes NUL the field separator, so a NUL-bearing
/// value reaching argv is now a realistic mistake rather than an exotic one
/// (`read` a NUL-separated stream, forget to split, pass the whole thing).
///
/// Truncation is the one outcome the language must not have: the callee
/// receives a *different, shorter* argument than the program computed and
/// nothing says so. So this aborts (DD#55) instead.
///
/// # Safety
/// `ptr` must be valid for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn sash_argv_guard(ptr: *const u8, len: i64) {
    if ptr.is_null() || len <= 0 {
        return;
    }
    let bytes = core::slice::from_raw_parts(ptr, len as usize);
    if !bytes.contains(&0) {
        return;
    }
    let argv = sash_argv;
    if !argv.is_null() {
        let a0 = *argv;
        if !a0.is_null() {
            diag_write(a0, strlen(a0) as i64);
            diag_write(b": ".as_ptr(), 2);
        }
    }
    sash_write(
        2,
        b"argument contains a NUL byte and would be truncated at exec\n".as_ptr(),
        60,
    );
    sash_exit(1);
}

/// DD#77: `${var:?MESSAGE}` — abort when the scalar is unset/empty. `s_len <= 0`
/// (unset reads as empty) writes `<argv0>: MESSAGE\n` to stderr and exits 1
/// (mirrors the splat `:?` abort); otherwise a no-op. The caller keeps the
/// original `(s_ptr, s_len)` for the value.
#[no_mangle]
pub unsafe extern "C" fn sash_require_value(s_len: i64, msg_ptr: *const u8, msg_len: i64) {
    if s_len > 0 {
        return;
    }
    let argv = sash_argv;
    if !argv.is_null() {
        let a0 = *argv;
        if !a0.is_null() {
            diag_write(a0, strlen(a0) as i64);
        }
    }
    diag_write(b": ".as_ptr(), 2);
    if !msg_ptr.is_null() && msg_len > 0 {
        diag_write(msg_ptr, msg_len);
    }
    diag_write(b"\n".as_ptr(), 1);
    sash_exit(1);
}

#[cold]
#[inline(never)]
fn sash_panic_argv(idx: i64, argc: i64) -> ! {
    // 0-based C argv: available range is 0..argc-1
    let max = argc - 1;
    let mut buf = [0u8; 128];
    let msg = b"sash: argv index out of range (requested ";
    let mut pos = msg.len();
    buf[..pos].copy_from_slice(msg);
    pos += write_i64(&mut buf[pos..], idx);
    let mid = b", have 0..";
    buf[pos..pos + mid.len()].copy_from_slice(mid);
    pos += mid.len();
    pos += write_i64(&mut buf[pos..], max);
    buf[pos] = b')';
    pos += 1;
    buf[pos] = b'\n';
    pos += 1;
    diag_write(buf.as_ptr(), pos as i64);
    sash_exit(1);
}

/// Write an i64 into a byte buffer, return number of bytes written.
fn write_i64(buf: &mut [u8], val: i64) -> usize {
    if val == 0 {
        buf[0] = b'0';
        return 1;
    }
    let negative = val < 0;
    // Magnitude as u64 so i64::MIN formats correctly (its i64 negation
    // overflows). bug-runtime-arith-overflow-wraps.
    let mut mag: u64 = val.unsigned_abs();
    let mut tmp = [0u8; 20];
    let mut i = 0;
    while mag > 0 {
        tmp[i] = b'0' + (mag % 10) as u8;
        mag /= 10;
        i += 1;
    }
    let mut pos = 0;
    if negative {
        buf[pos] = b'-';
        pos += 1;
    }
    for j in (0..i).rev() {
        buf[pos] = tmp[j];
        pos += 1;
    }
    pos
}

// ═══════════════════════════════════════════════════════════════
// Capture
// ═══════════════════════════════════════════════════════════════

/// Drain a capture pipe to EOF and apply bash's `$( )` rule.
///
/// The one place that rule lives. There were four copies of this loop —
/// `sash_capture`, `sash_capture_status`, `sash_capture_end`, and an inline
/// one emitted by native codegen — and they had drifted into disagreeing
/// with each other and with bash on both counts:
///
/// * Three read into a fixed **64 KiB** buffer and stopped. That is not even
///   silent truncation in practice: the reader stops, the pipe fills, and the
///   child dies of SIGPIPE, so `x=$(cat 100kb)` failed with exit 141 rather
///   than returning a short string.
/// * Two stripped only **one** trailing newline. Bash strips them all, so
///   `$(printf 'a\n\n\n')` was `"a\n\n"` where bash gives `"a"`.
///
/// Reads via `sash_read_all`, which grows on the heap, retries `EINTR`, and
/// aborts on a real error instead of reporting it as EOF.
///
/// Always returns a non-null, NUL-terminated buffer; empty output is a valid
/// empty string rather than a null pointer the caller has to remember about.
#[no_mangle]
pub extern "C" fn sash_capture_drain(fd: i64) -> (*mut u8, usize) {
    let (buf, len) = sash_read_all(fd);
    if buf.is_null() {
        let empty = unsafe { alloc_cstr(0) };
        return (empty, 0);
    }
    let mut total = len;
    // Bash strips *every* trailing newline from a command substitution.
    while total > 0 && unsafe { *buf.add(total - 1) } == b'\n' {
        total -= 1;
    }
    unsafe { *buf.add(total) = 0 };
    (buf, total)
}

// ── Buffered capture: the zero-syscall fast path ─────────────────────
//
// `$( … )` over a *pure* body — one that never forks a process onto the
// capture's own stdout and never rebinds fd 1 — needs no kernel object at
// all. The compiler proves purity (`capture_body_is_pure` in `transform/`)
// and emits `sash_capture_start_buf`/`sash_capture_end_buf` in place of the
// memfd pair. Because every in-process stdout write funnels through
// `sash_write`, diverting there collects the body's entire output without a
// single syscall: the measured cost of a 1000-line captured function drops
// from 3014 syscalls to 0.
//
// The *stack* is what makes nesting correct, and it is the reason an impure
// capture may still sit inside a pure one. An impure inner capture really
// does point fd 1 at a memfd, so for its duration the outer buffer must stop
// swallowing writes — otherwise the inner body's output would land in the
// outer's string. `sash_capture_start` therefore pushes a *transparent*
// frame that suspends diversion, and pops it on end. Equally, an external
// command inside a pure body is only safe because it is itself inside an
// inner `$( )` whose fork gets a pipe rather than our stdout; a bare external
// disqualifies the site at compile time.
//
// Depth is static: sash inlines functions and rejects recursion (SB5037), so
// no construct produces dynamic capture nesting. A fixed array is sufficient
// and an overflow is a compiler bug, not user input.

const CAPTURE_STACK_MAX: usize = 64;
const CAPTURE_BUF_INITIAL: usize = 256;

#[derive(Clone, Copy)]
struct CaptureFrame {
    /// `false` is a transparent frame: fd 1 is genuinely redirected, so
    /// writes must reach the kernel rather than any enclosing buffer.
    buffered: bool,
    ptr: *mut u8,
    len: usize,
    cap: usize,
}

static mut CAPTURE_STACK: [CaptureFrame; CAPTURE_STACK_MAX] = [CaptureFrame {
    buffered: false,
    ptr: core::ptr::null_mut(),
    len: 0,
    cap: 0,
}; CAPTURE_STACK_MAX];
static mut CAPTURE_DEPTH: usize = 0;

fn capture_push(frame: CaptureFrame) {
    unsafe {
        if CAPTURE_DEPTH >= CAPTURE_STACK_MAX {
            let fd = BorrowedFd::borrow_raw(2);
            let _ = rustix::io::write(fd, b"sash: bug: capture nesting exceeded 64\n");
            sash_shutdown(99);
        }
        CAPTURE_STACK[CAPTURE_DEPTH] = frame;
        CAPTURE_DEPTH += 1;
    }
}

fn capture_pop() -> CaptureFrame {
    unsafe {
        if CAPTURE_DEPTH == 0 {
            let fd = BorrowedFd::borrow_raw(2);
            let _ = rustix::io::write(fd, b"sash: bug: capture end without start\n");
            sash_shutdown(99);
        }
        CAPTURE_DEPTH -= 1;
        CAPTURE_STACK[CAPTURE_DEPTH]
    }
}

/// Absorb a stdout write into the active buffered capture, if there is one.
/// `Some(n)` means the bytes were taken and the caller must not also write
/// them; `None` means the write goes to the fd as usual — either no capture
/// is live, or the innermost one is transparent.
fn capture_buf_append(buf: *const u8, len: usize) -> Option<i64> {
    unsafe {
        if CAPTURE_DEPTH == 0 {
            return None;
        }
        let frame = &mut CAPTURE_STACK[CAPTURE_DEPTH - 1];
        if !frame.buffered {
            return None;
        }
        // The `+ 1` reserves the DD#87 NUL terminator up front, so the
        // terminating write in `sash_capture_end_buf` can never need a grow.
        let needed = frame.len + len + 1;
        if needed > frame.cap {
            let mut new_cap = frame.cap;
            while new_cap < needed {
                new_cap *= 2;
            }
            let grown = sash_crealloc(frame.ptr, new_cap);
            if grown.is_null() {
                sash_panic_oom(new_cap);
            }
            frame.ptr = grown;
            frame.cap = new_cap;
        }
        core::ptr::copy_nonoverlapping(buf, frame.ptr.add(frame.len), len);
        frame.len += len;
        Some(len as i64)
    }
}

/// Begin a buffered capture. No syscall, no fd: stdout stays exactly as it
/// is, and `sash_write` diverts into this frame until the matching end.
#[no_mangle]
pub extern "C" fn sash_capture_start_buf() {
    let ptr = unsafe { sash_alloc(CAPTURE_BUF_INITIAL) };
    capture_push(CaptureFrame {
        buffered: true,
        ptr,
        len: 0,
        cap: CAPTURE_BUF_INITIAL,
    });
}

/// End a buffered capture, yielding (ptr, len) with trailing newlines
/// stripped — the same contract as `sash_capture_drain`, so the two capture
/// paths are interchangeable to the caller. The strip is a length decrement
/// rather than a copy.
#[no_mangle]
pub extern "C" fn sash_capture_end_buf() -> (*mut u8, usize) {
    let frame = capture_pop();
    let mut total = frame.len;
    // Bash strips *every* trailing newline from a command substitution.
    while total > 0 && unsafe { *frame.ptr.add(total - 1) } == b'\n' {
        total -= 1;
    }
    unsafe { *frame.ptr.add(total) = 0 };
    (frame.ptr, total)
}

// Simple bump allocator — must stay unsafe (DD#34)
// ── Heap — mmap'd region managed by heap_4 ────────────────────────────
// heap4.c provides sash_malloc/sash_free/sash_realloc (first-fit + coalesce).
// We initialize it from _start with an mmap'd anonymous region.

extern "C" {
    fn sash_heap_init(heap: *mut u8, size: usize);
    pub fn sash_malloc(n: usize) -> *mut u8;
}

extern "C" {
    /// The heap reservation in bytes. Defined **weakly** in `heap4.c` at
    /// 256 MB; the compiler emits a strong definition when `--heap-size` is
    /// given, and the linker prefers that one. Compiled in rather than read
    /// from the environment on purpose — see the comment in `heap4.c`.
    static sash_heap_size: usize;
}

/// Called from _start before main. mmap anonymous region, init heap4.
///
/// `NORESERVE` is what makes a large default affordable: the kernel commits
/// pages only as they are touched, so RSS still tracks real use and the
/// reservation costs address space alone. The heap is a single contiguous
/// region — heap_4 has one `xStart`/`pxEnd` sentinel pair and cannot span
/// discontiguous mappings — so this reservation is a hard ceiling, not a
/// starting size. Exceeding it is a clean `sash: out of memory`, never a
/// segfault.
#[no_mangle]
pub unsafe extern "C" fn sash_init_heap() {
    let size = sash_heap_size;
    let ptr = rustix::mm::mmap_anonymous(
        core::ptr::null_mut(),
        size,
        rustix::mm::ProtFlags::READ | rustix::mm::ProtFlags::WRITE,
        rustix::mm::MapFlags::PRIVATE | rustix::mm::MapFlags::NORESERVE,
    )
    .expect("sash: mmap failed for heap");
    sash_heap_init(ptr as *mut u8, size);
}

/// DD#32: check SASH_TRACE env var at startup
#[no_mangle]
pub extern "C" fn sash_trace_init() {
    let envp = unsafe { sash_envp };
    if envp.is_null() {
        return;
    }
    let mut i = 0;
    loop {
        let entry = unsafe { *envp.add(i) };
        if entry.is_null() {
            break;
        }
        let prefix = b"SASH_TRACE=";
        let mut matches = true;
        for (j, &b) in prefix.iter().enumerate() {
            if unsafe { *entry.add(j) } != b {
                matches = false;
                break;
            }
        }
        if matches {
            let val = unsafe { *entry.add(prefix.len()) };
            if val != b'0' && val != 0 {
                unsafe {
                    sash_trace_flag = 1;
                }
            }
            return;
        }
        i += 1;
    }
}

#[no_mangle]
pub unsafe extern "C" fn sash_alloc(n: usize) -> *mut u8 {
    let p = sash_malloc(n);
    if p.is_null() {
        sash_panic_oom(n);
    }
    p
}

/// heap_4 returns NULL when the heap cannot satisfy a request. Nothing used
/// to check, so the next write — typically `alloc_cstr`'s NUL terminator —
/// dereferenced NULL and the program died with SIGSEGV, no message, and no
/// status a script could test. Fail loudly instead.
#[cold]
#[inline(never)]
fn sash_panic_oom(n: usize) -> ! {
    let mut buf = [0u8; 128];
    let msg = b"sash: out of memory (requested ";
    let mut pos = msg.len();
    buf[..pos].copy_from_slice(msg);
    pos += write_i64(&mut buf[pos..], n as i64);
    let tail = b" bytes)\n";
    buf[pos..pos + tail.len()].copy_from_slice(tail);
    pos += tail.len();
    diag_write(buf.as_ptr(), pos as i64);
    sash_exit(1);
}

/// DD#87 invariant: every sash **string** buffer is NUL-terminated at `[len]`,
/// so a sash string can cross into an execve/C-string boundary directly (an
/// external command reads argv entries as C strings). Allocates `len + 1`
/// bytes and writes the terminator at `[len]`; the caller fills `[0..len]`.
/// (Non-string allocations — tables, pointer arrays — keep using `sash_alloc`.)
unsafe fn alloc_cstr(len: usize) -> *mut u8 {
    let buf = sash_alloc(len + 1);
    *buf.add(len) = 0;
    buf
}

#[no_mangle]
pub unsafe extern "C" fn sash_free_ptr(ptr: *mut u8) {
    sash_free_c(ptr);
}

extern "C" {
    #[link_name = "sash_free"]
    fn sash_free_c(ptr: *mut u8);
}

// ═══════════════════════════════════════════════════════════════
// _start — the ONLY assembly in the safebash project.
// ═══════════════════════════════════════════════════════════════

extern "C" {
    #[allow(dead_code)]
    fn main() -> i64;
}

/// The startup sequence, called from `_start` on every architecture.
///
/// It exists so the sequence is written **once**. The two `_start` bodies are
/// per-architecture assembly and cannot share code, and while each enumerated
/// the init calls itself they drifted: aarch64 called `sash_init_heap` but not
/// `sash_trace_init`, so `sash_trace_flag` stayed 0 and every trace site
/// compiled in by `--trace` (DD#32) tested false. `sash --trace` on aarch64
/// produced a binary that was larger, slower and silent — a feature reporting
/// success and doing nothing. Adding an init step now means editing this
/// function rather than one assembly block per architecture.
///
/// `sash_argc`/`sash_argv`/`sash_envp` must already be stored: `_start` does
/// that before the call, and `sash_trace_init` reads `sash_envp`.
#[no_mangle]
pub unsafe extern "C" fn sash_startup() {
    unsafe { sash_init_heap() };
    sash_trace_init();
    // DD#120: bind the invoker's own 0/1/2 before any user code can redirect
    // them. Both backends reach this through `_start`, so neither has to
    // emit the call.
    sash_scope_bind();
}

#[cfg(target_arch = "x86_64")]
#[unsafe(naked)]
#[no_mangle]
pub unsafe extern "C" fn _start() -> ! {
    core::arch::naked_asm!(
        "xor rbp, rbp",
        "mov rdi, [rsp]",
        "mov [rip + sash_argc], rdi",
        "lea rsi, [rsp + 8]",
        "mov [rip + sash_argv], rsi",
        "lea rax, [rsp + rdi*8 + 16]",
        "mov [rip + sash_envp], rax",
        "call sash_startup",
        "call main",
        "mov rdi, rax",
        "call sash_shutdown",
    )
}

#[cfg(target_arch = "aarch64")]
#[unsafe(naked)]
#[no_mangle]
pub unsafe extern "C" fn _start() -> ! {
    core::arch::naked_asm!(
        "mov fp, xzr",
        "ldr x0, [sp]",
        "adrp x2, sash_argc",
        "str x0, [x2, :lo12:sash_argc]",
        "add x1, sp, #8",
        "adrp x2, sash_argv",
        "str x1, [x2, :lo12:sash_argv]",
        "add x1, sp, x0, lsl #3",
        "add x1, x1, #16",
        "adrp x2, sash_envp",
        "str x1, [x2, :lo12:sash_envp]",
        "bl sash_startup",
        "bl main",
        "bl sash_shutdown",
    )
}

// ═══════════════════════════════════════════════════════════════
// String operations — thin unsafe wrappers over safe inners
// ═══════════════════════════════════════════════════════════════

/// ${var:-default} — returns 0 if var is empty (use default), 1 if set
#[no_mangle]
pub extern "C" fn sash_default_value(s_len: i64) -> i64 {
    if s_len <= 0 {
        0
    } else {
        1
    }
}

/// Concatenate two strings into a new heap-allocated buffer.
#[no_mangle]
pub unsafe extern "C" fn sash_concat(
    a_ptr: *const u8,
    a_len: i64,
    b_ptr: *const u8,
    b_len: i64,
    out_len: *mut i64,
) -> *mut u8 {
    let total = a_len as usize + b_len as usize;
    let buf = alloc_cstr(total);
    if a_len > 0 && !a_ptr.is_null() {
        core::ptr::copy_nonoverlapping(a_ptr, buf, a_len as usize);
    }
    if b_len > 0 && !b_ptr.is_null() {
        core::ptr::copy_nonoverlapping(b_ptr, buf.add(a_len as usize), b_len as usize);
    }
    *out_len = total as i64;
    buf
}

/// Parse a decimal string to i64.
#[no_mangle]
pub unsafe extern "C" fn sash_atoi(s_ptr: *const u8, s_len: i64) -> i64 {
    if s_len <= 0 || s_ptr.is_null() {
        return 0;
    }
    let bytes = core::slice::from_raw_parts(s_ptr, s_len as usize);
    // DD#25/#26: a runtime string→int overflow traps cleanly
    // (`<prog>: arithmetic overflow`, like arithmetic overflow), NOT a Rust
    // panic ("sash: panic", rc 99). DD#77: `-i`/is_int already range-checks, so
    // a validated `[[ -i $s ]]` value never reaches this trap.
    match atoi_checked(bytes) {
        Some(v) => v,
        None => sash_arith_overflow(),
    }
}

/// DD#77: strict integer-validity test — 1 if the string is a complete integer
/// literal (`[[ -i s ]]` and the arith conversion disposition), else 0. Empty
/// → 0 (false). Format-only (see `is_int_inner`).
#[no_mangle]
pub unsafe extern "C" fn sash_is_int(s_ptr: *const u8, s_len: i64) -> i64 {
    if s_len <= 0 || s_ptr.is_null() {
        return 0;
    }
    let bytes = core::slice::from_raw_parts(s_ptr, s_len as usize);
    is_int_inner(bytes) as i64
}

/// DD#93 Part C: `p?` — 1 if the string is non-empty and every byte is ASCII
/// printable (0x20–0x7E), else 0. Empty → 0 (false). See `is_print_inner`.
#[no_mangle]
pub unsafe extern "C" fn sash_is_print(s_ptr: *const u8, s_len: i64) -> i64 {
    if s_len <= 0 || s_ptr.is_null() {
        return 0;
    }
    let bytes = core::slice::from_raw_parts(s_ptr, s_len as usize);
    is_print_inner(bytes) as i64
}

/// DD#93 Part C: `g?` — 1 if the string is non-empty and every byte is ASCII
/// graph (0x21–0x7E, printable non-space), else 0. Empty → 0 (false).
/// See `is_graph_inner`.
#[no_mangle]
pub unsafe extern "C" fn sash_is_graph(s_ptr: *const u8, s_len: i64) -> i64 {
    if s_len <= 0 || s_ptr.is_null() {
        return 0;
    }
    let bytes = core::slice::from_raw_parts(s_ptr, s_len as usize);
    is_graph_inner(bytes) as i64
}

/// DD#77 arith `:-` disposition: `${s:-N}` in arithmetic. The i64 value of `s`
/// when it is a valid, representable integer; otherwise `default` (the numeric
/// literal N). Not-a-valid-integer unifies unset/empty/non-numeric/out-of-range.
#[no_mangle]
pub unsafe extern "C" fn sash_atoi_or(s_ptr: *const u8, s_len: i64, default: i64) -> i64 {
    if s_len <= 0 || s_ptr.is_null() {
        return default;
    }
    let bytes = core::slice::from_raw_parts(s_ptr, s_len as usize);
    parse_strict_i64(bytes).unwrap_or(default)
}

/// DD#77 arith `:?` disposition: `${s:?MESSAGE}` in arithmetic. The i64 value of
/// `s` when it is a valid, representable integer; otherwise abort
/// (`<prog>: MESSAGE`, exit 1) — the same as a word-context `:?` on unset/empty,
/// extended to the non-integer case.
#[no_mangle]
pub unsafe extern "C" fn sash_atoi_or_abort(
    s_ptr: *const u8,
    s_len: i64,
    msg_ptr: *const u8,
    msg_len: i64,
) -> i64 {
    if s_len > 0 && !s_ptr.is_null() {
        let bytes = core::slice::from_raw_parts(s_ptr, s_len as usize);
        if let Some(v) = parse_strict_i64(bytes) {
            return v;
        }
    }
    sash_require_value(0, msg_ptr, msg_len); // s_len 0 forces the abort
    0 // unreachable (require_value aborts), keeps the type
}

/// Format an i64 as a decimal string into a heap-allocated buffer.
#[no_mangle]
pub unsafe extern "C" fn sash_itoa(val: i64, out_len: *mut i64) -> *mut u8 {
    let mut tmp = [0u8; 21];
    let formatted = itoa_inner(val, &mut tmp);
    let len = formatted.len();
    let buf = alloc_cstr(len);
    core::ptr::copy_nonoverlapping(formatted.as_ptr(), buf, len);
    *out_len = len as i64;
    buf
}

// ═══════════════════════════════════════════════════════════════
// sash_join_args
// ═══════════════════════════════════════════════════════════════

#[no_mangle]
pub unsafe extern "C" fn sash_join_args(
    argv: *const *const u8,
    argc: i64,
    sep_ptr: *const u8,
    sep_len: i64,
    out_len: *mut i64,
) -> *mut u8 {
    let argc = argc as usize;
    let sep_len = sep_len as usize;

    if argc <= 1 {
        *out_len = 0;
        return alloc_cstr(0);
    }

    let mut total = 0usize;
    for i in 1..argc {
        let arg = *argv.add(i);
        if !arg.is_null() {
            total += strlen(arg);
        }
        if i < argc - 1 {
            total += sep_len;
        }
    }

    let buf = alloc_cstr(total);
    let mut pos = 0usize;
    for i in 1..argc {
        let arg = *argv.add(i);
        if !arg.is_null() {
            let len = strlen(arg);
            core::ptr::copy_nonoverlapping(arg, buf.add(pos), len);
            pos += len;
        }
        if i < argc - 1 && sep_len > 0 {
            core::ptr::copy_nonoverlapping(sep_ptr, buf.add(pos), sep_len);
            pos += sep_len;
        }
    }

    *out_len = pos as i64;
    buf
}

/// dup(fd)
#[no_mangle]
pub extern "C" fn sash_dup(fd: i64) -> i64 {
    save_fd(fd, &[])
}

/// A private redirect backup. EBADF means the original descriptor was closed;
/// every other failure must abort rather than silently lose the ability to restore.
#[no_mangle]
pub extern "C" fn sash_save_fd(fd: i64, protected: *const i64, count: i64) -> i64 {
    let protected = unsafe { core::slice::from_raw_parts(protected, count as usize) };
    save_fd(fd, protected)
}

fn save_fd(fd: i64, protected: &[i64]) -> i64 {
    let saved = dup_unreserved(fd, 3, protected);
    if saved < 0 && saved != -9 {
        set_last_errno_raw(-saved);
        fd_backup_abort(b"save fd");
    }
    saved
}

#[no_mangle]
pub extern "C" fn sash_restore_fd(saved: i64, fd: i64) {
    if saved < 0 {
        sash_close(fd);
        return;
    }
    let result = sash_dup2(saved, fd);
    if result < 0 {
        fd_backup_abort(b"restore fd");
    }
    sash_close(saved);
}

fn fd_backup_abort(operation: &[u8]) -> ! {
    sash_emsg(
        operation.as_ptr(),
        operation.len() as i64,
        core::ptr::null(),
        0,
    );
    sash_exit(1)
}

/// Lexicographic string comparison. Returns <0, 0, or >0.
#[no_mangle]
pub extern "C" fn sash_strcmp(p1: *const u8, l1: i64, p2: *const u8, l2: i64) -> i64 {
    let a = unsafe { core::slice::from_raw_parts(p1, l1 as usize) };
    let b = unsafe { core::slice::from_raw_parts(p2, l2 as usize) };
    strcmp_inner(a, b)
}

/// Associative array lookup.
#[no_mangle]
pub extern "C" fn sash_assoc_lookup(
    table: *const i64,
    count: i64,
    key_ptr: *const u8,
    key_len: i64,
    out_len: *mut i64,
) -> *const u8 {
    unsafe {
        let key = core::slice::from_raw_parts(key_ptr, key_len as usize);
        for i in 0..count as usize {
            let base = i * 4;
            let k_ptr = *table.add(base) as *const u8;
            let k_len = *table.add(base + 1);
            let v_ptr = *table.add(base + 2) as *const u8;
            let v_len = *table.add(base + 3);
            let k = core::slice::from_raw_parts(k_ptr, k_len as usize);
            if streq_inner(k, key) {
                *out_len = v_len;
                return v_ptr;
            }
        }
        *out_len = 0;
        core::ptr::null()
    }
}

/// Read one line from fd.
#[unsafe(no_mangle)]
pub extern "C" fn sash_read_line(fd: i64) -> (*mut u8, usize) {
    let mut buf = [0u8; 4096];
    let mut total = 0usize;
    let mut got_newline = false;
    loop {
        if total >= buf.len() {
            break;
        }
        let n = unsafe {
            rustix::io::read(
                rustix::fd::BorrowedFd::borrow_raw(fd as i32),
                &mut buf[total..total + 1],
            )
        };
        match n {
            Ok(0) => break,
            Ok(1) => {
                if buf[total] == b'\n' {
                    got_newline = true;
                    break;
                }
                total += 1;
            }
            _ => break,
        }
    }
    if total == 0 && !got_newline {
        // True EOF
        return (core::ptr::null_mut(), 0);
    }
    // Empty line (got_newline with total=0) or normal line
    let ptr = unsafe { alloc_cstr(total) };
    if total > 0 {
        unsafe {
            core::ptr::copy_nonoverlapping(buf.as_ptr(), ptr, total);
        }
    }
    (ptr, total)
}

/// Redirect fd to a file, path given as (ptr, len).
#[unsafe(no_mangle)]
pub extern "C" fn sash_redirect_s(fd: i64, path_ptr: *const u8, path_len: i64, flags: i64) -> i64 {
    unsafe {
        let len = path_len as usize;
        if len >= 4095 {
            return -1;
        }
        let mut buf: [u8; 4096] = [0; 4096];
        core::ptr::copy_nonoverlapping(path_ptr, buf.as_mut_ptr(), len);
        buf[len] = 0;
        sash_redirect(fd, buf.as_ptr(), flags)
    }
}

// ═══════════════════════════════════════════════════════════════
// sash_splice_argv
// ═══════════════════════════════════════════════════════════════

#[no_mangle]
pub unsafe extern "C" fn sash_splice_argv(
    prefix: *const *const u8,
    argv: *const *const u8,
    argc: i64,
) -> *const *const u8 {
    let mut prefix_count = 0usize;
    loop {
        if (*prefix.add(prefix_count)).is_null() {
            break;
        }
        prefix_count += 1;
    }
    let pos_count = if argc > 1 { (argc - 1) as usize } else { 0 };
    let total = prefix_count + pos_count + 1;
    let buf = sash_alloc(total * 8) as *mut *const u8;
    for i in 0..prefix_count {
        *buf.add(i) = *prefix.add(i);
    }
    for i in 0..pos_count {
        *buf.add(prefix_count + i) = *argv.add(1 + i);
    }
    *buf.add(prefix_count + pos_count) = core::ptr::null();
    buf
}

// ═══════════════════════════════════════════════════════════════
// sash_table — unified hash table (DD#33)
// Safe inner layer operates on slices; unsafe extern "C" wrappers
// convert raw pointers at the boundary.
// ═══════════════════════════════════════════════════════════════

const TABLE_EMPTY: i32 = -1;
const TABLE_DELETED: i32 = -2;

#[repr(C)]
struct TableEntry {
    key_ptr: *const u8,
    key_len: i64,
    val_ptr: *const u8,
    val_len: i64,
    deleted: bool,
}

#[repr(C)]
pub struct SashTable {
    indices: *mut i32,
    entries: *mut TableEntry,
    index_mask: u32,
    entry_count: u32,
    entry_capacity: u32,
    live_count: u32,
}

/// Safe view of the index array for table operations.
/// entries_eq(i, key) returns true if entry i's key matches key.
fn table_find_slot_inner(
    indices: &[i32],
    entries_eq: impl Fn(usize, &[u8]) -> bool,
    mask: u32,
    key: &[u8],
) -> u32 {
    let h = table_hash_inner(key);
    let mut pos = h & mask;
    let mut first_deleted: Option<u32> = None;
    loop {
        let idx = indices[pos as usize];
        if idx == TABLE_EMPTY {
            // Return first deleted slot if we passed one, otherwise this empty slot
            return first_deleted.unwrap_or(pos);
        }
        if idx == TABLE_DELETED {
            if first_deleted.is_none() {
                first_deleted = Some(pos);
            }
        } else if entries_eq(idx as usize, key) {
            return pos;
        }
        pos = (pos + 1) & mask;
    }
}

unsafe fn table_grow(t: *mut SashTable) {
    let old_indices = (*t).indices;
    let new_index_size = ((*t).index_mask + 1) * 2;
    let new_mask = new_index_size - 1;
    let new_indices = sash_alloc((new_index_size as usize) * 4) as *mut i32;
    // Init to EMPTY
    let idx_slice = core::slice::from_raw_parts_mut(new_indices, new_index_size as usize);
    for slot in idx_slice.iter_mut() {
        *slot = TABLE_EMPTY;
    }
    // Compact: copy live entries, skip deleted, rehash
    let mut new_count = 0u32;
    for i in 0..(*t).entry_count as usize {
        let e = &*(*t).entries.add(i);
        if e.deleted {
            continue;
        }
        if new_count as usize != i {
            let dst = &mut *(*t).entries.add(new_count as usize);
            *dst = TableEntry {
                key_ptr: e.key_ptr,
                key_len: e.key_len,
                val_ptr: e.val_ptr,
                val_len: e.val_len,
                deleted: false,
            };
        }
        let key = core::slice::from_raw_parts(e.key_ptr, e.key_len as usize);
        let h = table_hash_inner(key);
        let mut pos = h & new_mask;
        loop {
            if idx_slice[pos as usize] == TABLE_EMPTY {
                idx_slice[pos as usize] = new_count as i32;
                break;
            }
            pos = (pos + 1) & new_mask;
        }
        new_count += 1;
    }
    (*t).indices = new_indices;
    (*t).index_mask = new_mask;
    (*t).entry_count = new_count;
    (*t).live_count = new_count;
    // The old index array is unreachable once `(*t).indices` is replaced.
    // It used to be leaked on every doubling, so a growing table burned the
    // heap at roughly twice its live size and heap_4's coalescing could not
    // reclaim blocks nothing had freed.
    if !old_indices.is_null() {
        sash_free_c(old_indices as *mut u8);
    }
}

#[no_mangle]
pub unsafe extern "C" fn sash_table_new(capacity: i64) -> *mut SashTable {
    let cap = if capacity < 4 {
        4u32
    } else {
        (capacity as u32).next_power_of_two()
    };
    let index_size = (cap * 2).next_power_of_two();
    let t = sash_alloc(core::mem::size_of::<SashTable>()) as *mut SashTable;
    let indices = sash_alloc((index_size as usize) * 4) as *mut i32;
    let idx_slice = core::slice::from_raw_parts_mut(indices, index_size as usize);
    for slot in idx_slice.iter_mut() {
        *slot = TABLE_EMPTY;
    }
    let entries =
        sash_alloc((cap as usize) * core::mem::size_of::<TableEntry>()) as *mut TableEntry;
    (*t).indices = indices;
    (*t).entries = entries;
    (*t).index_mask = index_size - 1;
    (*t).entry_count = 0;
    (*t).entry_capacity = cap;
    (*t).live_count = 0;
    t
}

#[no_mangle]
pub unsafe extern "C" fn sash_table_set(
    t: *mut SashTable,
    key_ptr: *const u8,
    key_len: i64,
    val_ptr: *const u8,
    val_len: i64,
) {
    if (*t).entry_count * 4 >= ((*t).index_mask + 1) * 3 {
        table_grow(t);
    }
    let key = core::slice::from_raw_parts(key_ptr, key_len as usize);
    // NOTE: indices slice must be created AFTER potential grow, which reallocates
    let indices = core::slice::from_raw_parts((*t).indices, ((*t).index_mask + 1) as usize);
    let entries_ptr = (*t).entries;
    let pos = table_find_slot_inner(
        indices,
        |i, k| {
            let e = &*entries_ptr.add(i);
            e.key_len == k.len() as i64
                && core::slice::from_raw_parts(e.key_ptr, e.key_len as usize) == k
        },
        (*t).index_mask,
        key,
    );

    let idx = *(*t).indices.add(pos as usize);
    // `table_find_slot_inner` returns the first DELETED slot in the probe
    // chain when the key is absent, so a negative `idx` here is either
    // TABLE_EMPTY (-1) or TABLE_DELETED (-2) — both mean "not present, insert
    // here". Testing only `!= TABLE_EMPTY` treated the -2 sentinel as a live
    // entry index and wrote val_ptr/val_len through `entries.add(-2 as usize)`.
    if idx >= 0 {
        let e = &mut *(*t).entries.add(idx as usize);
        e.val_ptr = val_ptr;
        e.val_len = val_len;
    } else {
        if (*t).entry_count >= (*t).entry_capacity {
            let new_cap = (*t).entry_capacity * 2;
            let new_entries = sash_alloc((new_cap as usize) * core::mem::size_of::<TableEntry>())
                as *mut TableEntry;
            core::ptr::copy_nonoverlapping((*t).entries, new_entries, (*t).entry_count as usize);
            let old_entries = (*t).entries;
            (*t).entries = new_entries;
            (*t).entry_capacity = new_cap;
            // Freeing the previous generation: it was leaked on every
            // doubling. Entries own no memory themselves — `key_ptr` blocks
            // are copied by pointer into the new array — so only the array
            // itself is released.
            if !old_entries.is_null() {
                sash_free_c(old_entries as *mut u8);
            }
        }
        let ei = (*t).entry_count as usize;
        let e = &mut *(*t).entries.add(ei);
        // Copy key to heap so table owns it (caller's key may be stack-allocated)
        let key_copy = alloc_cstr(key_len as usize);
        core::ptr::copy_nonoverlapping(key_ptr, key_copy, key_len as usize);
        e.key_ptr = key_copy;
        e.key_len = key_len;
        e.val_ptr = val_ptr;
        e.val_len = val_len;
        e.deleted = false;
        *(*t).indices.add(pos as usize) = ei as i32;
        (*t).entry_count += 1;
        (*t).live_count += 1;
    }
}

#[no_mangle]
pub unsafe extern "C" fn sash_table_get(
    t: *const SashTable,
    key_ptr: *const u8,
    key_len: i64,
    out_len: *mut i64,
) -> *const u8 {
    let key = core::slice::from_raw_parts(key_ptr, key_len as usize);
    let indices = core::slice::from_raw_parts((*t).indices, ((*t).index_mask + 1) as usize);
    let entries_ptr = (*t).entries;
    let pos = table_find_slot_inner(
        indices,
        |i, k| {
            let e = &*entries_ptr.add(i);
            e.key_len == k.len() as i64
                && core::slice::from_raw_parts(e.key_ptr, e.key_len as usize) == k
        },
        (*t).index_mask,
        key,
    );

    let idx = indices[pos as usize];
    // Negative is TABLE_EMPTY or TABLE_DELETED — both mean "absent", which
    // reads as the empty string (bash). Testing only `== TABLE_EMPTY` let the
    // -2 sentinel through and dereferenced `entries.add(-2 as usize)`, so any
    // absent key whose probe chain crossed a deleted slot returned a garbage
    // (ptr, len) and the subsequent write faulted with EFAULT.
    if idx < 0 {
        *out_len = 0;
        return core::ptr::null();
    }
    let e = &*(*t).entries.add(idx as usize);
    *out_len = e.val_len;
    e.val_ptr
}

#[no_mangle]
pub unsafe extern "C" fn sash_table_len(t: *const SashTable) -> i64 {
    (*t).live_count as i64
}

/// Reset a table to empty in place, preserving its allocation. Used for
/// the clear-on-entry semantics of `match S P ARR` (DD#63): each match
/// call replaces the array contents, and a no-match leaves it empty.
#[no_mangle]
pub unsafe extern "C" fn sash_table_clear(t: *mut SashTable) {
    let index_size = ((*t).index_mask + 1) as usize;
    let idx = core::slice::from_raw_parts_mut((*t).indices, index_size);
    for slot in idx.iter_mut() {
        *slot = TABLE_EMPTY;
    }
    (*t).entry_count = 0;
    (*t).live_count = 0;
}

#[no_mangle]
pub unsafe extern "C" fn sash_table_delete(
    t: *mut SashTable,
    key_ptr: *const u8,
    key_len: i64,
) -> i64 {
    let key = core::slice::from_raw_parts(key_ptr, key_len as usize);
    let entries_ptr = (*t).entries;
    let indices = core::slice::from_raw_parts((*t).indices, ((*t).index_mask + 1) as usize);
    let pos = table_find_slot_inner(
        indices,
        |i, k| {
            let e = &*entries_ptr.add(i);
            !e.deleted
                && e.key_len == k.len() as i64
                && core::slice::from_raw_parts(e.key_ptr, e.key_len as usize) == k
        },
        (*t).index_mask,
        key,
    );

    let idx = *(*t).indices.add(pos as usize);
    if idx == TABLE_EMPTY || idx == TABLE_DELETED {
        return 1; // not found
    }
    let e = &mut *(*t).entries.add(idx as usize);
    e.deleted = true;
    *(*t).indices.add(pos as usize) = TABLE_DELETED;
    (*t).live_count -= 1;
    0 // success
}

#[no_mangle]
pub unsafe extern "C" fn sash_table_key(
    t: *const SashTable,
    index: i64,
    out_len: *mut i64,
) -> *const u8 {
    // index is a "live index" — skip deleted entries
    let mut live = 0i64;
    for i in 0..(*t).entry_count as usize {
        let e = &*(*t).entries.add(i);
        if e.deleted {
            continue;
        }
        if live == index {
            *out_len = e.key_len;
            return e.key_ptr;
        }
        live += 1;
    }
    *out_len = 0;
    core::ptr::null()
}

#[no_mangle]
pub unsafe extern "C" fn sash_table_value(
    t: *const SashTable,
    index: i64,
    out_len: *mut i64,
) -> *const u8 {
    let mut live = 0i64;
    for i in 0..(*t).entry_count as usize {
        let e = &*(*t).entries.add(i);
        if e.deleted {
            continue;
        }
        if live == index {
            // DD#76 stores an int cell as {val_ptr: null, val_len: <the i64>}.
            // Keyed reads (`${a[0]}`, arithmetic) go through sash_table_get,
            // where codegen knows the static element type and itoa's itself.
            // This is the POSITIONAL accessor — splat, join, for-in — which
            // reads cells generically with no type to consult, so it renders
            // the integer here. Without this the caller receives a null
            // pointer with a non-zero length and faults on the first write.
            // An empty string element is stored as ("", 0) — a real pointer —
            // so a null val_ptr is an unambiguous int-cell discriminator.
            if e.val_ptr.is_null() {
                return sash_itoa(e.val_len, out_len) as *const u8;
            }
            *out_len = e.val_len;
            return e.val_ptr;
        }
        live += 1;
    }
    *out_len = 0;
    core::ptr::null()
}

unsafe fn table_entry_text<'a>(entry: &'a TableEntry, integer: &'a mut [u8; 21]) -> &'a [u8] {
    if entry.val_ptr.is_null() {
        itoa_inner(entry.val_len, integer)
    } else {
        core::slice::from_raw_parts(entry.val_ptr, entry.val_len as usize)
    }
}

/// Join live values in insertion order into one owned, NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn sash_table_join(
    t: *const SashTable,
    sep_ptr: *const u8,
    sep_len: i64,
    out_len: *mut i64,
) -> *mut u8 {
    let entries = core::slice::from_raw_parts((*t).entries, (*t).entry_count as usize);
    let mut integer = [0; 21];
    let mut total = 0usize;
    let mut separator = 0usize;
    for entry in entries.iter().filter(|entry| !entry.deleted) {
        let value = table_entry_text(entry, &mut integer);
        total = total
            .checked_add(separator)
            .and_then(|n| n.checked_add(value.len()))
            .filter(|&n| n < isize::MAX as usize)
            .unwrap_or_else(|| sash_panic_oom(isize::MAX as usize));
        separator = sep_len as usize;
    }

    let buf = alloc_cstr(total);
    let mut pos = 0usize;
    let mut first = true;
    for entry in entries.iter().filter(|entry| !entry.deleted) {
        if !first && sep_len > 0 {
            core::ptr::copy_nonoverlapping(sep_ptr, buf.add(pos), sep_len as usize);
            pos += sep_len as usize;
        }
        let value = table_entry_text(entry, &mut integer);
        core::ptr::copy_nonoverlapping(value.as_ptr(), buf.add(pos), value.len());
        pos += value.len();
        first = false;
    }
    *out_len = total as i64;
    buf
}

// ── C-callable memory wrappers for Plan 9 regexp ───────────────────────
// Route to heap4 via sash_alloc/sash_free_ptr.

#[no_mangle]
pub unsafe extern "C" fn sash_cmalloc(n: usize) -> *mut u8 {
    sash_alloc(n)
}

#[no_mangle]
pub unsafe extern "C" fn sash_cfree(p: *mut u8) {
    sash_free_ptr(p);
}

#[no_mangle]
pub unsafe extern "C" fn sash_crealloc(p: *mut u8, n: usize) -> *mut u8 {
    sash_realloc(p, n)
}

extern "C" {
    fn sash_realloc(p: *mut u8, n: usize) -> *mut u8;
}

// ── Plan 9 regexp error handler ────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn regerror9(s: *const u8) {
    // Write "<argv0>: regexp error: <msg>\n" to stderr (DD#55: diagnostics
    // are prefixed with the program name, argv[0], for consistency).
    let sep: &[u8] = b": ";
    let argv = sash_argv;
    if !argv.is_null() {
        let a0 = *argv;
        if !a0.is_null() {
            let l = strlen(a0);
            diag_write(a0, l as i64);
        }
    }
    diag_write(sep.as_ptr(), sep.len() as i64);
    let prefix = b"regexp error: ";
    diag_write(prefix.as_ptr(), prefix.len() as i64);
    if !s.is_null() {
        let len = strlen(s);
        diag_write(s, len as i64);
    }
    diag_write(b"\n".as_ptr(), 1);
}

// ── Regex match wrapper ────────────────────────────────────────────────

extern "C" {
    fn regcomp9(pattern: *const u8) -> *mut u8;
    fn regexec9(prog: *mut u8, str: *const u8, match_: *mut u8, ms: i32) -> i32;
}

/// Match a string against a regex pattern.

/// Returns 1 if matched, 0 if not.
/// pattern and str must be null-terminated.
#[no_mangle]
pub unsafe extern "C" fn sash_regex_match(
    str_ptr: *const u8,
    str_len: i64,
    pat_ptr: *const u8,
    pat_len: i64,
) -> i64 {
    // Need null-terminated copies for Plan 9 regex API
    let pat_buf = sash_alloc((pat_len + 1) as usize);
    core::ptr::copy_nonoverlapping(pat_ptr, pat_buf, pat_len as usize);
    *pat_buf.add(pat_len as usize) = 0;

    let str_buf = sash_alloc((str_len + 1) as usize);
    core::ptr::copy_nonoverlapping(str_ptr, str_buf, str_len as usize);
    *str_buf.add(str_len as usize) = 0;

    let prog = regcomp9(pat_buf);
    if prog.is_null() {
        // Invalid regex is a runtime error — exit with failure
        sash_exit(2);
    }
    // Resub is 2 pointers (sp, ep) per entry = 16 bytes; 10 entries = 160
    // bytes. Over-allocated to 320 (matches sash_regex_replace).
    let mut resub = [0u8; 320];
    let result = regexec9(prog, str_buf, resub.as_mut_ptr(), 10);
    if result > 0 {
        1
    } else {
        0
    }
}

/// DD#63: whole-match extraction for `$(match S P)`.
/// Returns (ptr, len) of `resub[0]` (the whole match) as a slice of the
/// original subject. No match → `(str_ptr, 0)` (empty string). Invalid
/// regex is a runtime error (`sash_exit(2)`), mirroring `sash_regex_match`.
#[no_mangle]
pub unsafe extern "C" fn sash_regex_match_str(
    str_ptr: *const u8,
    str_len: i64,
    pat_ptr: *const u8,
    pat_len: i64,
) -> (*const u8, i64) {
    // Need null-terminated copies for the Plan 9 regex API.
    let pat_buf = sash_alloc((pat_len + 1) as usize);
    core::ptr::copy_nonoverlapping(pat_ptr, pat_buf, pat_len as usize);
    *pat_buf.add(pat_len as usize) = 0;

    let str_buf = sash_alloc((str_len + 1) as usize);
    core::ptr::copy_nonoverlapping(str_ptr, str_buf, str_len as usize);
    *str_buf.add(str_len as usize) = 0;

    let prog = regcomp9(pat_buf);
    if prog.is_null() {
        sash_exit(2);
    }
    // Resub stride is 16 bytes (sp, ep); 10 entries, over-allocated to 320.
    let mut resub = [0u8; 320];
    let matched = regexec9(prog, str_buf, resub.as_mut_ptr(), 10);
    if matched <= 0 {
        return (str_ptr, 0);
    }
    // resub[0] = whole match. Offsets are relative to str_buf; return the
    // matching slice of the caller's original subject.
    let ptrs = resub.as_ptr() as *const *const u8;
    let match_start = *ptrs; // sp
    let match_end = *ptrs.add(1); // ep
    let start_off = match_start as usize - str_buf as usize;
    let end_off = match_end as usize - str_buf as usize;
    (str_ptr.add(start_off), (end_off - start_off) as i64)
}

/// DD#63 captures: the out-array form `match S P ARR`. Fills `table` (the
/// destination array) with the whole match at index 0 and capture groups
/// g1..gN at indices 1..N, where N is the pattern's capturing-group count
/// (bash-parity: array length = N+1). A group that exists in the pattern
/// but did not participate in the match is stored as the empty string.
/// The table is cleared on entry, so a no-match leaves it empty. Returns 1
/// if matched, 0 if not. Invalid regex aborts (`sash_exit(2)`), mirroring
/// `sash_regex_match`.
#[no_mangle]
pub unsafe extern "C" fn sash_regex_match_into(
    str_ptr: *const u8,
    str_len: i64,
    pat_ptr: *const u8,
    pat_len: i64,
    table: *mut SashTable,
) -> i64 {
    sash_table_clear(table); // clear-on-entry (DD#63)

    let pat_buf = sash_alloc((pat_len + 1) as usize);
    core::ptr::copy_nonoverlapping(pat_ptr, pat_buf, pat_len as usize);
    *pat_buf.add(pat_len as usize) = 0;

    let str_buf = sash_alloc((str_len + 1) as usize);
    core::ptr::copy_nonoverlapping(str_ptr, str_buf, str_len as usize);
    *str_buf.add(str_len as usize) = 0;

    let prog = regcomp9(pat_buf);
    if prog.is_null() {
        sash_exit(2);
    }
    // Reprog.nsub (the capturing-group count) sits just past the leading
    // `startinst` pointer in the struct (sash DD#63 engine field).
    let nsub = (*(prog.add(core::mem::size_of::<*const u8>()) as *const i32)) as usize;
    // The engine reports at most NSUBEXP (128) submatches; size the Resub
    // buffer to whole + N groups so every group is surfaced (no 9-group cap).
    let ngroups = if nsub > 127 { 127 } else { nsub };
    let ms = ngroups + 1;
    let resub = sash_alloc(ms * 16); // 16 bytes/entry (sp, ep)
    core::ptr::write_bytes(resub, 0, ms * 16);

    let matched = regexec9(prog, str_buf, resub, ms as i32);
    if matched <= 0 {
        return 0; // table already cleared → empty
    }
    let ptrs = resub as *const *const u8;
    for i in 0..=ngroups {
        let sp = *ptrs.add(2 * i);
        let ep = *ptrs.add(2 * i + 1);
        let (vptr, vlen): (*const u8, i64) = if sp.is_null() {
            (b"".as_ptr(), 0) // non-participating group → empty string
        } else {
            let start_off = sp as usize - str_buf as usize;
            let len = ep as usize - sp as usize;
            // Copy the captured slice to heap so the array owns its data
            // (the subject may be a temporary that outlives this call).
            let cp = alloc_cstr(len);
            core::ptr::copy_nonoverlapping(str_ptr.add(start_off), cp, len);
            (cp, len as i64)
        };
        let key = (i as i64).to_ne_bytes();
        sash_table_set(table, key.as_ptr(), 8, vptr, vlen);
    }
    1
}

/// DD#63 `matchall S P ARR`: every match, not just the first.
///
/// Appends each match's CAPTURE GROUPS, or the WHOLE match when the pattern
/// has no groups. That one rule serves both shapes the DD#99 migration needs:
/// a group-less pattern yields the matched spans (lines), and an N-group
/// pattern yields `g1..gN` per match flattened, which is exactly the pairwise
/// array `map m = @{a:DISP}` consumes. The whole match is deliberately NOT
/// prepended when there are groups — it would break the pairing.
///
/// Returns 1 if at least one match was found, 0 otherwise. The table is
/// cleared on entry (DD#63), so the no-match path leaves it empty.
///
/// # Safety
/// `str_ptr`/`pat_ptr` must be valid for their lengths; `table` must be a
/// live `SashTable`.
#[no_mangle]
pub unsafe extern "C" fn sash_regex_matchall_into(
    str_ptr: *const u8,
    str_len: i64,
    pat_ptr: *const u8,
    pat_len: i64,
    table: *mut SashTable,
) -> i64 {
    sash_table_clear(table);

    let pat_buf = sash_alloc((pat_len + 1) as usize);
    core::ptr::copy_nonoverlapping(pat_ptr, pat_buf, pat_len as usize);
    *pat_buf.add(pat_len as usize) = 0;

    let str_buf = sash_alloc((str_len + 1) as usize);
    core::ptr::copy_nonoverlapping(str_ptr, str_buf, str_len as usize);
    *str_buf.add(str_len as usize) = 0;

    let prog = regcomp9(pat_buf);
    if prog.is_null() {
        sash_exit(2);
    }
    let nsub = (*(prog.add(core::mem::size_of::<*const u8>()) as *const i32)) as usize;
    let ngroups = if nsub > 127 { 127 } else { nsub };
    let ms = ngroups + 1;
    let resub = sash_alloc(ms * 16);

    let mut slot: i64 = 0;
    let mut off: usize = 0;
    let total = str_len as usize;
    while off <= total {
        core::ptr::write_bytes(resub, 0, ms * 16);
        if regexec9(prog, str_buf.add(off), resub, ms as i32) <= 0 {
            break;
        }
        let ptrs = resub as *const *const u8;
        let whole_sp = *ptrs;
        let whole_ep = *ptrs.add(1);
        if whole_sp.is_null() {
            break;
        }
        // With groups, emit the groups; without, the whole match.
        let first = if ngroups == 0 { 0 } else { 1 };
        for i in first..=ngroups {
            let sp = *ptrs.add(2 * i);
            let ep = *ptrs.add(2 * i + 1);
            let (vptr, vlen): (*const u8, i64) = if sp.is_null() {
                (b"".as_ptr(), 0)
            } else {
                let len = ep as usize - sp as usize;
                let cp = alloc_cstr(len);
                core::ptr::copy_nonoverlapping(sp, cp, len);
                (cp, len as i64)
            };
            let key = slot.to_ne_bytes();
            sash_table_set(table, key.as_ptr(), 8, vptr, vlen);
            slot += 1;
        }
        // Advance past this match. A zero-width match would otherwise spin
        // forever on the same offset, so step one byte in that case.
        let end = whole_ep as usize - str_buf as usize;
        let start = whole_sp as usize - str_buf as usize;
        off = if end > start { end } else { end + 1 };
    }
    i64::from(slot > 0)
}

/// DD#63: expand a `sub`/`gsub` replacement template against the current
/// match's `Resub`. `\0` = whole match, `\1`..`\9` = capture groups, `\\`
/// = a literal backslash. `\` + a non-digit is rejected at compile time
/// (SB4033), so it should not reach here; defensively the two bytes are
/// kept verbatim. A backref to a group that did not participate (null
/// span) expands to nothing. Returns a heap buffer (ptr, len) the caller
/// then copies into the result.
unsafe fn expand_replacement(rep: &[u8], resub: *const *const u8) -> (*mut u8, usize) {
    let group_len = |d: usize| -> usize {
        let sp = *resub.add(2 * d);
        let ep = *resub.add(2 * d + 1);
        if sp.is_null() {
            0
        } else {
            ep as usize - sp as usize
        }
    };
    // Pass 1: total expanded length.
    let mut out_len = 0usize;
    let mut i = 0;
    while i < rep.len() {
        if rep[i] == b'\\' && i + 1 < rep.len() {
            let c = rep[i + 1];
            if c.is_ascii_digit() {
                out_len += group_len((c - b'0') as usize);
            } else if c == b'\\' {
                out_len += 1;
            } else {
                out_len += 2;
            }
            i += 2;
        } else {
            out_len += 1;
            i += 1;
        }
    }
    let out = alloc_cstr(out_len);
    // Pass 2: fill.
    let mut w = 0usize;
    i = 0;
    while i < rep.len() {
        if rep[i] == b'\\' && i + 1 < rep.len() {
            let c = rep[i + 1];
            if c.is_ascii_digit() {
                let d = (c - b'0') as usize;
                let sp = *resub.add(2 * d);
                let glen = group_len(d);
                if glen > 0 {
                    core::ptr::copy_nonoverlapping(sp, out.add(w), glen);
                    w += glen;
                }
            } else if c == b'\\' {
                *out.add(w) = b'\\';
                w += 1;
            } else {
                *out.add(w) = rep[i];
                *out.add(w + 1) = rep[i + 1];
                w += 2;
            }
            i += 2;
        } else {
            *out.add(w) = rep[i];
            w += 1;
            i += 1;
        }
    }
    (out, w)
}

/// Regex substitution: s/pattern/replacement/flags
/// flags: 'g' for global replace
/// Returns new string (ptr, len). Caller owns the memory.
#[no_mangle]
pub unsafe extern "C" fn sash_regex_replace(
    str_ptr: *const u8,
    str_len: i64,
    pat_ptr: *const u8,
    pat_len: i64,
    rep_ptr: *const u8,
    rep_len: i64,
    global: i64,
) -> (*mut u8, i64) {
    let src = core::slice::from_raw_parts(str_ptr, str_len as usize);
    let rep = core::slice::from_raw_parts(rep_ptr, rep_len as usize);

    // Null-terminate pattern for Plan 9 API
    let pat_buf = sash_alloc((pat_len + 1) as usize);
    core::ptr::copy_nonoverlapping(pat_ptr, pat_buf, pat_len as usize);
    *pat_buf.add(pat_len as usize) = 0;

    let prog = regcomp9(pat_buf);
    if prog.is_null() {
        // No match possible — return copy of original
        let out = alloc_cstr(str_len as usize);
        core::ptr::copy_nonoverlapping(str_ptr, out, str_len as usize);
        return (out, str_len);
    }

    // Build result incrementally
    let mut result = sash_alloc(str_len as usize + 256);
    let mut result_len: usize = 0;
    let mut result_cap: usize = str_len as usize + 256;
    let mut offset: usize = 0;

    loop {
        if offset >= src.len() {
            break;
        }

        // Null-terminate the remaining string
        let remaining_len = src.len() - offset;
        let str_buf = sash_alloc(remaining_len + 1);
        core::ptr::copy_nonoverlapping(src.as_ptr().add(offset), str_buf, remaining_len);
        *str_buf.add(remaining_len) = 0;

        // resub: 10 submatches, each is (start_ptr, end_ptr) = 2 pointers = 16 bytes
        let mut resub = [0u8; 320];
        let matched = regexec9(prog, str_buf, resub.as_mut_ptr(), 10);
        if matched <= 0 {
            // No match — copy remainder
            let needed = result_len + remaining_len;
            if needed > result_cap {
                result_cap = needed * 2;
                let new_buf = sash_alloc(result_cap);
                core::ptr::copy_nonoverlapping(result, new_buf, result_len);
                result = new_buf;
            }
            core::ptr::copy_nonoverlapping(
                src.as_ptr().add(offset),
                result.add(result_len),
                remaining_len,
            );
            result_len += remaining_len;
            break;
        }

        // Extract match start/end from resub[0] (first submatch = whole match)
        let ptrs = resub.as_ptr() as *const *const u8;
        let match_start = *ptrs; // sp
        let match_end = *ptrs.add(1); // ep
        let match_start_off = match_start as usize - str_buf as usize;
        let match_end_off = match_end as usize - str_buf as usize;

        // Expand the replacement template (\0-\9 backrefs, \\) against
        // this match's Resub (DD#63), then size the result for pre-match
        // text + the expanded replacement together.
        let ptrs = resub.as_ptr() as *const *const u8;
        let (exp_ptr, exp_len) = expand_replacement(rep, ptrs);

        // Copy pre-match text
        let pre_len = match_start_off;
        let needed = result_len + pre_len + exp_len;
        if needed > result_cap {
            result_cap = needed * 2;
            let new_buf = sash_alloc(result_cap);
            core::ptr::copy_nonoverlapping(result, new_buf, result_len);
            result = new_buf;
        }
        core::ptr::copy_nonoverlapping(src.as_ptr().add(offset), result.add(result_len), pre_len);
        result_len += pre_len;

        // Copy expanded replacement
        core::ptr::copy_nonoverlapping(exp_ptr, result.add(result_len), exp_len);
        result_len += exp_len;

        offset += match_end_off;
        // Prevent infinite loop on zero-length match
        if match_end_off == match_start_off {
            if offset < src.len() {
                // Copy one byte and advance
                if result_len + 1 > result_cap {
                    result_cap = (result_len + 1) * 2;
                    let new_buf = sash_alloc(result_cap);
                    core::ptr::copy_nonoverlapping(result, new_buf, result_len);
                    result = new_buf;
                }
                *result.add(result_len) = src[offset];
                result_len += 1;
                offset += 1;
            } else {
                break;
            }
        }

        if global == 0 {
            // Non-global: copy remainder after first match
            let remaining = src.len() - offset;
            if result_len + remaining > result_cap {
                result_cap = (result_len + remaining) * 2;
                let new_buf = sash_alloc(result_cap);
                core::ptr::copy_nonoverlapping(result, new_buf, result_len);
                result = new_buf;
            }
            core::ptr::copy_nonoverlapping(
                src.as_ptr().add(offset),
                result.add(result_len),
                remaining,
            );
            result_len += remaining;
            break;
        }
    }

    // DD#87 invariant: NUL-terminate the grown result buffer at [result_len]
    // so it is a valid C string at the execve boundary. The grow buffers keep
    // headroom (`*2`, `+256`), but guard the tight case explicitly.
    if result_len == result_cap {
        let grown = alloc_cstr(result_len);
        core::ptr::copy_nonoverlapping(result, grown, result_len);
        return (grown, result_len as i64);
    }
    *result.add(result_len) = 0;
    (result, result_len as i64)
}

// ── File test operations ─────────────────────────────────────

/// File test: returns 1 if the test passes, 0 otherwise.
/// op: 0=exists(-e), 1=isfile(-f), 2=isdir(-d), 3=readable(-r),
///     4=writable(-w), 5=executable(-x), 6=non-empty(-s), 7=symlink(-L)
#[unsafe(no_mangle)]
pub extern "C" fn sash_file_test(path_ptr: *const u8, path_len: i64, op: i64) -> i64 {
    if path_ptr.is_null() || path_len <= 0 {
        return 0;
    }
    let path = unsafe { core::slice::from_raw_parts(path_ptr, path_len as usize) };

    // We need a null-terminated path for syscalls
    let mut buf = [0u8; 4096];
    if path.len() >= buf.len() {
        return 0;
    }
    buf[..path.len()].copy_from_slice(path);
    buf[path.len()] = 0;
    let path_cstr = unsafe { core::ffi::CStr::from_ptr(buf.as_ptr() as *const core::ffi::c_char) };

    match op {
        0 => {
            // -e: exists (stat succeeds)
            match rustix::fs::stat(path_cstr) {
                Ok(_) => 1,
                Err(_) => 0,
            }
        }
        1 => {
            // -f: is regular file
            match rustix::fs::stat(path_cstr) {
                Ok(st) => {
                    if rustix::fs::FileType::from_raw_mode(st.st_mode)
                        == rustix::fs::FileType::RegularFile
                    {
                        1
                    } else {
                        0
                    }
                }
                Err(_) => 0,
            }
        }
        2 => {
            // -d: is directory
            match rustix::fs::stat(path_cstr) {
                Ok(st) => {
                    if rustix::fs::FileType::from_raw_mode(st.st_mode)
                        == rustix::fs::FileType::Directory
                    {
                        1
                    } else {
                        0
                    }
                }
                Err(_) => 0,
            }
        }
        3 => {
            // -r: readable
            rustix::fs::access(path_cstr, rustix::fs::Access::READ_OK).is_ok() as i64
        }
        4 => {
            // -w: writable
            rustix::fs::access(path_cstr, rustix::fs::Access::WRITE_OK).is_ok() as i64
        }
        5 => {
            // -x: executable
            rustix::fs::access(path_cstr, rustix::fs::Access::EXEC_OK).is_ok() as i64
        }
        6 => {
            // -s: non-empty file (size > 0)
            match rustix::fs::stat(path_cstr) {
                Ok(st) => {
                    if st.st_size > 0 {
                        1
                    } else {
                        0
                    }
                }
                Err(_) => 0,
            }
        }
        7 => {
            // -L / -h: is symlink (use lstat)
            match rustix::fs::lstat(path_cstr) {
                Ok(st) => {
                    if rustix::fs::FileType::from_raw_mode(st.st_mode)
                        == rustix::fs::FileType::Symlink
                    {
                        1
                    } else {
                        0
                    }
                }
                Err(_) => 0,
            }
        }
        8 => {
            // -c: is character device
            match rustix::fs::stat(path_cstr) {
                Ok(st) => {
                    if rustix::fs::FileType::from_raw_mode(st.st_mode)
                        == rustix::fs::FileType::CharacterDevice
                    {
                        1
                    } else {
                        0
                    }
                }
                Err(_) => 0,
            }
        }
        9 => {
            // -p: is named pipe (fifo)
            match rustix::fs::stat(path_cstr) {
                Ok(st) => {
                    if rustix::fs::FileType::from_raw_mode(st.st_mode) == rustix::fs::FileType::Fifo
                    {
                        1
                    } else {
                        0
                    }
                }
                Err(_) => 0,
            }
        }
        10 => {
            // -k: sticky bit set
            match rustix::fs::stat(path_cstr) {
                Ok(st) => {
                    if st.st_mode & 0o1000 != 0 {
                        1
                    } else {
                        0
                    }
                }
                Err(_) => 0,
            }
        }
        11 => {
            // -b: block device
            match rustix::fs::stat(path_cstr) {
                Ok(st) => {
                    if rustix::fs::FileType::from_raw_mode(st.st_mode)
                        == rustix::fs::FileType::BlockDevice
                    {
                        1
                    } else {
                        0
                    }
                }
                Err(_) => 0,
            }
        }
        12 => {
            // -g: setgid bit
            match rustix::fs::stat(path_cstr) {
                Ok(st) => {
                    if st.st_mode & 0o2000 != 0 {
                        1
                    } else {
                        0
                    }
                }
                Err(_) => 0,
            }
        }
        13 => {
            // -u: setuid bit
            match rustix::fs::stat(path_cstr) {
                Ok(st) => {
                    if st.st_mode & 0o4000 != 0 {
                        1
                    } else {
                        0
                    }
                }
                Err(_) => 0,
            }
        }
        14 => {
            // -G: owned by effective gid
            match rustix::fs::stat(path_cstr) {
                Ok(st) => {
                    let egid = rustix::process::getegid().as_raw();
                    if st.st_gid == egid {
                        1
                    } else {
                        0
                    }
                }
                Err(_) => 0,
            }
        }
        15 => {
            // -O: owned by effective uid
            match rustix::fs::stat(path_cstr) {
                Ok(st) => {
                    let euid = rustix::process::geteuid().as_raw();
                    if st.st_uid == euid {
                        1
                    } else {
                        0
                    }
                }
                Err(_) => 0,
            }
        }
        16 => {
            // -S: is socket
            match rustix::fs::stat(path_cstr) {
                Ok(st) => {
                    if rustix::fs::FileType::from_raw_mode(st.st_mode)
                        == rustix::fs::FileType::Socket
                    {
                        1
                    } else {
                        0
                    }
                }
                Err(_) => 0,
            }
        }
        17 => {
            // -N: modified since last read. Bash compares WHOLE SECONDS with
            // `>=` (test.c: `st_mtime >= st_atime`) and ignores nanoseconds
            // entirely. Matching that exactly matters twice: a freshly written,
            // never-read file has mtime == atime to the nanosecond — the very
            // case -N exists to report true — and a same-second file whose
            // atime nsec is the larger still reports true in bash. A strict
            // `>` with an nsec tiebreak answered "no" to both.
            match rustix::fs::stat(path_cstr) {
                Ok(st) => {
                    if st.st_mtime >= st.st_atime {
                        1
                    } else {
                        0
                    }
                }
                Err(_) => 0,
            }
        }
        _ => 0,
    }
}

/// Integer power: base ** exp with checked_mul for overflow detection.
/// On overflow, traps (DD#25) — `sash_arith_overflow`. Negative exponent
/// → 0 in integer math.
#[unsafe(no_mangle)]
pub extern "C" fn sash_pow(base: i64, exp: i64) -> i64 {
    if exp < 0 {
        return 0;
    } // negative exponent → 0 in integer math
    if exp == 0 {
        return 1;
    }
    let mut result: i64 = 1;
    for _ in 0..exp {
        match result.checked_mul(base) {
            Some(v) => result = v,
            None => sash_arith_overflow(), // DD#25: overflow traps, not saturates
        }
    }
    result
}

/// Substring: ${var:offset:length}
/// Returns (ptr, len) of the substring. Negative offset counts from end.
/// If length is 0 (not provided), returns from offset to end.
#[unsafe(no_mangle)]
pub extern "C" fn sash_substring(
    s_ptr: *const u8,
    s_len: i64,
    offset: i64,
    length: i64,
    has_length: i64,
) -> (*const u8, i64) {
    let total = s_len as usize;
    // Resolve offset (negative = from end)
    let start = if offset < 0 {
        let from_end = (-offset) as usize;
        if from_end > total {
            0
        } else {
            total - from_end
        }
    } else {
        let o = offset as usize;
        if o > total {
            total
        } else {
            o
        }
    };

    let remaining = total - start;
    let len = if has_length != 0 {
        if length < 0 {
            // Negative length: remove from end
            let trim = (-length) as usize;
            if trim >= remaining {
                0
            } else {
                remaining - trim
            }
        } else {
            let l = length as usize;
            if l > remaining {
                remaining
            } else {
                l
            }
        }
    } else {
        remaining
    };

    // DD#87 invariant: a substring must be NUL-terminated at its own length,
    // but a slice into the subject is terminated only when it reaches the
    // subject's end. Copy into a NUL-terminated buffer so it is a valid C
    // string at the execve boundary (a partial `${s:0:2}` otherwise reads on
    // into the subject for an external command).
    let start_ptr = unsafe { s_ptr.add(start) };
    let out = unsafe { alloc_cstr(len) };
    if len > 0 {
        unsafe { core::ptr::copy_nonoverlapping(start_ptr, out, len) };
    }
    (out as *const u8, len as i64)
}

/// Compare file modification times: returns 1 if file1 is newer than file2.
#[unsafe(no_mangle)]
pub extern "C" fn sash_file_newer(
    p1_ptr: *const u8,
    p1_len: i64,
    p2_ptr: *const u8,
    p2_len: i64,
) -> i64 {
    let get_mtime = |ptr: *const u8, len: i64| -> Option<i64> {
        if ptr.is_null() || len <= 0 {
            return None;
        }
        let path = unsafe { core::slice::from_raw_parts(ptr, len as usize) };
        let mut buf = [0u8; 4096];
        if path.len() >= buf.len() {
            return None;
        }
        buf[..path.len()].copy_from_slice(path);
        buf[path.len()] = 0;
        let cstr = unsafe { core::ffi::CStr::from_ptr(buf.as_ptr() as *const core::ffi::c_char) };
        match rustix::fs::stat(cstr) {
            Ok(st) => Some(st.st_mtime as i64 * 1_000_000_000 + st.st_mtime_nsec as i64),
            Err(_) => None,
        }
    };
    match (get_mtime(p1_ptr, p1_len), get_mtime(p2_ptr, p2_len)) {
        (Some(t1), Some(t2)) => {
            if t1 > t2 {
                1
            } else {
                0
            }
        }
        (Some(_), None) => 1, // file1 exists, file2 doesn't → file1 newer
        _ => 0,
    }
}

/// Compare files by device and inode: returns 1 if same device+inode.
/// Implements [[ file1 -ef file2 ]].
#[unsafe(no_mangle)]
pub extern "C" fn sash_file_same_dev(
    p1_ptr: *const u8,
    p1_len: i64,
    p2_ptr: *const u8,
    p2_len: i64,
) -> i64 {
    let get_dev_ino = |ptr: *const u8, len: i64| -> Option<(u64, u64)> {
        if ptr.is_null() || len <= 0 {
            return None;
        }
        let path = unsafe { core::slice::from_raw_parts(ptr, len as usize) };
        let mut buf = [0u8; 4096];
        if path.len() >= buf.len() {
            return None;
        }
        buf[..path.len()].copy_from_slice(path);
        buf[path.len()] = 0;
        let cstr = unsafe { core::ffi::CStr::from_ptr(buf.as_ptr() as *const core::ffi::c_char) };
        match rustix::fs::stat(cstr) {
            Ok(st) => Some((st.st_dev, st.st_ino)),
            Err(_) => None,
        }
    };
    match (get_dev_ino(p1_ptr, p1_len), get_dev_ino(p2_ptr, p2_len)) {
        (Some((d1, i1)), Some((d2, i2))) => {
            if d1 == d2 && i1 == i2 {
                1
            } else {
                0
            }
        }
        _ => 0,
    }
}

/// Change directory. Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn sash_chdir(path_ptr: *const u8, path_len: i64) -> i64 {
    if path_ptr.is_null() || path_len <= 0 {
        return -1;
    }
    let path = unsafe { core::slice::from_raw_parts(path_ptr, path_len as usize) };
    let mut buf = [0u8; 4096];
    if path.len() >= buf.len() {
        return -1;
    }
    buf[..path.len()].copy_from_slice(path);
    buf[path.len()] = 0;
    let path_cstr = unsafe { core::ffi::CStr::from_ptr(buf.as_ptr() as *const core::ffi::c_char) };
    match rustix::process::chdir(path_cstr) {
        Ok(()) => 0,
        Err(e) => {
            set_last_errno(e);
            -1
        }
    }
}

/// Get current working directory. Returns (ptr, len).
/// Caller must not free — uses a static buffer.
#[unsafe(no_mangle)]
pub extern "C" fn sash_getcwd(out_len: *mut i64) -> *const u8 {
    static mut CWD_BUF: [u8; 4096] = [0u8; 4096];
    unsafe {
        // Use raw syscall since rustix::process::getcwd wants a Vec
        let ret: i64;
        core::arch::asm!(
            "syscall",
            in("rax") 79_i64,  // __NR_getcwd
            in("rdi") CWD_BUF.as_mut_ptr(),
            in("rsi") 4096_i64,
            lateout("rax") ret,
            lateout("rcx") _,
            lateout("r11") _,
        );
        if ret < 0 {
            // Record errno so a caller that aborts (DD#73 glob) renders the
            // real reason via sash_emsg.
            SASH_LAST_ERRNO = -ret;
            *out_len = 0;
            return core::ptr::null();
        }
        // ret includes null terminator, we want length without it
        let len = ret - 1;
        *out_len = len;
        CWD_BUF.as_ptr()
    }
}

/// Convert all characters to uppercase. Returns len.
#[unsafe(no_mangle)]
pub extern "C" fn sash_toupper_all(s_ptr: *const u8, s_len: i64, out_ptr: *mut u8) -> i64 {
    if s_ptr.is_null() || s_len <= 0 {
        return 0;
    }
    let s = unsafe { core::slice::from_raw_parts(s_ptr, s_len as usize) };
    let out = unsafe { core::slice::from_raw_parts_mut(out_ptr, s_len as usize) };
    for i in 0..s.len() {
        out[i] = if s[i] >= b'a' && s[i] <= b'z' {
            s[i] - 32
        } else {
            s[i]
        };
    }
    s_len
}

/// Convert all characters to lowercase. Returns len.
#[unsafe(no_mangle)]
pub extern "C" fn sash_tolower_all(s_ptr: *const u8, s_len: i64, out_ptr: *mut u8) -> i64 {
    if s_ptr.is_null() || s_len <= 0 {
        return 0;
    }
    let s = unsafe { core::slice::from_raw_parts(s_ptr, s_len as usize) };
    let out = unsafe { core::slice::from_raw_parts_mut(out_ptr, s_len as usize) };
    for i in 0..s.len() {
        out[i] = if s[i] >= b'A' && s[i] <= b'Z' {
            s[i] + 32
        } else {
            s[i]
        };
    }
    s_len
}

/// Get process ID.
#[unsafe(no_mangle)]
pub extern "C" fn sash_getpid() -> i64 {
    rustix::process::getpid().as_raw_nonzero().get() as i64
}

#[no_mangle]
pub extern "C" fn sash_getppid() -> i64 {
    match rustix::process::getppid() {
        Some(pid) => pid.as_raw_nonzero().get() as i64,
        None => 0, // init process has no parent
    }
}

/// A non-negative 63-bit random integer in `[0, i64::MAX]`, drawn from
/// `getrandom(2)` (the same kernel CSPRNG as `/dev/urandom`). DG
/// (2026-07-07): `$(random)` range. Clearing the sign bit keeps it
/// non-negative and uniform over 63 bits.
#[no_mangle]
pub extern "C" fn sash_random() -> i64 {
    let mut buf = [0u8; 8];
    // 8 bytes is never a short read from the seeded CSPRNG.
    let _ = rustix::rand::getrandom(&mut buf, rustix::rand::GetRandomFlags::empty());
    i64::from_ne_bytes(buf) & i64::MAX
}

/// Current epoch seconds.
///
/// DG (2026-10-10): an int intrinsic returns an **int** from the runtime. It
/// used to return a decimal string `(ptr, len)` and so had its own private
/// itoa, which made it the one `ArithDenotation::Int` name whose int context
/// was a round trip — `int t=$(epochseconds)` rendered the number here only
/// for `sash_atoi` to parse it straight back. The decimal rendering belongs
/// to the one conversion site both backends already share (`sash_itoa`, via
/// `emit_int_to_str`), reached only when the program denotes the conversion.
#[no_mangle]
pub extern "C" fn sash_epochseconds() -> i64 {
    let ts = rustix::time::clock_gettime(rustix::time::ClockId::Realtime);
    ts.tv_sec as i64
}

/// Start capturing stdout: save stdout, point it at an anonymous memory file.
/// Returns (saved_stdout_fd, capture_fd).
///
/// A pipe would deadlock here.  The captured body runs *in-process*, so nothing
/// drains while it writes; past the 64 KiB pipe buffer the writer blocks forever
/// waiting for a reader that cannot start until the body returns.  A `memfd` has
/// no such wall: it is an ordinary file, so writes never block, yet it stays an
/// *fd*, so redirects, `dup2`, and forked children keep working unchanged.
#[no_mangle]
pub extern "C" fn sash_capture_start() -> (i64, i64) {
    let saved = sash_dup(1);
    let cap = match memfd_create(c"sash-capture", MemfdFlags::CLOEXEC) {
        Ok(f) => {
            let raw = f.as_raw_fd() as i64;
            core::mem::forget(f);
            protect_owned_fd(raw)
        }
        Err(e) => {
            set_last_errno(e);
            return (saved, -1);
        }
    };
    // `dup2` makes fd 1 share `cap`'s file description, so the offset `cap` sees
    // also advances for writes made through fd 1 — including a forked child's.
    sash_dup2(cap, 1);
    // Suspend any enclosing buffered capture: fd 1 now really is redirected,
    // so writes belong to this memfd, not to the outer string.
    capture_push(CaptureFrame {
        buffered: false,
        ptr: core::ptr::null_mut(),
        len: 0,
        cap: 0,
    });
    (saved, cap)
}

/// End capturing: restore stdout, read all captured data, return (ptr, len).
/// Strips trailing newlines (bash convention).
#[no_mangle]
pub extern "C" fn sash_capture_end(saved_stdout: i64, cap_fd: i64) -> (*mut u8, usize) {
    // Restore original stdout
    sash_restore_fd(saved_stdout, 1);
    // Resume an enclosing buffered capture, if any.
    capture_pop();

    if cap_fd < 0 {
        return (core::ptr::null_mut(), 0);
    }
    // The body left the offset at end-of-data; rewind before draining.
    let fd = unsafe { BorrowedFd::borrow_raw(cap_fd as i32) };
    if let Err(e) = rustix::fs::seek(fd, rustix::fs::SeekFrom::Start(0)) {
        set_last_errno(e);
    }
    let (ptr, total) = sash_capture_drain(cap_fd);
    sash_close(cap_fd);
    (ptr, total)
}

// ── Signal handling (trap) ───────────────────────────────────

/// DD#56: the compiler validates the signal number (SB4060/SB4061) and the
/// disposition (SB4062) before emitting a registration, so a `rt_sigaction`
/// the kernel rejects means the program is running without a disposition it
/// believes it has. Report it rather than carrying on with the wrong one.
fn signal_registration_failed(signum: i64) -> ! {
    let mut buf = [0u8; 20];
    let n = write_i64(&mut buf, signum);
    unsafe {
        let fd = BorrowedFd::borrow_raw(2);
        let _ = rustix::io::write(fd, b"sash: bug: kernel rejected signal ");
        let _ = rustix::io::write(fd, &buf[..n]);
        let _ = rustix::io::write(fd, b" registration\n");
    }
    sash_shutdown(1)
}

/// Register a signal handler.  handler_fn is a function pointer:
///   extern "C" fn(i32) -> ()
/// Pass 0 for SIG_DFL, 1 for SIG_IGN.
/// Returns 0 on success; aborts on registration failure.
#[no_mangle]
pub extern "C" fn sash_trap(signum: i64, handler: i64) -> i64 {
    use core::mem;

    // sigaction struct for x86_64 Linux (kernel ABI)
    #[repr(C)]
    struct KernelSigaction {
        sa_handler: u64,
        sa_flags: u64,
        sa_restorer: u64,
        sa_mask: [u64; 16], // 128 bytes = 1024 bits
    }

    let mut act: KernelSigaction = unsafe { mem::zeroed() };
    act.sa_handler = handler as u64;
    // SA_RESTORER = 0x04000000, SA_RESTART = 0x10000000
    act.sa_flags = 0x04000000 | 0x10000000;

    act.sa_restorer = sash_flag_restorer as *const () as u64;

    // rt_sigaction syscall (13)
    let ret: i64;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") 13i64,           // __NR_rt_sigaction
            in("rdi") signum,
            in("rsi") &act as *const _ as i64,
            in("rdx") 0i64,           // NULL oldact
            in("r10") 8i64,           // sigsetsize
            lateout("rax") ret,
            lateout("rcx") _,
            lateout("r11") _,
        );
    }
    if ret < 0 {
        signal_registration_failed(signum);
    }
    0
}

// ═══════════════════════════════════════════════════════════════
// sash_signal_flag
// ═══════════════════════════════════════════════════════════════

/// Register FLAG (exiting = 0) or EXIT (exiting = 1).
/// The handler records the disposition at delivery, not at a later checkpoint.
#[no_mangle]
pub extern "C" fn sash_signal_flag(signum_val: i64, exiting: i64) -> i64 {
    let handler = if exiting == 0 {
        sash_flag_handler
    } else {
        sash_exit_handler
    };
    sash_trap(signum_val, handler as *const () as i64)
}

extern "C" fn sash_flag_handler(sig: i32) {
    sash_signum.store(sig as i64, core::sync::atomic::Ordering::Relaxed);
}

extern "C" fn sash_exit_handler(sig: i32) {
    sash_flag_handler(sig);
    sash_exit_signum.store(sig as i64, core::sync::atomic::Ordering::Relaxed);
}

#[unsafe(naked)]
extern "C" fn sash_flag_restorer() {
    core::arch::naked_asm!(
        "mov rax, 15", // __NR_rt_sigreturn
        "syscall",
    );
}

// ── Directory reading (glob support) ─────────────────────────

/// DD#73: lexically normalize an absolute path (`input` must be absolute).
/// Collapses `//`, `.`, and `..` segments textually — NO symlink
/// resolution (lexical, not `realpath`). Writes into `out`, returns length.
fn sash_normalize_abs(input: &[u8], out: &mut [u8; 4096]) -> usize {
    let mut starts = [0usize; 256];
    let mut nseg = 0usize;
    let mut len = 0usize;
    let mut i = 0usize;
    while i < input.len() {
        while i < input.len() && input[i] == b'/' {
            i += 1;
        }
        let seg_start = i;
        while i < input.len() && input[i] != b'/' {
            i += 1;
        }
        let seg = &input[seg_start..i];
        if seg.is_empty() || seg == b"." {
            continue;
        }
        if seg == b".." {
            if nseg > 0 {
                nseg -= 1;
                len = starts[nseg];
            }
            continue;
        }
        if nseg < 256 && len + 1 + seg.len() <= 4096 {
            starts[nseg] = len;
            nseg += 1;
            out[len] = b'/';
            len += 1;
            out[len..len + seg.len()].copy_from_slice(seg);
            len += seg.len();
        }
    }
    if len == 0 {
        out[0] = b'/';
        len = 1;
    }
    len
}

/// DD#73: normalized absolute directory prefix (no trailing slash) for a
/// glob whose directory portion is `dir`. A relative `dir` is resolved
/// against `getcwd()`; a failed `getcwd` aborts through the DD#55 channel,
/// like any syscall failure — NEVER a silent fallback to the bare
/// (relative, injectable) form. Result written into `out`; returns length.
fn sash_glob_abs_prefix(dir: &[u8], out: &mut [u8; 4096]) -> usize {
    let mut raw = [0u8; 4096];
    let mut n = 0usize;
    if dir.first() != Some(&b'/') {
        let mut cwd_len: i64 = 0;
        let cwd_ptr = sash_getcwd(&mut cwd_len);
        if cwd_ptr.is_null() {
            let seg1 = b"glob";
            let seg2 = b"getcwd";
            sash_emsg(
                seg1.as_ptr(),
                seg1.len() as i64,
                seg2.as_ptr(),
                seg2.len() as i64,
            );
            sash_exit(1);
        }
        let cwd = unsafe { core::slice::from_raw_parts(cwd_ptr, cwd_len as usize) };
        let cl = cwd.len().min(4095);
        raw[..cl].copy_from_slice(&cwd[..cl]);
        n = cl;
    }
    // Append the dir portion unless it is a bare "." (already == cwd).
    if dir != b"." {
        if n > 0 && n < 4095 {
            raw[n] = b'/';
            n += 1;
        }
        let dl = dir.len().min(4095 - n);
        raw[n..n + dl].copy_from_slice(&dir[..dl]);
        n += dl;
    }
    sash_normalize_abs(&raw[..n], out)
}

/// Read directory entries matching a star-glob pattern.
/// Pattern supports: "*.ext", "prefix*", "prefix*suffix", "*".
/// Returns a table (indexed array) of matching filenames.
/// dir_ptr/dir_len: directory path (e.g. ".")
/// pat_ptr/pat_len: glob pattern (e.g. "*.sh")
#[no_mangle]
pub extern "C" fn sash_glob(
    dir_ptr: *const u8,
    dir_len: usize,
    pat_ptr: *const u8,
    pat_len: usize,
) -> *mut u8 {
    let dir = unsafe { core::slice::from_raw_parts(dir_ptr, dir_len) };
    let pat = unsafe { core::slice::from_raw_parts(pat_ptr, pat_len) };

    // Parse pattern into prefix/suffix around '*'
    let star_pos = pat.iter().position(|&b| b == b'*');
    let (prefix, suffix) = match star_pos {
        Some(pos) => (&pat[..pos], &pat[pos + 1..]),
        None => (pat, &[] as &[u8]), // no star = exact match
    };

    // Open directory
    let mut dir_path = [0u8; 4096];
    let dlen = dir.len().min(4095);
    dir_path[..dlen].copy_from_slice(&dir[..dlen]);
    dir_path[dlen] = 0;

    // Use raw getdents64 syscall
    let dir_fd: i64;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") 2i64, // __NR_open
            in("rdi") dir_path.as_ptr() as i64,
            in("rsi") 0x10000i64, // O_RDONLY | O_DIRECTORY
            in("rdx") 0i64,
            lateout("rax") dir_fd,
            lateout("rcx") _,
            lateout("r11") _,
        );
    }
    if dir_fd < 0 {
        return unsafe { sash_table_new(16) } as *mut u8;
    }

    let table = unsafe { sash_table_new(16) } as *mut u8;
    let mut buf = [0u8; 4096];
    let mut idx: i64 = 0;
    // DD#73: every element is `<abs-dir>/name`, not the bare dirent name.
    let mut prefix_buf = [0u8; 4096];
    let prefix_len = sash_glob_abs_prefix(dir, &mut prefix_buf);

    loop {
        let nread: i64;
        unsafe {
            core::arch::asm!(
                "syscall",
                in("rax") 217i64, // __NR_getdents64
                in("rdi") dir_fd,
                in("rsi") buf.as_mut_ptr() as i64,
                in("rdx") buf.len() as i64,
                lateout("rax") nread,
                lateout("rcx") _,
                lateout("r11") _,
            );
        }
        if nread <= 0 {
            break;
        }

        let mut pos = 0usize;
        while pos < nread as usize {
            // struct linux_dirent64: d_ino(8), d_off(8), d_reclen(2), d_type(1), d_name(...)
            let reclen = u16::from_ne_bytes([buf[pos + 16], buf[pos + 17]]) as usize;
            let name_start = pos + 19;
            let name_end = buf[name_start..pos + reclen]
                .iter()
                .position(|&b| b == 0)
                .map(|p| name_start + p)
                .unwrap_or(pos + reclen);
            let name = &buf[name_start..name_end];

            // Skip . and ..
            if name != b"." && name != b".." {
                // Match: prefix + * + suffix
                let matches = if star_pos.is_none() {
                    name == pat
                } else {
                    name.len() >= prefix.len() + suffix.len()
                        && name.starts_with(prefix)
                        && name.ends_with(suffix)
                };
                if matches {
                    // Key is raw i64 bytes (same format as codegen IndexedArray)
                    let idx_bytes = idx.to_ne_bytes();
                    // DD#73: store `<prefix>/name` (absolute), not the bare name.
                    let total = prefix_len + 1 + name.len();
                    let elem = unsafe { alloc_cstr(total) };
                    unsafe {
                        core::ptr::copy_nonoverlapping(prefix_buf.as_ptr(), elem, prefix_len);
                        *elem.add(prefix_len) = b'/';
                        core::ptr::copy_nonoverlapping(
                            name.as_ptr(),
                            elem.add(prefix_len + 1),
                            name.len(),
                        );
                    }
                    unsafe {
                        sash_table_set(
                            table as *mut SashTable,
                            idx_bytes.as_ptr(),
                            8,
                            elem,
                            total as i64,
                        );
                    }
                    idx += 1;
                }
            }
            pos += reclen;
        }
    }

    // Close directory
    sash_close(dir_fd);
    table
}

#[allow(dead_code)]

fn itoa_buf(n: i64, buf: &mut [u8; 20]) -> usize {
    if n == 0 {
        buf[0] = b'0';
        return 1;
    }
    let neg = n < 0;
    // Magnitude as u64 so i64::MIN is handled (its i64 negation overflows).
    let mut mag: u64 = n.unsigned_abs();
    let mut pos = 19;
    while mag > 0 {
        buf[pos] = b'0' + (mag % 10) as u8;
        mag /= 10;
        if pos == 0 {
            break;
        }
        pos -= 1;
    }
    if neg {
        buf[pos] = b'-';
    } else {
        pos += 1;
    }
    let len = 20 - pos;
    // Manual copy to avoid memmove
    for j in 0..len {
        buf[j] = buf[pos + j];
    }
    len
}

/// Split a string by a single-byte delimiter and store results into an existing table.
/// Each token becomes an entry with integer index key (0, 1, 2, ...).
///
/// `terminator` selects the delimiter discipline for a trailing delimiter:
///   * `0` — SEPARATOR: a trailing delimiter yields a trailing empty element
///     (`a:b:` → `a`,`b`,``). This is the default whitespace-split behavior.
///   * non-zero — TERMINATOR: a trailing delimiter does NOT create a final
///     empty element (`a:b:` → `a`,`b`), matching bash `mapfile -d ''` and the
///     `find -print0` / `xargs -0` convention. Used for an explicit `read -d`.
/// Interior empty records (`a::b`) are kept in BOTH modes; only a single
/// trailing empty produced by a final delimiter is dropped under terminator
/// semantics.
#[no_mangle]
pub unsafe extern "C" fn sash_split_into_array(
    ptr: *const u8,
    len: usize,
    delim: i64,
    terminator: i64,
    table: *mut u8,
) {
    sash_table_clear(table as *mut SashTable);
    if ptr.is_null() || len == 0 {
        return;
    }
    let data = core::slice::from_raw_parts(ptr, len);
    let d = delim as u8;
    let mut idx: i64 = 0;
    let mut start = 0;
    for i in 0..=len {
        let at_end = i == len;
        let at_delim = if at_end { true } else { data[i] == d };
        if !at_delim {
            continue;
        }
        // Terminator semantics: the input ended exactly on a delimiter, so the
        // final record is empty and elided (bash `mapfile -d ''`).
        if at_end && terminator != 0 && start == len {
            break;
        }
        let token_len = i - start;
        // DD#87 invariant: an element must be NUL-terminated AT ITS OWN LENGTH
        // so it can cross the execve boundary as a C string. A borrowed slice
        // into the source buffer is terminated only where the *buffer* ends,
        // so an external command read past the element to the end of the
        // buffer (`read -a x -d:` on "a:b:c" passed "a:b:c" as element 0).
        // Copy each token into its own NUL-terminated allocation.
        let token_ptr = alloc_cstr(token_len);
        core::ptr::copy_nonoverlapping(ptr.add(start), token_ptr, token_len);
        // Key = 8-byte i64 index (same format as IndexedArray)
        let key_bytes = idx.to_ne_bytes();
        sash_table_set(
            table as *mut SashTable,
            key_bytes.as_ptr(),
            8_i64,
            token_ptr as *const u8,
            token_len as i64,
        );
        idx += 1;
        start = i + 1;
    }
}

/// DD#99: fill `dst` (a map) from `src` (an indexed array) consumed PAIRWISE —
/// element 0 is a key, element 1 its value, and so on. An **odd** count leaves
/// a trailing element with no partner, and `mode` says what to do:
///
/// - 0 `@{a:-KEY}` — the trailing element becomes the VALUE of `KEY`. DG:
///   supplying the key gives a landmark (`${m[KEY]}`); supplying a value would
///   force a scan to find which key holds it.
/// - 1 `@{a:?MSG}` — odd is a failure: abort at runtime with `MSG` (passed in
///   the key slot), exactly as an empty-splat `@{a:?MSG}` does.
/// - 2 `@{a:~}` / `@{a:!}` — discard the trailing element.
///
/// An EMPTY array is a valid empty map (zero is even), so the disposition
/// governs odd count only.
#[no_mangle]
pub unsafe extern "C" fn sash_map_from_array(
    src: *const SashTable,
    dst: *mut u8,
    mode: i64,
    key_ptr: *const u8,
    key_len: i64,
) -> i64 {
    let n = sash_table_len(src);
    let dst = dst as *mut SashTable;
    let mut i: i64 = 0;
    while i + 1 < n {
        let (k_ptr, k_len) = table_index_get(src, i);
        let (v_ptr, v_len) = table_index_get(src, i + 1);
        sash_table_set(dst, k_ptr, k_len, v_ptr, v_len);
        i += 2;
    }
    if i >= n {
        return 0; // even count — nothing owed
    }
    match mode {
        // `:?` — abort at runtime, exactly as an empty-splat `:?` does.
        1 => {
            let argv = sash_argv;
            if !argv.is_null() {
                let a0 = *argv;
                if !a0.is_null() {
                    diag_write(a0, strlen(a0) as i64);
                }
            }
            diag_write(b": ".as_ptr(), 2);
            if !key_ptr.is_null() && key_len > 0 {
                diag_write(key_ptr, key_len);
            }
            diag_write(b"\n".as_ptr(), 1);
            sash_exit(1);
        }
        2 => {} // `:~` / `:!` — discard the trailing element
        _ => {
            // `:-KEY` — the trailing element is the VALUE of the supplied key.
            let (v_ptr, v_len) = table_index_get(src, i);
            sash_table_set(dst, key_ptr, key_len, v_ptr, v_len);
        }
    }
    0
}

/// Element `i` of an indexed array (8-byte i64 keys), as `(ptr, len)`.
unsafe fn table_index_get(t: *const SashTable, i: i64) -> (*const u8, i64) {
    let key = i.to_ne_bytes();
    let mut len: i64 = 0;
    let ptr = sash_table_get(t, key.as_ptr(), 8, &mut len);
    (ptr, len)
}

/// Read fd to EOF into a heap-allocated buffer.
///
/// This used to read into a fixed 64 KiB **stack** buffer and stop there, so
/// anything larger was silently truncated to a prefix and the caller had no
/// way to tell. A read error was likewise indistinguishable from EOF. Both
/// are the same defect: answering plausibly instead of failing. The buffer
/// now grows on the heap, `EINTR` retries, and a real error aborts.
#[no_mangle]
pub extern "C" fn sash_read_all(fd: i64) -> (*mut u8, usize) {
    const INITIAL: usize = 8192;
    let mut cap = INITIAL;
    let mut buf = unsafe { sash_alloc(cap) };
    let mut total = 0usize;
    loop {
        if total == cap {
            let new_cap = cap * 2;
            let grown = unsafe { sash_crealloc(buf, new_cap) };
            if grown.is_null() {
                sash_panic_oom(new_cap);
            }
            buf = grown;
            cap = new_cap;
        }
        let n = unsafe {
            rustix::io::read(
                rustix::fd::BorrowedFd::borrow_raw(fd as i32),
                core::slice::from_raw_parts_mut(buf.add(total), cap - total),
            )
        };
        match n {
            Ok(0) => break,
            Ok(count) => {
                total += count;
            }
            Err(rustix::io::Errno::INTR) => continue,
            Err(_) => sash_panic_read(),
        }
    }
    if total == 0 {
        unsafe { sash_free_c(buf) };
        return (core::ptr::null_mut(), 0);
    }
    // DD#87: the value crosses into string contexts, so it must be
    // NUL-terminated at [len]. Reserve the terminator explicitly.
    if total == cap {
        let grown = unsafe { sash_crealloc(buf, cap + 1) };
        if grown.is_null() {
            sash_panic_oom(cap + 1);
        }
        buf = grown;
    }
    unsafe { *buf.add(total) = 0 };
    (buf, total)
}

/// `$(<file)` — the slurp form. Open, read to EOF, close, strip trailing
/// newlines.
///
/// The whole reason this spelling exists in bash is that it does **not**
/// fork. sash used to desugar it to `$(cat file)` at parse time, which both
/// spawned a process and silently gave the static binary a runtime dependency
/// on `/usr/bin/cat`.
///
/// The strip is delegated to `sash_capture_drain` rather than reimplemented,
/// so the slurp and the general `$( )` capture cannot drift on the
/// trailing-newline rule — they are literally the same code.
///
/// Failure to open **aborts**. The alternative (bash's: warn and yield the
/// empty string) would hand the program a fabricated value; sash's standing
/// rule is to fail loudly rather than answer with something it did not read.
/// That also matches what the `cat` desugar did, so this is not a behaviour
/// change — only the diagnostic improves, naming the file instead of `cat`.
#[unsafe(no_mangle)]
pub extern "C" fn sash_slurp_file(
    path_ptr: *const u8,
    path_len: i64,
    out_len: *mut i64,
) -> *mut u8 {
    let len = path_len as usize;
    if path_ptr.is_null() || len >= 4095 {
        sash_panic_slurp(path_ptr, path_len);
    }
    let mut buf: [u8; 4096] = [0; 4096];
    unsafe { core::ptr::copy_nonoverlapping(path_ptr, buf.as_mut_ptr(), len) };
    buf[len] = 0;
    let cstr = unsafe { core::ffi::CStr::from_ptr(buf.as_ptr() as *const core::ffi::c_char) };
    let opened = match rustix::fs::open(cstr, rustix::fs::OFlags::RDONLY, rustix::fs::Mode::empty())
    {
        Ok(f) => f,
        Err(e) => {
            // `sash_emsg` appends the reason from SASH_LAST_ERRNO, so recording
            // it here is what turns "cannot read file: error 0" into the real
            // "No such file or directory".
            set_last_errno(e);
            sash_panic_slurp(path_ptr, path_len)
        }
    };
    // A directory opens fine with O_RDONLY on Linux; it is `read` that fails,
    // with EISDIR. Catching it here keeps the diagnostic specific ("<path>:
    // cannot read file: Is a directory") instead of falling through to the
    // generic `sash_panic_read`, which knows no path.
    if let Ok(st) = rustix::fs::fstat(&opened) {
        if rustix::fs::FileType::from_raw_mode(st.st_mode) == rustix::fs::FileType::Directory {
            set_last_errno(rustix::io::Errno::ISDIR);
            sash_panic_slurp(path_ptr, path_len);
        }
    }
    let fd = rustix::fd::IntoRawFd::into_raw_fd(opened) as i64;
    let (data, total) = sash_capture_drain(fd);
    sash_close(fd);
    unsafe { *out_len = total as i64 };
    data
}

/// A file named in `$(<file)` that cannot be opened. Named separately from
/// `sash_panic_read` so the message can carry the path.
#[cold]
#[inline(never)]
fn sash_panic_slurp(path_ptr: *const u8, path_len: i64) -> ! {
    let msg = b"cannot read file";
    sash_emsg(path_ptr, path_len, msg.as_ptr(), msg.len() as i64);
    sash_exit(1);
}

/// A failed `read` is not EOF. Reporting it as EOF handed the caller a
/// short value that looked like a complete one.
#[cold]
#[inline(never)]
fn sash_panic_read() -> ! {
    let msg = b"sash: read failed\n";
    diag_write(msg.as_ptr(), msg.len() as i64);
    sash_exit(1);
}
