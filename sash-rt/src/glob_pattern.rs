//! The same bounded pattern language is checked by the compiler and runtime.

pub fn split(pattern: &[u8]) -> Result<(&[u8], &[u8]), &'static str> {
    if pattern.contains(&0) {
        return Err("glob patterns cannot contain NUL");
    }
    let (directory, filename) = match pattern.iter().rposition(|&b| b == b'/') {
        Some(0) => (&pattern[..1], &pattern[1..]),
        Some(pos) => (&pattern[..pos], &pattern[pos + 1..]),
        None => (&b"."[..], pattern),
    };
    if directory.contains(&b'*') {
        return Err("glob stars are allowed only in the final filename component");
    }
    if filename.iter().filter(|&&b| b == b'*').count() > 1 {
        return Err("glob patterns allow at most one star");
    }
    Ok((directory, filename))
}
