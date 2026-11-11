//! Linux calls that do not fit public rustix APIs (DD#129).
//! Architecture-specific registers, numbers and raw results stay here.

use core::ffi::CStr;
use core::mem::{size_of, MaybeUninit};
use linux_raw_sys::general as linux;
use rustix::fd::RawFd;
use rustix::io::{Errno, Result};

#[cfg(not(all(
    target_os = "linux",
    target_pointer_width = "64",
    any(target_arch = "x86_64", target_arch = "aarch64")
)))]
compile_error!("sash-rt requires x86_64 or AArch64 Linux");

#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn syscall(number: u32, args: [usize; 6]) -> isize {
    let result;
    core::arch::asm!(
        "syscall",
        inlateout("rax") number as usize => result,
        in("rdi") args[0],
        in("rsi") args[1],
        in("rdx") args[2],
        in("r10") args[3],
        in("r8") args[4],
        in("r9") args[5],
        lateout("rcx") _,
        lateout("r11") _,
        options(nostack),
    );
    result
}

#[cfg(target_arch = "aarch64")]
#[inline(always)]
unsafe fn syscall(number: u32, args: [usize; 6]) -> isize {
    let result;
    core::arch::asm!(
        "svc 0",
        in("x8") number as usize,
        inlateout("x0") args[0] => result,
        in("x1") args[1],
        in("x2") args[2],
        in("x3") args[3],
        in("x4") args[4],
        in("x5") args[5],
        options(nostack),
    );
    result
}

fn decode(result: isize) -> Result<usize> {
    if result < 0 {
        Err(Errno::from_raw_os_error(-result as i32))
    } else {
        Ok(result as usize)
    }
}

pub(super) unsafe fn fork() -> Result<i32> {
    let mut child_tid = MaybeUninit::<i32>::uninit();
    let mut args = [
        (linux::SIGCHLD | linux::CLONE_CHILD_SETTID) as usize,
        0,
        0,
        0,
        0,
        0,
    ];
    // clone's child-tid and TLS arguments swap positions between these ABIs.
    #[cfg(target_arch = "x86_64")]
    {
        args[3] = child_tid.as_mut_ptr() as usize;
    }
    #[cfg(target_arch = "aarch64")]
    {
        args[4] = child_tid.as_mut_ptr() as usize;
    }
    decode(syscall(linux::__NR_clone, args)).map(|pid| pid as i32)
}

pub(super) unsafe fn execve(path: &CStr, argv: *const *const u8, envp: *const *const u8) -> Errno {
    decode(syscall(
        linux::__NR_execve,
        [
            path.as_ptr() as usize,
            argv as usize,
            envp as usize,
            0,
            0,
            0,
        ],
    ))
    .expect_err("execve returned without replacing the process")
}

#[cfg(target_arch = "x86_64")]
pub(super) fn exit_group(status: i32) -> ! {
    unsafe {
        core::arch::asm!(
            "syscall",
            "ud2",
            in("rax") linux::__NR_exit_group as usize,
            in("rdi") status as usize,
            options(noreturn, nostack),
        );
    }
}

#[cfg(target_arch = "aarch64")]
pub(super) fn exit_group(status: i32) -> ! {
    unsafe {
        core::arch::asm!(
            "svc 0",
            "brk 0",
            in("x8") linux::__NR_exit_group as usize,
            in("x0") status as usize,
            options(noreturn, nostack),
        );
    }
}

// These accept possibly closed raw descriptors, without inventing Rust fd ownership.
pub(super) fn fcntl_dupfd_cloexec(fd: RawFd, min: RawFd) -> Result<RawFd> {
    decode(unsafe {
        syscall(
            linux::__NR_fcntl,
            [
                fd as usize,
                linux::F_DUPFD_CLOEXEC as usize,
                min as usize,
                0,
                0,
                0,
            ],
        )
    })
    .map(|fd| fd as RawFd)
}

pub(super) unsafe fn dup2(src: RawFd, dst: RawFd) -> Result<()> {
    #[cfg(target_arch = "x86_64")]
    let result = syscall(linux::__NR_dup2, [src as usize, dst as usize, 0, 0, 0, 0]);
    #[cfg(target_arch = "aarch64")]
    let result = if src == dst {
        // dup3 rejects this case; dup2 must check validity without changing flags.
        syscall(
            linux::__NR_fcntl,
            [src as usize, linux::F_GETFD as usize, 0, 0, 0, 0],
        )
    } else {
        syscall(linux::__NR_dup3, [src as usize, dst as usize, 0, 0, 0, 0])
    };
    decode(result).map(|_| ())
}

pub(super) unsafe fn close(fd: RawFd) -> Result<()> {
    decode(syscall(linux::__NR_close, [fd as usize, 0, 0, 0, 0, 0])).map(|_| ())
}

pub(super) fn getcwd(buffer: &mut [u8]) -> Result<usize> {
    // Public rustix getcwd allocates a CString; the runtime owns its buffer.
    decode(unsafe {
        syscall(
            linux::__NR_getcwd,
            [buffer.as_mut_ptr() as usize, buffer.len(), 0, 0, 0, 0],
        )
    })
}

// Kernel layout on both supported 64-bit ABIs; the mask is 8 bytes, not libc's 128.
#[repr(C)]
struct Sigaction {
    handler: usize,
    flags: usize,
    restorer: usize,
    mask: u64,
}

pub(super) unsafe fn sigaction(signum: i32, handler: usize) -> Result<()> {
    let action = Sigaction {
        handler,
        flags: (linux::SA_RESTART | linux::SA_RESTORER) as usize,
        restorer: signal_restorer as *const () as usize,
        mask: 0,
    };
    decode(syscall(
        linux::__NR_rt_sigaction,
        [
            signum as usize,
            &action as *const Sigaction as usize,
            0,
            size_of::<u64>(),
            0,
            0,
        ],
    ))
    .map(|_| ())
}

#[cfg(target_arch = "x86_64")]
#[unsafe(naked)]
unsafe extern "C" fn signal_restorer() {
    core::arch::naked_asm!(
        "mov rax, {number}",
        "syscall",
        "ud2",
        number = const linux::__NR_rt_sigreturn,
    );
}

#[cfg(target_arch = "aarch64")]
#[unsafe(naked)]
unsafe extern "C" fn signal_restorer() {
    core::arch::naked_asm!(
        "mov x8, #{number}",
        "svc 0",
        "brk 0",
        number = const linux::__NR_rt_sigreturn,
    );
}
