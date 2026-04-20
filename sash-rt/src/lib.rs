//! sash-rt: pure Rust runtime for compiled safebash binaries.
//!
//! No C, no std, no libc. Uses `rustix` for all syscalls.
//! The only `asm!` is _start (~8 instructions per arch).
//!
//! DD#34: unsafe is confined to boundaries (syscalls, extern "C" entry points,
//! sash_alloc, _start). All pure data manipulation lives in safe inner functions.

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
fn panic(info: &core::panic::PanicInfo) -> ! {
    let fd = unsafe { BorrowedFd::borrow_raw(2) };
    // Try to print the panic message if available
    if let Some(msg) = info.message().as_str() {
        let _ = rustix::io::write(fd, msg.as_bytes());
        let _ = rustix::io::write(fd, b"\n");
    } else {
        let _ = rustix::io::write(fd, b"sash: panic\n");
    }
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
// Safe inner functions — no raw pointers, no unsafe
// Defined in inner.rs, shared with tests via include!
// ═══════════════════════════════════════════════════════════════

include!("inner.rs");

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
            buf[..dir.len()].copy_from_slice(dir);
            buf[dir.len()] = b'/';
            buf[dir.len() + 1..dir.len() + 1 + cmd_bytes.len()].copy_from_slice(cmd_bytes);
            buf[dir.len() + 1 + cmd_bytes.len()] = 0;
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
            _ => return -1,
        };
        let mode = rustix::fs::Mode::from_raw_mode(0o666);
        let opened = match rustix::fs::open(path_cstr, oflags, mode) {
            Ok(f) => f,
            Err(_) => return -1,
        };
        let src = rustix::fd::IntoRawFd::into_raw_fd(opened) as i64;
        core::arch::asm!(
            "syscall",
            in("rax") 33_i64,
            in("rdi") src,
            in("rsi") fd,
            lateout("rcx") _,
            lateout("r11") _,
        );
        sash_close(src);
        0
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
        if result < 0 { -1 } else { 0 }
    }
}

/// Create a pipe.
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
        if result < 0 { -1 } else { 0 }
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
        127
    }
}

// ═══════════════════════════════════════════════════════════════
// sash_getenv
// ═══════════════════════════════════════════════════════════════

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
// sash_capture
// ═══════════════════════════════════════════════════════════════

#[no_mangle]
pub extern "C" fn sash_capture(
    argv: *const *const u8,
    envp: *const *const u8,
    out_len: *mut i64,
) -> *const u8 {
    unsafe {
        let mut pipe_fds = [0i64; 2];
        if sash_pipe(pipe_fds.as_mut_ptr()) < 0 {
            *out_len = 0;
            return core::ptr::null();
        }
        let pipe_r = pipe_fds[0];
        let pipe_w = pipe_fds[1];

        let pid = sash_fork();
        if pid == 0 {
            sash_close(pipe_r);
            sash_dup2(pipe_w, 1);
            sash_close(pipe_w);
            sash_exec(argv, envp);
        } else if pid > 0 {
            sash_close(pipe_w);

            let mut result = [0u8; 65536];
            let mut total = 0usize;
            loop {
                let n = sash_read(pipe_r, result.as_mut_ptr().add(total), (result.len() - total) as i64);
                if n <= 0 { break; }
                total += n as usize;
            }
            sash_close(pipe_r);
            let _ = sash_wait(pid);

            if total > 0 && result[total - 1] == b'\n' {
                total -= 1;
            }

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

// Simple bump allocator — must stay unsafe (DD#34)
// ── Heap — mmap'd region managed by heap_4 ────────────────────────────
// heap4.c provides sash_malloc/sash_free/sash_realloc (first-fit + coalesce).
// We initialize it from _start with an mmap'd anonymous region.

extern "C" {
    fn sash_heap_init(heap: *mut u8, size: usize);
    pub fn sash_malloc(n: usize) -> *mut u8;
}

const HEAP_SIZE: usize = 1024 * 1024; // 1MB, no BSS cost

/// Called from _start before main. mmap anonymous region, init heap4.
#[no_mangle]
pub unsafe extern "C" fn sash_init_heap() {
    let ptr = rustix::mm::mmap_anonymous(
        core::ptr::null_mut(),
        HEAP_SIZE,
        rustix::mm::ProtFlags::READ | rustix::mm::ProtFlags::WRITE,
        rustix::mm::MapFlags::PRIVATE,
    ).expect("sash: mmap failed for heap");
    sash_heap_init(ptr as *mut u8, HEAP_SIZE);
}

#[no_mangle]
pub unsafe extern "C" fn sash_alloc(n: usize) -> *mut u8 {
    sash_malloc(n)
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
    fn main() -> i64;
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
        "call sash_init_heap",
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
        "bl sash_init_heap",
        "bl main",
        "mov x8, #93",
        "svc #0",
    )
}

// ═══════════════════════════════════════════════════════════════
// String operations — thin unsafe wrappers over safe inners
// ═══════════════════════════════════════════════════════════════

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
    strip_suffix_inner(s_bytes, pat_bytes, longest != 0) as i64
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
    strip_prefix_inner(s_bytes, pat_bytes, longest != 0) as i64
}

/// ${var:-default} — returns 0 if var is empty (use default), 1 if set
#[no_mangle]
pub extern "C" fn sash_default_value(s_len: i64) -> i64 {
    if s_len <= 0 { 0 } else { 1 }
}

/// Concatenate two strings into a new heap-allocated buffer.
#[no_mangle]
pub unsafe extern "C" fn sash_concat(
    a_ptr: *const u8, a_len: i64,
    b_ptr: *const u8, b_len: i64,
    out_len: *mut i64,
) -> *mut u8 {
    let total = a_len as usize + b_len as usize;
    let buf = sash_alloc(total);
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
    atoi_inner(bytes)
}

/// Format an i64 as a decimal string into a heap-allocated buffer.
#[no_mangle]
pub unsafe extern "C" fn sash_itoa(val: i64, out_len: *mut i64) -> *mut u8 {
    let mut tmp = [0u8; 21];
    let formatted = itoa_inner(val, &mut tmp);
    let len = formatted.len();
    let buf = sash_alloc(len);
    core::ptr::copy_nonoverlapping(formatted.as_ptr(), buf, len);
    *out_len = len as i64;
    buf
}

/// Evaluate a simple arithmetic expression.
#[no_mangle]
pub extern "C" fn sash_arith_eval(var_val: i64, op: u8, operand: i64) -> i64 {
    arith_eval_inner(var_val, op, operand)
}

// ═══════════════════════════════════════════════════════════════
// sash_setenv
// ═══════════════════════════════════════════════════════════════

#[no_mangle]
pub unsafe extern "C" fn sash_setenv(
    name_ptr: *const u8, name_len: i64,
    val_ptr: *const u8, val_len: i64,
) {
    let nlen = name_len as usize;
    let vlen = val_len as usize;
    let entry_len = nlen + 1 + vlen;
    let entry = sash_alloc(entry_len + 1);
    core::ptr::copy_nonoverlapping(name_ptr, entry, nlen);
    *entry.add(nlen) = b'=';
    core::ptr::copy_nonoverlapping(val_ptr, entry.add(nlen + 1), vlen);
    *entry.add(entry_len) = 0;

    let name_slice = core::slice::from_raw_parts(name_ptr, nlen);

    let envp = sash_envp;
    if envp.is_null() { return; }
    let mut count = 0usize;
    let mut found: Option<usize> = None;
    loop {
        let e = *envp.add(count);
        if e.is_null() { break; }
        let elen = strlen(e);
        let eslice = core::slice::from_raw_parts(e, elen);
        if found.is_none() && elen > nlen && eslice[nlen] == b'=' && &eslice[..nlen] == name_slice {
            found = Some(count);
        }
        count += 1;
    }

    if let Some(idx) = found {
        let envp_mut = envp as *mut *const u8;
        *envp_mut.add(idx) = entry;
    } else {
        let new_envp = sash_alloc((count + 2) * 8) as *mut *const u8;
        core::ptr::copy_nonoverlapping(envp, new_envp, count);
        *new_envp.add(count) = entry;
        *new_envp.add(count + 1) = core::ptr::null();
        sash_envp = new_envp;
    }
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
        return sash_alloc(1);
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

    let buf = sash_alloc(total);
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
    unsafe {
        let result: i64;
        core::arch::asm!(
            "syscall",
            in("rax") 32_i64,
            in("rdi") fd,
            lateout("rax") result,
            lateout("rcx") _,
            lateout("r11") _,
        );
        result
    }
}

/// Compare two length-prefixed strings for equality.
#[no_mangle]
pub extern "C" fn sash_streq(p1: *const u8, l1: i64, p2: *const u8, l2: i64) -> i64 {
    if l1 != l2 { return 0; }
    let a = unsafe { core::slice::from_raw_parts(p1, l1 as usize) };
    let b = unsafe { core::slice::from_raw_parts(p2, l2 as usize) };
    if streq_inner(a, b) { 1 } else { 0 }
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
    loop {
        if total >= buf.len() { break; }
        let n = unsafe { rustix::io::read(rustix::fd::BorrowedFd::borrow_raw(fd as i32), &mut buf[total..total+1]) };
        match n {
            Ok(0) => break,
            Ok(1) => {
                if buf[total] == b'\n' { break; }
                total += 1;
            }
            _ => break,
        }
    }
    if total == 0 {
        return (core::ptr::null_mut(), 0);
    }
    let ptr = unsafe { sash_alloc(total) };
    unsafe { core::ptr::copy_nonoverlapping(buf.as_ptr(), ptr, total); }
    (ptr, total)
}

// ═══════════════════════════════════════════════════════════════
// sash_source_kvp
// ═══════════════════════════════════════════════════════════════

#[no_mangle]
pub unsafe extern "C" fn sash_source_kvp(path: *const u8, path_len: i64) -> i64 {
    let path_slice = core::slice::from_raw_parts(path, path_len as usize);
    let mut path_buf = [0u8; 4096];
    if path_len as usize >= path_buf.len() {
        return 1;
    }
    path_buf[..path_len as usize].copy_from_slice(path_slice);
    path_buf[path_len as usize] = 0;

    let fd: i64;
    core::arch::asm!(
        "syscall",
        in("rax") 2_i64,
        in("rdi") path_buf.as_ptr(),
        in("rsi") 0_i64,
        in("rdx") 0_i64,
        lateout("rax") fd,
        out("rcx") _,
        out("r11") _,
    );
    if fd < 0 {
        return 1;
    }

    let mut buf = [0u8; 65536];
    let mut total = 0usize;
    loop {
        let n = sash_read(fd, buf[total..].as_mut_ptr(), (buf.len() - total) as i64);
        if n <= 0 { break; }
        total += n as usize;
    }
    sash_close(fd);

    let content = &buf[..total];
    let mut pos = 0;
    while pos < total {
        let mut eol = pos;
        while eol < total && content[eol] != b'\n' {
            eol += 1;
        }
        let line = &content[pos..eol];
        pos = eol + 1;

        let trimmed = {
            let mut s = 0;
            while s < line.len() && (line[s] == b' ' || line[s] == b'\t') {
                s += 1;
            }
            &line[s..]
        };
        if trimmed.is_empty() || trimmed[0] == b'#' {
            continue;
        }

        let mut eq = 0;
        while eq < trimmed.len() && trimmed[eq] != b'=' {
            eq += 1;
        }
        if eq >= trimmed.len() {
            sash_write(2, b"sash: bad line in config: ".as_ptr(), 26);
            sash_write(2, trimmed.as_ptr(), trimmed.len() as i64);
            sash_write(2, b"\n".as_ptr(), 1);
            sash_exit(1);
        }

        let key = &trimmed[..eq];
        let val = &trimmed[eq + 1..];
        sash_setenv(key.as_ptr(), key.len() as i64, val.as_ptr(), val.len() as i64);
    }

    0
}

/// Redirect fd to a file, path given as (ptr, len).
#[unsafe(no_mangle)]
pub extern "C" fn sash_redirect_s(fd: i64, path_ptr: *const u8, path_len: i64, flags: i64) -> i64 {
    unsafe {
        let len = path_len as usize;
        if len >= 4095 { return -1; }
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
        if (*prefix.add(prefix_count)).is_null() { break; }
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

#[repr(C)]
struct TableEntry {
    key_ptr: *const u8,
    key_len: i64,
    val_ptr: *const u8,
    val_len: i64,
}

#[repr(C)]
struct SashTable {
    indices: *mut i32,
    entries: *mut TableEntry,
    index_mask: u32,
    entry_count: u32,
    entry_capacity: u32,
}

/// Safe view of the index array for table operations.
/// entries_eq(i, key) returns true if entry i's key matches key.
fn table_find_slot_inner(indices: &[i32], entries_eq: impl Fn(usize, &[u8]) -> bool, mask: u32, key: &[u8]) -> u32 {
    let h = table_hash_inner(key);
    let mut pos = h & mask;
    loop {
        let idx = indices[pos as usize];
        if idx == TABLE_EMPTY {
            return pos;
        }
        if entries_eq(idx as usize, key) {
            return pos;
        }
        pos = (pos + 1) & mask;
    }
}

unsafe fn table_grow(t: *mut SashTable) {
    let new_index_size = ((*t).index_mask + 1) * 2;
    let new_mask = new_index_size - 1;
    let new_indices = sash_alloc((new_index_size as usize) * 4) as *mut i32;
    // Init to EMPTY
    let idx_slice = core::slice::from_raw_parts_mut(new_indices, new_index_size as usize);
    for slot in idx_slice.iter_mut() {
        *slot = TABLE_EMPTY;
    }
    // Rehash existing entries
    for i in 0..(*t).entry_count as usize {
        let e = &*(*t).entries.add(i);
        let key = core::slice::from_raw_parts(e.key_ptr, e.key_len as usize);
        let h = table_hash_inner(key);
        let mut pos = h & new_mask;
        loop {
            if idx_slice[pos as usize] == TABLE_EMPTY {
                idx_slice[pos as usize] = i as i32;
                break;
            }
            pos = (pos + 1) & new_mask;
        }
    }
    (*t).indices = new_indices;
    (*t).index_mask = new_mask;
}

#[no_mangle]
pub unsafe extern "C" fn sash_table_new(capacity: i64) -> *mut SashTable {
    let cap = if capacity < 4 { 4u32 } else { (capacity as u32).next_power_of_two() };
    let index_size = (cap * 2).next_power_of_two();
    let t = sash_alloc(core::mem::size_of::<SashTable>()) as *mut SashTable;
    let indices = sash_alloc((index_size as usize) * 4) as *mut i32;
    let idx_slice = core::slice::from_raw_parts_mut(indices, index_size as usize);
    for slot in idx_slice.iter_mut() {
        *slot = TABLE_EMPTY;
    }
    let entries = sash_alloc((cap as usize) * core::mem::size_of::<TableEntry>()) as *mut TableEntry;
    (*t).indices = indices;
    (*t).entries = entries;
    (*t).index_mask = index_size - 1;
    (*t).entry_count = 0;
    (*t).entry_capacity = cap;
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
    let pos = table_find_slot_inner(indices, |i, k| {
        let e = &*entries_ptr.add(i);
        e.key_len == k.len() as i64 && core::slice::from_raw_parts(e.key_ptr, e.key_len as usize) == k
    }, (*t).index_mask, key);

    let idx = *(*t).indices.add(pos as usize);
    if idx != TABLE_EMPTY {
        let e = &mut *(*t).entries.add(idx as usize);
        e.val_ptr = val_ptr;
        e.val_len = val_len;
    } else {
        if (*t).entry_count >= (*t).entry_capacity {
            let new_cap = (*t).entry_capacity * 2;
            let new_entries = sash_alloc((new_cap as usize) * core::mem::size_of::<TableEntry>()) as *mut TableEntry;
            core::ptr::copy_nonoverlapping((*t).entries, new_entries, (*t).entry_count as usize);
            (*t).entries = new_entries;
            (*t).entry_capacity = new_cap;
        }
        let ei = (*t).entry_count as usize;
        let e = &mut *(*t).entries.add(ei);
        // Copy key to heap so table owns it (caller's key may be stack-allocated)
        let key_copy = sash_alloc(key_len as usize);
        core::ptr::copy_nonoverlapping(key_ptr, key_copy, key_len as usize);
        e.key_ptr = key_copy;
        e.key_len = key_len;
        e.val_ptr = val_ptr;
        e.val_len = val_len;
        *(*t).indices.add(pos as usize) = ei as i32;
        (*t).entry_count += 1;
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
    let pos = table_find_slot_inner(indices, |i, k| {
        let e = &*entries_ptr.add(i);
        e.key_len == k.len() as i64 && core::slice::from_raw_parts(e.key_ptr, e.key_len as usize) == k
    }, (*t).index_mask, key);

    let idx = indices[pos as usize];
    if idx == TABLE_EMPTY {
        *out_len = 0;
        return core::ptr::null();
    }
    let e = &*(*t).entries.add(idx as usize);
    *out_len = e.val_len;
    e.val_ptr
}

#[no_mangle]
pub unsafe extern "C" fn sash_table_len(t: *const SashTable) -> i64 {
    (*t).entry_count as i64
}

#[no_mangle]
pub unsafe extern "C" fn sash_table_key(
    t: *const SashTable,
    index: i64,
    out_len: *mut i64,
) -> *const u8 {
    if index < 0 || index >= (*t).entry_count as i64 {
        *out_len = 0;
        return core::ptr::null();
    }
    let e = &*(*t).entries.add(index as usize);
    *out_len = e.key_len;
    e.key_ptr
}

#[no_mangle]
pub unsafe extern "C" fn sash_table_value(
    t: *const SashTable,
    index: i64,
    out_len: *mut i64,
) -> *const u8 {
    if index < 0 || index >= (*t).entry_count as i64 {
        *out_len = 0;
        return core::ptr::null();
    }
    let e = &*(*t).entries.add(index as usize);
    *out_len = e.val_len;
    e.val_ptr
}

// ═══════════════════════════════════════════════════════════════
// Fixed-point conversion (DD#36)
// ═══════════════════════════════════════════════════════════════

/// dtoi(digits, string) → int
/// Convert decimal string to fixed-point integer with `digits` decimal places.
#[no_mangle]
pub unsafe extern "C" fn sash_dtoi(digits: i64, ptr: *const u8, len: i64) -> i64 {
    let bytes = core::slice::from_raw_parts(ptr, len as usize);
    dtoi_inner(digits, bytes)
}

/// itod(digits, value) → (ptr, len)
/// Convert fixed-point integer to decimal string with `digits` decimal places.
#[no_mangle]
pub unsafe extern "C" fn sash_itod(digits: i64, value: i64, out_len: *mut i64) -> *const u8 {
    static mut ITOD_BUF: [u8; 32] = [0u8; 32];
    let len = itod_inner(digits, value, &mut ITOD_BUF);
    *out_len = len as i64;
    ITOD_BUF.as_ptr()
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
    // Write "sash: regexp error: <msg>\n" to stderr
    let prefix = b"sash: regexp error: ";
    sash_write(2, prefix.as_ptr(), prefix.len() as i64);
    if !s.is_null() {
        let len = strlen(s);
        sash_write(2, s, len as i64);
    }
    sash_write(2, b"\n".as_ptr(), 1);
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
    str_ptr: *const u8, str_len: i64,
    pat_ptr: *const u8, pat_len: i64,
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
        return 0;
    }
    // Resub is 4 pointers per entry, 10 entries = 320 bytes on 64-bit
    let mut resub = [0u8; 320];
    let result = regexec9(prog, str_buf, resub.as_mut_ptr(), 10);
    if result > 0 { 1 } else { 0 }
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
    if path.len() >= buf.len() { return 0; }
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
                Ok(st) => if rustix::fs::FileType::from_raw_mode(st.st_mode) == rustix::fs::FileType::RegularFile { 1 } else { 0 },
                Err(_) => 0,
            }
        }
        2 => {
            // -d: is directory
            match rustix::fs::stat(path_cstr) {
                Ok(st) => if rustix::fs::FileType::from_raw_mode(st.st_mode) == rustix::fs::FileType::Directory { 1 } else { 0 },
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
                Ok(st) => if st.st_size > 0 { 1 } else { 0 },
                Err(_) => 0,
            }
        }
        7 => {
            // -L: is symlink (use lstat)
            match rustix::fs::lstat(path_cstr) {
                Ok(st) => if rustix::fs::FileType::from_raw_mode(st.st_mode) == rustix::fs::FileType::Symlink { 1 } else { 0 },
                Err(_) => 0,
            }
        }
        _ => 0,
    }
}

/// Integer power: base ** exp with checked_mul for overflow detection.
/// Returns base^exp. On overflow, returns i64::MAX (saturating).
#[unsafe(no_mangle)]
pub extern "C" fn sash_pow(base: i64, exp: i64) -> i64 {
    if exp < 0 { return 0; } // negative exponent → 0 in integer math
    if exp == 0 { return 1; }
    let mut result: i64 = 1;
    for _ in 0..exp {
        match result.checked_mul(base) {
            Some(v) => result = v,
            None => return i64::MAX, // overflow → saturate
        }
    }
    result
}

/// Substring: ${var:offset:length}
/// Returns (ptr, len) of the substring. Negative offset counts from end.
/// If length is 0 (not provided), returns from offset to end.
#[unsafe(no_mangle)]
pub extern "C" fn sash_substring(
    s_ptr: *const u8, s_len: i64,
    offset: i64, length: i64, has_length: i64,
) -> (*const u8, i64) {
    let total = s_len as usize;
    // Resolve offset (negative = from end)
    let start = if offset < 0 {
        let from_end = (-offset) as usize;
        if from_end > total { 0 } else { total - from_end }
    } else {
        let o = offset as usize;
        if o > total { total } else { o }
    };
    
    let remaining = total - start;
    let len = if has_length != 0 {
        let l = length as usize;
        if l > remaining { remaining } else { l }
    } else {
        remaining
    };
    
    (unsafe { s_ptr.add(start) }, len as i64)
}

/// Test if fd is a terminal (isatty equivalent)
#[unsafe(no_mangle)]
pub extern "C" fn sash_isatty(fd: i64) -> i64 {
    let raw_fd = fd as core::ffi::c_int;
    let fd = unsafe { rustix::fd::BorrowedFd::borrow_raw(raw_fd) };
    rustix::termios::isatty(fd) as i64
}

/// Compare file modification times: returns 1 if file1 is newer than file2.
#[unsafe(no_mangle)]
pub extern "C" fn sash_file_newer(
    p1_ptr: *const u8, p1_len: i64,
    p2_ptr: *const u8, p2_len: i64,
) -> i64 {
    let get_mtime = |ptr: *const u8, len: i64| -> Option<i64> {
        if ptr.is_null() || len <= 0 { return None; }
        let path = unsafe { core::slice::from_raw_parts(ptr, len as usize) };
        let mut buf = [0u8; 4096];
        if path.len() >= buf.len() { return None; }
        buf[..path.len()].copy_from_slice(path);
        buf[path.len()] = 0;
        let cstr = unsafe { core::ffi::CStr::from_ptr(buf.as_ptr() as *const core::ffi::c_char) };
        match rustix::fs::stat(cstr) {
            Ok(st) => Some(st.st_mtime as i64 * 1_000_000_000 + st.st_mtime_nsec as i64),
            Err(_) => None,
        }
    };
    match (get_mtime(p1_ptr, p1_len), get_mtime(p2_ptr, p2_len)) {
        (Some(t1), Some(t2)) => if t1 > t2 { 1 } else { 0 },
        (Some(_), None) => 1,  // file1 exists, file2 doesn't → file1 newer
        _ => 0,
    }
}

/// Change directory. Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn sash_chdir(path_ptr: *const u8, path_len: i64) -> i64 {
    if path_ptr.is_null() || path_len <= 0 { return -1; }
    let path = unsafe { core::slice::from_raw_parts(path_ptr, path_len as usize) };
    let mut buf = [0u8; 4096];
    if path.len() >= buf.len() { return -1; }
    buf[..path.len()].copy_from_slice(path);
    buf[path.len()] = 0;
    let path_cstr = unsafe { core::ffi::CStr::from_ptr(buf.as_ptr() as *const core::ffi::c_char) };
    match rustix::process::chdir(path_cstr) {
        Ok(()) => 0,
        Err(_) => -1,
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
            *out_len = 0;
            return core::ptr::null();
        }
        // ret includes null terminator, we want length without it
        let len = ret - 1;
        *out_len = len;
        CWD_BUF.as_ptr()
    }
}

/// Set up heredoc: create pipe, fork-write body to write end, dup2 read end to fd.
/// For simplicity in a static binary (no threads), we write synchronously then close.
/// This works for small heredocs. Large ones would need fork.
#[unsafe(no_mangle)]
pub extern "C" fn sash_heredoc_redirect(fd: i64, body_ptr: *const u8, body_len: i64) {
    if body_ptr.is_null() || body_len < 0 { return; }
    let (read_fd, write_fd) = match rustix::pipe::pipe() {
        Ok(pair) => pair,
        Err(_) => return,
    };
    // Write the body to the pipe
    let body = unsafe { core::slice::from_raw_parts(body_ptr, body_len as usize) };
    let _ = rustix::io::write(&write_fd, body);
    drop(write_fd);
    // dup2 read end to target fd via raw syscall
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") 33_i64,  // __NR_dup2
            in("rdi") rustix::fd::AsRawFd::as_raw_fd(&read_fd) as i64,
            in("rsi") fd,
            lateout("rax") _,
            lateout("rcx") _,
            lateout("r11") _,
        );
    }
    drop(read_fd);
}

/// Convert first character to uppercase. Returns (ptr, len).
#[unsafe(no_mangle)]
pub extern "C" fn sash_toupper_first(s_ptr: *const u8, s_len: i64, out_ptr: *mut u8) -> i64 {
    if s_ptr.is_null() || s_len <= 0 { return 0; }
    let s = unsafe { core::slice::from_raw_parts(s_ptr, s_len as usize) };
    let out = unsafe { core::slice::from_raw_parts_mut(out_ptr, s_len as usize) };
    out.copy_from_slice(s);
    if !out.is_empty() && out[0] >= b'a' && out[0] <= b'z' {
        out[0] -= 32;
    }
    s_len
}

/// Convert first character to lowercase. Returns len.
#[unsafe(no_mangle)]
pub extern "C" fn sash_tolower_first(s_ptr: *const u8, s_len: i64, out_ptr: *mut u8) -> i64 {
    if s_ptr.is_null() || s_len <= 0 { return 0; }
    let s = unsafe { core::slice::from_raw_parts(s_ptr, s_len as usize) };
    let out = unsafe { core::slice::from_raw_parts_mut(out_ptr, s_len as usize) };
    out.copy_from_slice(s);
    if !out.is_empty() && out[0] >= b'A' && out[0] <= b'Z' {
        out[0] += 32;
    }
    s_len
}

/// Convert all characters to uppercase. Returns len.
#[unsafe(no_mangle)]
pub extern "C" fn sash_toupper_all(s_ptr: *const u8, s_len: i64, out_ptr: *mut u8) -> i64 {
    if s_ptr.is_null() || s_len <= 0 { return 0; }
    let s = unsafe { core::slice::from_raw_parts(s_ptr, s_len as usize) };
    let out = unsafe { core::slice::from_raw_parts_mut(out_ptr, s_len as usize) };
    for i in 0..s.len() {
        out[i] = if s[i] >= b'a' && s[i] <= b'z' { s[i] - 32 } else { s[i] };
    }
    s_len
}

/// Convert all characters to lowercase. Returns len.
#[unsafe(no_mangle)]
pub extern "C" fn sash_tolower_all(s_ptr: *const u8, s_len: i64, out_ptr: *mut u8) -> i64 {
    if s_ptr.is_null() || s_len <= 0 { return 0; }
    let s = unsafe { core::slice::from_raw_parts(s_ptr, s_len as usize) };
    let out = unsafe { core::slice::from_raw_parts_mut(out_ptr, s_len as usize) };
    for i in 0..s.len() {
        out[i] = if s[i] >= b'A' && s[i] <= b'Z' { s[i] + 32 } else { s[i] };
    }
    s_len
}

/// command -v: check if a command exists in PATH.
/// Prints the path if found, returns 0 on success, 1 on failure.
#[unsafe(no_mangle)]
pub extern "C" fn sash_command_v(name_ptr: *const u8, name_len: i64) -> i64 {
    if name_ptr.is_null() || name_len <= 0 { return 1; }
    let name = unsafe { core::slice::from_raw_parts(name_ptr, name_len as usize) };
    
    // Get PATH
    let path_env = b"PATH\0";
    let path_cstr = unsafe { core::ffi::CStr::from_ptr(path_env.as_ptr() as *const core::ffi::c_char) };
    
    // Use raw getenv via syscall... actually, use our existing sash_getenv pattern
    // For simplicity, read /proc/self/environ or just use the C getenv pattern
    // Actually let's do the simple thing: iterate PATH entries
    
    // We need the PATH value. Let's use a static buffer approach.
    extern "C" { fn sash_getenv(name_ptr: *const u8, name_len: i64, out_len: *mut i64) -> *const u8; }
    let mut path_len: i64 = 0;
    let path_ptr = unsafe { sash_getenv(b"PATH".as_ptr(), 4, &mut path_len) };
    if path_ptr.is_null() || path_len <= 0 { return 1; }
    let path = unsafe { core::slice::from_raw_parts(path_ptr, path_len as usize) };
    
    let mut buf = [0u8; 4096];
    // Split PATH on ':'
    let mut start = 0;
    loop {
        let end = path[start..].iter().position(|&b| b == b':').map(|p| start + p).unwrap_or(path.len());
        let dir = &path[start..end];
        let total = dir.len() + 1 + name.len();
        if total < buf.len() - 1 {
            buf[..dir.len()].copy_from_slice(dir);
            buf[dir.len()] = b'/';
            buf[dir.len()+1..dir.len()+1+name.len()].copy_from_slice(name);
            buf[total] = 0;
            let cpath = unsafe { core::ffi::CStr::from_ptr(buf.as_ptr() as *const core::ffi::c_char) };
            // Check if file exists and is executable
            match rustix::fs::access(cpath, rustix::fs::Access::EXEC_OK) {
                Ok(()) => {
                    // Print the path
                    let _ = rustix::io::write(unsafe { rustix::fd::BorrowedFd::borrow_raw(1) }, &buf[..total]);
                    let _ = rustix::io::write(unsafe { rustix::fd::BorrowedFd::borrow_raw(1) }, b"\n");
                    return 0;
                }
                Err(_) => {}
            }
        }
        if end >= path.len() { break; }
        start = end + 1;
    }
    1
}


/// Get process ID.
#[unsafe(no_mangle)]
pub extern "C" fn sash_getpid() -> i64 {
    rustix::process::getpid().as_raw_nonzero().get() as i64
}
