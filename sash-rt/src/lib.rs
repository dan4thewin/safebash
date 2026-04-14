//! sash-rt: pure Rust runtime for compiled safebash binaries.
//!
//! No C, no std, no libc. Uses `rustix` for all syscalls.
//! The only `asm!` is _start (~8 instructions per arch).

#![no_std]
#![no_builtins]

use rustix::fd::BorrowedFd;

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
fn panic(_info: &core::panic::PanicInfo) -> ! {
    let msg = b"sash: panic\n";
    let fd = unsafe { BorrowedFd::borrow_raw(2) };
    let _ = rustix::io::write(fd, msg);
    rustix::runtime::exit_group(99)
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

// ═══════════════════════════════════════════════════════════════
// sash_write
// ═══════════════════════════════════════════════════════════════

#[no_mangle]
pub extern "C" fn sash_write(fd: i64, buf: *const u8, len: i64) -> i64 {
    let slice = unsafe { core::slice::from_raw_parts(buf, len as usize) };
    let fd = unsafe { BorrowedFd::borrow_raw(fd as i32) };
    match rustix::io::write(fd, slice) {
        Ok(n) => n as i64,
        Err(_) => -1,
    }
}

// ═══════════════════════════════════════════════════════════════
// sash_exit
// ═══════════════════════════════════════════════════════════════

#[no_mangle]
pub extern "C" fn sash_exit(status: i64) -> ! {
    rustix::runtime::exit_group(status as i32)
}

// ═══════════════════════════════════════════════════════════════
// Composable process primitives (Design Decision #14)
//
// Instead of monolithic fork+exec wrappers, we expose:
//   sash_fork()                  → pid (0 in child, >0 in parent)
//   sash_exec(argv, envp)        → no return (execve with PATH)
//   sash_wait(pid)               → exit status
//   sash_redirect(fd, path, flags) → 0 ok, -1 err (open+dup2)
//   sash_dup2(src, dst)          → 0 ok, -1 err
//   sash_pipe(out)               → 0 ok, -1 err (writes [r,w] to out)
//   sash_close(fd)               → 0 ok, -1 err
//   sash_read(fd, buf, len)      → bytes read
//
// Codegen emits the sequence: fork, redirect in child, exec.
// Parent's file descriptors are never touched.
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

unsafe fn find_path(envp: *const *const u8) -> &'static [u8] {
    if envp.is_null() {
        return b"/usr/bin:/bin";
    }
    let mut i = 0;
    loop {
        let entry = *envp.add(i);
        if entry.is_null() {
            break;
        }
        let bytes = cstr_bytes(entry);
        if bytes.len() >= 5
            && bytes[0] == b'P'
            && bytes[1] == b'A'
            && bytes[2] == b'T'
            && bytes[3] == b'H'
            && bytes[4] == b'='
        {
            return &bytes[5..];
        }
        i += 1;
    }
    b"/usr/bin:/bin"
}

/// Fork. Returns child pid in parent, 0 in child, -1 on error.
#[no_mangle]
pub extern "C" fn sash_fork() -> i64 {
    match unsafe { rustix::runtime::kernel_fork() } {
        Ok(rustix::runtime::Fork::Child(_)) => 0,
        Ok(rustix::runtime::Fork::ParentOf(pid)) => pid.as_raw_nonzero().get() as i64,
        Err(_) => -1,
    }
}

/// Exec with PATH lookup. Does not return on success.
/// On failure, calls exit_group(127).
#[no_mangle]
pub extern "C" fn sash_exec(argv: *const *const u8, envp: *const *const u8) -> ! {
    unsafe {
        let cmd = *argv;
        if cmd.is_null() {
            rustix::runtime::exit_group(127);
        }

        let cmd_bytes = cstr_bytes(cmd);
        let has_slash = cmd_bytes.iter().any(|&b| b == b'/');
        let cmd_cstr = core::ffi::CStr::from_ptr(cmd as *const i8);

        if has_slash {
            let _ = rustix::runtime::execve(cmd_cstr, argv, envp);
            rustix::runtime::exit_group(127);
        }

        let path_env = find_path(envp);
        let mut buf = [0u8; 4096];
        let mut pos = 0;
        while pos <= path_env.len() {
            let mut end = pos;
            while end < path_env.len() && path_env[end] != b':' {
                end += 1;
            }
            let dir = &path_env[pos..end];
            pos = end + 1;
            if dir.is_empty() { continue; }
            let total = dir.len() + 1 + cmd_bytes.len() + 1;
            if total > buf.len() { continue; }
            let mut j = 0;
            while j < dir.len() { buf[j] = dir[j]; j += 1; }
            buf[j] = b'/'; j += 1;
            let mut k = 0;
            while k < cmd_bytes.len() { buf[j+k] = cmd_bytes[k]; k += 1; }
            buf[j+k] = 0;
            let path_cstr = core::ffi::CStr::from_ptr(buf.as_ptr() as *const i8);
            if rustix::fs::access(path_cstr, rustix::fs::Access::EXEC_OK).is_ok() {
                let _ = rustix::runtime::execve(path_cstr, argv, envp);
            }
        }
        rustix::runtime::exit_group(127);
    }
}

/// Wait for a child process. Returns exit status, or 127 on error.
#[no_mangle]
pub extern "C" fn sash_wait(pid: i64) -> i64 {
    let pid = unsafe { rustix::process::Pid::from_raw_unchecked(pid as i32) };
    match rustix::process::waitpid(Some(pid), rustix::process::WaitOptions::empty()) {
        Ok(Some((_pid, status))) => {
            if let Some(code) = status.exit_status() {
                return code as i64;
            }
            128
        }
        Ok(None) => 0,
        Err(_) => 127,
    }
}

/// Redirect fd to a file: open(path, flags, 0666) then dup2 to fd.
/// flags: 0 = O_RDONLY, 1 = O_WRONLY|O_CREAT|O_TRUNC, 2 = O_WRONLY|O_CREAT|O_APPEND
/// Returns 0 on success, -1 on error.
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
            _ => return -1,
        };
        let mode = rustix::fs::Mode::from_raw_mode(0o666);
        let opened = match rustix::fs::open(path_cstr, oflags, mode) {
            Ok(f) => f,
            Err(_) => return -1,
        };
        let src = rustix::fd::AsRawFd::as_raw_fd(&opened);
        // dup2(opened, fd)
        core::arch::asm!(
            "syscall",
            in("rax") 33_i64,  // SYS_dup2
            in("rdi") src as i64,
            in("rsi") fd,
            lateout("rcx") _,
            lateout("r11") _,
        );
        // close the original opened fd (now duplicated)
        rustix::io::close(rustix::fd::AsRawFd::as_raw_fd(&opened));
        0
    }
}

/// dup2(src_fd, dst_fd). Returns 0 on success, -1 on error.
#[no_mangle]
pub extern "C" fn sash_dup2(src_fd: i64, dst_fd: i64) -> i64 {
    unsafe {
        let result: i64;
        core::arch::asm!(
            "syscall",
            in("rax") 33_i64,  // SYS_dup2
            in("rdi") src_fd,
            in("rsi") dst_fd,
            lateout("rax") result,
            lateout("rcx") _,
            lateout("r11") _,
        );
        if result < 0 { -1 } else { 0 }
    }
}

/// Create a pipe. Writes [read_fd, write_fd] as two i64s at out_ptr.
/// Returns 0 on success, -1 on error.
#[no_mangle]
pub extern "C" fn sash_pipe(out_ptr: *mut i64) -> i64 {
    match rustix::pipe::pipe() {
        Ok((r, w)) => {
            unsafe {
                let r_raw = rustix::fd::IntoRawFd::into_raw_fd(r) as i64;
                let w_raw = rustix::fd::IntoRawFd::into_raw_fd(w) as i64;
                *out_ptr = r_raw;
                *out_ptr.add(1) = w_raw;
            }
            0
        }
        Err(_) => -1,
    }
}

/// Close a file descriptor. Returns 0 on success, -1 on error.
#[no_mangle]
pub extern "C" fn sash_close(fd: i64) -> i64 {
    unsafe {
        let result: i64;
        core::arch::asm!(
            "syscall",
            in("rax") 3_i64,   // SYS_close
            in("rdi") fd,
            lateout("rax") result,
            lateout("rcx") _,
            lateout("r11") _,
        );
        if result < 0 { -1 } else { 0 }
    }
}

/// Read from a file descriptor. Returns bytes read, 0 on EOF, -1 on error.
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
// Legacy wrappers — sash_run and sash_capture built on primitives
// These remain for backward compatibility with experiments 021–028.
// ═══════════════════════════════════════════════════════════════

/// Execute a command (fork + exec + wait). Returns exit status.
#[no_mangle]
pub extern "C" fn sash_run(argv: *const *const u8, envp: *const *const u8) -> i64 {
    let pid = sash_fork();
    if pid == 0 {
        // child
        sash_exec(argv, envp); // does not return
    } else if pid > 0 {
        sash_wait(pid)
    } else {
        127
    }
}

// ═══════════════════════════════════════════════════════════════
// sash_getenv — look up an environment variable, return (ptr, len)
// ═══════════════════════════════════════════════════════════════

/// Returns a pointer to the value and writes the length to *out_len.
/// Returns null with *out_len = 0 if not found.
#[no_mangle]
pub extern "C" fn sash_getenv(
    name: *const u8,
    name_len: i64,
    out_len: *mut i64,
) -> *const u8 {
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
        // Check if entry starts with "name="
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
// sash_capture — built on composable primitives
// ═══════════════════════════════════════════════════════════════

/// Fork/exec a command, capture stdout into a heap buffer.
/// Returns pointer to buffer, writes length to *out_len.
/// Strips one trailing newline if present (shell convention).
#[no_mangle]
pub extern "C" fn sash_capture(
    argv: *const *const u8,
    envp: *const *const u8,
    out_len: *mut i64,
) -> *const u8 {
    unsafe {
        // Create pipe
        let mut pipe_fds = [0i64; 2];
        if sash_pipe(pipe_fds.as_mut_ptr()) < 0 {
            *out_len = 0;
            return core::ptr::null();
        }
        let pipe_r = pipe_fds[0];
        let pipe_w = pipe_fds[1];

        let pid = sash_fork();
        if pid == 0 {
            // Child: redirect stdout to pipe write end, close read end
            sash_close(pipe_r);
            sash_dup2(pipe_w, 1); // stdout
            sash_close(pipe_w);
            sash_exec(argv, envp); // does not return
        } else if pid > 0 {
            // Parent: close write end, read from read end
            sash_close(pipe_w);

            let mut result = [0u8; 65536]; // 64KB max capture
            let mut total = 0usize;
            loop {
                let n = sash_read(pipe_r, result.as_mut_ptr().add(total), (result.len() - total) as i64);
                if n <= 0 { break; }
                total += n as usize;
            }
            sash_close(pipe_r);
            let _ = sash_wait(pid);

            // Strip trailing newline (shell convention)
            if total > 0 && result[total - 1] == b'\n' {
                total -= 1;
            }

            // Allocate result buffer with null terminator
            let buf = sash_alloc(total + 1);
            core::ptr::copy_nonoverlapping(result.as_ptr(), buf, total);
            *buf.add(total) = 0;
            *out_len = total as i64;
            buf as *const u8
        } else {
            *out_len = 0;
            core::ptr::null()
        }
    }
}

// Simple bump allocator for captured output
static mut HEAP: [u8; 131072] = [0u8; 131072]; // 128KB
static mut HEAP_POS: usize = 0;

#[no_mangle]
pub unsafe extern "C" fn sash_alloc(n: usize) -> *mut u8 {
    let pos = HEAP_POS;
    HEAP_POS += (n + 7) & !7; // align to 8
    HEAP.as_mut_ptr().add(pos)
}

#[no_mangle]
pub extern "C" fn sash_free(_ptr: *const u8) {
    // bump allocator — no-op free
}

// ═══════════════════════════════════════════════════════════════
// _start — the ONLY assembly in the safebash project.
// ═══════════════════════════════════════════════════════════════

extern "C" {
    fn main() -> i64;
}

#[cfg(target_arch = "x86_64")]
#[unsafe(naked)]
#[no_mangle]
pub unsafe extern "C" fn _start() -> ! {
    core::arch::naked_asm!(
        "xor rbp, rbp",
        "mov rdi, [rsp]",           // rdi = argc
        "mov [rip + sash_argc], rdi",
        "lea rsi, [rsp + 8]",       // rsi = argv
        "mov [rip + sash_argv], rsi",
        "lea rax, [rsp + rdi*8 + 16]", // envp = argv + argc + 1
        "mov [rip + sash_envp], rax",
        "call main",
        "mov rdi, rax",
        "mov eax, 60",
        "syscall",
    )
}

#[cfg(target_arch = "aarch64")]
#[unsafe(naked)]
#[no_mangle]
pub unsafe extern "C" fn _start() -> ! {
    core::arch::naked_asm!(
        "mov fp, xzr",
        "ldr x0, [sp]",               // x0 = argc
        "adrp x2, sash_argc",
        "str x0, [x2, :lo12:sash_argc]",
        "add x1, sp, #8",             // x1 = argv
        "adrp x2, sash_argv",
        "str x1, [x2, :lo12:sash_argv]",
        "add x1, sp, x0, lsl #3",     // envp = argv + argc + 1
        "add x1, x1, #16",
        "adrp x2, sash_envp",
        "str x1, [x2, :lo12:sash_envp]",
        "bl main",
        "mov x8, #93",
        "svc #0",
    )
}

// ═══════════════════════════════════════════════════════════════════
/// ${var%%pat} / ${var%pat} — returns new length (ptr unchanged)
#[no_mangle]
pub extern "C" fn sash_strip_suffix(
    s: *const u8, s_len: i64,
    pat: *const u8, pat_len: i64,
    longest: i64,
) -> i64 {
    if s.is_null() || s_len <= 0 { return 0; }
    let s_bytes = unsafe { core::slice::from_raw_parts(s, s_len as usize) };
    let pat_bytes = unsafe { core::slice::from_raw_parts(pat, pat_len as usize) };

    // ${x%%/*} → pat="/*", meaning "/ then anything" → * at end
    // ${x%%*.txt} → pat="*.txt", meaning "anything then .txt" → * at start
    if pat_bytes.ends_with(b"*") && pat_bytes.len() > 1 {
        // "X*" pattern — find X, remove from there to end
        let prefix = &pat_bytes[..pat_bytes.len() - 1]; // literal before *
        if longest != 0 {
            // %% — find first occurrence, keep everything before
            if let Some(pos) = s_bytes.windows(prefix.len()).position(|w| w == prefix) {
                return pos as i64;
            }
        } else {
            // % — find last occurrence, keep everything before
            if let Some(pos) = s_bytes.windows(prefix.len()).rposition(|w| w == prefix) {
                return pos as i64;
            }
        }
    } else if pat_bytes.starts_with(b"*") && pat_bytes.len() > 1 {
        // "*X" pattern — find X, remove from start of match to end
        let suffix = &pat_bytes[1..]; // literal after *
        if longest != 0 {
            // %% — find first occurrence of suffix
            if let Some(pos) = s_bytes.windows(suffix.len()).position(|w| w == suffix) {
                return pos as i64;
            }
        } else {
            // % — find last occurrence of suffix
            if let Some(pos) = s_bytes.windows(suffix.len()).rposition(|w| w == suffix) {
                return pos as i64;
            }
        }
    } else if !pat_bytes.is_empty() && s_bytes.ends_with(pat_bytes) {
        return (s_len - pat_len) as i64;
    }
    s_len // no match
}

/// ${var##pat} / ${var#pat} — returns offset (bytes to skip from start)
#[no_mangle]
pub extern "C" fn sash_strip_prefix(
    s: *const u8, s_len: i64,
    pat: *const u8, pat_len: i64,
    longest: i64,
) -> i64 {
    if s.is_null() || s_len <= 0 { return 0; }
    let s_bytes = unsafe { core::slice::from_raw_parts(s, s_len as usize) };
    let pat_bytes = unsafe { core::slice::from_raw_parts(pat, pat_len as usize) };

    // ${x#*/} → pat="*/", meaning "anything then /" → * at start
    // ${x#foo*} → pat="foo*", meaning "foo then anything" → * at end
    if pat_bytes.starts_with(b"*") && pat_bytes.len() > 1 {
        // "*X" pattern — find X, skip past it
        let suffix = &pat_bytes[1..]; // literal after *
        if longest != 0 {
            // ## — find last occurrence, skip past it
            if let Some(pos) = s_bytes.windows(suffix.len()).rposition(|w| w == suffix) {
                return (pos + suffix.len()) as i64;
            }
        } else {
            // # — find first occurrence, skip past it
            if let Some(pos) = s_bytes.windows(suffix.len()).position(|w| w == suffix) {
                return (pos + suffix.len()) as i64;
            }
        }
    } else if pat_bytes.ends_with(b"*") && pat_bytes.len() > 1 {
        // "X*" pattern — find X, skip past X and everything before
        let prefix = &pat_bytes[..pat_bytes.len() - 1]; // literal before *
        if longest != 0 {
            // ## — find last occurrence, skip past it
            if let Some(pos) = s_bytes.windows(prefix.len()).rposition(|w| w == prefix) {
                return (pos + prefix.len()) as i64;
            }
        } else {
            // # — find first occurrence, skip past it
            if let Some(pos) = s_bytes.windows(prefix.len()).position(|w| w == prefix) {
                return (pos + prefix.len()) as i64;
            }
        }
    } else if !pat_bytes.is_empty() && s_bytes.starts_with(pat_bytes) {
        return pat_len;
    }
    0 // no match — skip nothing
}

/// ${var:-default} — returns 0 if var is empty (use default), 1 if set
#[no_mangle]
pub extern "C" fn sash_default_value(
    s_len: i64,
) -> i64 {
    if s_len <= 0 { 0 } else { 1 }
}
