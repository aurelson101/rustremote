pub const MAX_OUTPUT: usize = 64 * 1024;
#[cfg(any(windows, test))]
pub const EXPECTED: &str = "RUSTREMOTE_CONPTY_OK_été";

#[cfg(any(windows, test))]
pub fn validate(output: &[u8], overflow: bool) -> Result<(), String> {
    if overflow || output.len() > MAX_OUTPUT {
        return Err("ConPTY output exceeded 64 KiB".into());
    }
    let text = std::str::from_utf8(output).map_err(|_| "ConPTY output is not valid UTF-8")?;
    if !text.contains(EXPECTED) {
        return Err(format!("ConPTY marker missing: bytes={}", output.len()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_complete_valid_bounded_output() {
        assert!(validate(EXPECTED.as_bytes(), false).is_ok());
        let mut invalid = EXPECTED.as_bytes().to_vec();
        invalid.push(0xff);
        assert!(validate(&invalid, false).is_err());
        assert!(validate(b"echo %RR_A%%RR_B%", false).is_err());
        assert!(validate(EXPECTED.as_bytes(), true).is_err());
        assert!(validate(&vec![b'a'; MAX_OUTPUT + 1], false).is_err());
    }
}
