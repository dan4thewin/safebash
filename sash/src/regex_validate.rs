//! DD#39: Compile-time regex validation.
//! Calls Plan 9 regcomp9 on literal patterns to catch errors early.

use std::ffi::CString;

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
        if prog.is_null() || regex_get_error_flag() != 0 {
            let msg = if regex_get_error_flag() != 0 {
                let raw = regex_get_error_msg();
                let mut len = 0;
                while *raw.add(len) != 0 {
                    len += 1;
                }
                std::str::from_utf8(std::slice::from_raw_parts(raw, len))
                    .unwrap_or("unknown regex error")
                    .to_string()
            } else {
                "regex compilation failed".to_string()
            };
            regex_clear_error();
            return Err(msg);
        }
        sash_free(prog);
        regex_clear_error();
    }
    Ok(())
}
