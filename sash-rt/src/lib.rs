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
// sash_run — execute a command with PATH lookup
// ═══════════════════════════════════════════════════════════════

#[no_mangle]
pub extern "C" fn sash_run(argv: *const *const u8, envp: *const *const u8) -> i64 {
    unsafe { run_command(argv, envp) }
}

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

unsafe fn run_command(argv: *const *const u8, envp: *const *const u8) -> i64 {
    let cmd = *argv;
    if cmd.is_null() {
        return 127;
    }

    let cmd_bytes = cstr_bytes(cmd);
    let has_slash = cmd_bytes.iter().any(|&b| b == b'/');

    // CStr for execve — points to the same null-terminated data.
    let cmd_cstr = core::ffi::CStr::from_ptr(cmd as *const i8);

    match rustix::runtime::kernel_fork() {
        Ok(rustix::runtime::Fork::Child(_)) => {
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

                if dir.is_empty() {
                    continue;
                }
                let total = dir.len() + 1 + cmd_bytes.len() + 1;
                if total > buf.len() {
                    continue;
                }

                // Build path: dir/cmd\0
                let mut j = 0;
                while j < dir.len() {
                    buf[j] = dir[j];
                    j += 1;
                }
                buf[j] = b'/';
                j += 1;
                let mut k = 0;
                while k < cmd_bytes.len() {
                    buf[j + k] = cmd_bytes[k];
                    k += 1;
                }
                buf[j + k] = 0;

                let path_cstr = core::ffi::CStr::from_ptr(buf.as_ptr() as *const i8);
                if rustix::fs::access(path_cstr, rustix::fs::Access::EXEC_OK).is_ok() {
                    let _ = rustix::runtime::execve(path_cstr, argv, envp);
                }
            }

            rustix::runtime::exit_group(127);
        }
        Ok(rustix::runtime::Fork::ParentOf(child_pid)) => {
            match rustix::process::waitpid(
                Some(child_pid),
                rustix::process::WaitOptions::empty(),
            ) {
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
        Err(_) => 127,
    }
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
        "lea rax, [rsp + rdi*8 + 16]",
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
        "ldr x0, [sp]",
        "add x1, sp, x0, lsl #3",
        "add x1, x1, #16",
        "adrp x2, sash_envp",
        "str x1, [x2, :lo12:sash_envp]",
        "bl main",
        "mov x8, #93",
        "svc #0",
    )
}
