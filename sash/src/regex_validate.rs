//! DD#39: Compile-time regex validation.
//! Calls Plan 9 regcomp9 on literal patterns to catch errors early.

use std::ffi::CString;
use std::ptr::NonNull;

extern "C" {
    fn regcomp9(pattern: *const u8) -> *mut u8;
    fn regex_get_error_flag() -> i32;
    fn regex_get_error_msg() -> *const u8;
    fn regex_clear_error();
    fn sash_free(ptr: *mut u8);
}

/// Validate a regex pattern at compile time.
/// Returns Ok(()) if valid, Err(message) if invalid.
pub fn validate_regex(pattern: &str) -> Result<(), String> {
    let c_pat = match CString::new(pattern) {
        Ok(c) => c,
        Err(_) => return Err("pattern contains null byte".to_string()),
    };

    unsafe {
        regex_clear_error();
        let prog = regcomp9(c_pat.as_ptr() as *const u8);
        if regex_get_error_flag() != 0 {
            let raw = regex_get_error_msg();
            let mut len = 0;
            while *raw.add(len) != 0 {
                len += 1;
            }
            let msg = std::str::from_utf8(std::slice::from_raw_parts(raw, len))
                .unwrap_or("unknown regex error")
                .to_string();
            regex_clear_error();
            return Err(msg);
        }
        let prog = successful_regex_program(prog);
        sash_free(prog.as_ptr());
        regex_clear_error();
    }
    Ok(())
}

/// `regcomp9` reports every failure through `regerror9`, including OOM.
/// Once that flag is checked, success must carry an allocation to free.
fn successful_regex_program(raw: *mut u8) -> NonNull<u8> {
    let Some(program) = NonNull::new(raw) else {
        unreachable!("regcomp9 returned null without setting its error flag");
    };
    program
}

#[cfg(test)]
mod invariant_tests {
    #[test]
    #[should_panic(expected = "regcomp9 returned null without setting its error flag")]
    fn a_successful_regex_compilation_must_have_a_program() {
        super::successful_regex_program(std::ptr::null_mut());
    }
}
