/* Minimal setjmp/longjmp for x86_64 — no libc required.
 * Saves/restores callee-saved registers + rsp + rip.
 * Used by Plan 9 regcomp for error recovery.
 */

/* jmp_buf layout: rbx, rbp, r12, r13, r14, r15, rsp, rip = 8 slots */

.text
.globl _setjmp
.globl setjmp
.globl longjmp
.globl __longjmp_chk

_setjmp:
setjmp:
    mov  %rbx,   (%rdi)
    mov  %rbp,  8(%rdi)
    mov  %r12, 16(%rdi)
    mov  %r13, 24(%rdi)
    mov  %r14, 32(%rdi)
    mov  %r15, 40(%rdi)
    lea  8(%rsp), %rax       /* rsp before call */
    mov  %rax, 48(%rdi)
    mov  (%rsp), %rax        /* return address */
    mov  %rax, 56(%rdi)
    xor  %eax, %eax          /* return 0 */
    ret

__longjmp_chk:
longjmp:
    mov  %esi, %eax          /* return value */
    test %eax, %eax
    jnz  1f
    inc  %eax                /* if val==0, return 1 */
1:
    mov    (%rdi), %rbx
    mov   8(%rdi), %rbp
    mov  16(%rdi), %r12
    mov  24(%rdi), %r13
    mov  32(%rdi), %r14
    mov  40(%rdi), %r15
    mov  48(%rdi), %rsp
    jmp *56(%rdi)
