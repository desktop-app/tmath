//! `std::wstring` operations with the C++ semantics the parser relies on:
//! positions past the end throw `std::out_of_range`, counts past the end are
//! clamped, and a negative count converted to `size_t` means "to the end".

use crate::error::{Error, Result};

pub type WStr = Vec<u16>;

pub fn w(text: &str) -> WStr {
    text.encode_utf16().collect()
}

fn clamp_count(len: usize, pos: usize, count: i64) -> usize {
    if count < 0 {
        len - pos
    } else {
        (count as u64).min((len - pos) as u64) as usize
    }
}

/// `s.substr(pos, count)`.
pub fn substr(s: &[u16], pos: i64, count: i64) -> Result<WStr> {
    if pos < 0 || pos as usize > s.len() {
        return Err(Error::out_of_range());
    }
    let pos = pos as usize;
    let n = clamp_count(s.len(), pos, count);
    Ok(s[pos..pos + n].to_vec())
}

/// `s.substr(pos)`.
pub fn substr_from(s: &[u16], pos: i64) -> Result<WStr> {
    substr(s, pos, -1)
}

/// `s.replace(pos, count, with)`.
pub fn replace(s: &mut WStr, pos: i64, count: i64, with: &[u16]) -> Result<()> {
    if pos < 0 || pos as usize > s.len() {
        return Err(Error::out_of_range());
    }
    let pos = pos as usize;
    let n = clamp_count(s.len(), pos, count);
    s.splice(pos..pos + n, with.iter().copied());
    Ok(())
}

/// `s.erase(pos, count)`.
pub fn erase(s: &mut WStr, pos: i64, count: i64) -> Result<()> {
    replace(s, pos, count, &[])
}

/// `s.find(what, from)`.
pub fn find(s: &[u16], what: &[u16], from: usize) -> Option<usize> {
    if what.is_empty() {
        return (from <= s.len()).then_some(from);
    }
    if what.len() > s.len() {
        return None;
    }
    (from..=s.len() - what.len()).find(|&i| s[i..i + what.len()] == *what)
}

/// `replaceall(src, from, to)`.
pub fn replace_all(src: &mut WStr, from: &[u16], to: &[u16]) {
    if from.is_empty() {
        return;
    }
    let mut start = 0;
    while let Some(found) = find(src, from, start) {
        src.splice(found..found + from.len(), to.iter().copied());
        start = found + to.len();
    }
}

/// `towstring(int)`.
pub fn from_int(value: i64) -> WStr {
    w(&value.to_string())
}

pub fn starts_with(s: &[u16], prefix: &str) -> bool {
    let p = w(prefix);
    s.len() >= p.len() && s[..p.len()] == p[..]
}

pub fn eq(s: &[u16], text: &str) -> bool {
    s.iter().copied().eq(text.encode_utf16())
}

/// The message form of a command name: `wide2utf8`.
pub fn narrow(s: &[u16]) -> String {
    String::from_utf8_lossy(&crate::env::wide_to_utf8(s)).into_owned()
}

/// `tostring(wchar_t)`: `wctomb` in the "C" locale, one byte for values up
/// to 255 and "?" for the rest. Bytes past ASCII are kept as Latin-1 here,
/// which only changes how such a message prints.
pub fn char_message(c: u16) -> String {
    if c <= 0xFF {
        char::from(c as u8).to_string()
    } else {
        "?".to_owned()
    }
}

/// `wide2utf8` bytes.
pub fn narrow_bytes(s: &[u16]) -> Vec<u8> {
    crate::env::wide_to_utf8(s)
}
