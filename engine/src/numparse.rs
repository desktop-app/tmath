//! Number parsing as MicroTeX's `valueof()` does it: `std::stringstream >>`
//! with the MSVC standard library in the classic locale.

/// Classic-locale `isspace`.
fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t'..=b'\r')
}

/// The outcome of a stream read: the value the variable ends up holding, and
/// whether the text used a form this port does not read yet (hexadecimal
/// floating point), in which case the value is 0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Float {
    pub value: f32,
    pub unsupported: bool,
}

/// `stream >> float`. Fields are gathered as `num_get` gathers them: white
/// space, a sign, digits with one decimal point, then an exponent. Without a
/// digit (in the mantissa, or after an exponent marker) the read fails and
/// gives 0. Otherwise the value is the correctly rounded `strtof` of the
/// field, infinity on overflow included.
pub fn read_float(text: &[u8]) -> Float {
    let ok = |value| Float {
        value,
        unsupported: false,
    };
    let mut i = 0;
    while i < text.len() && is_space(text[i]) {
        i += 1;
    }
    let mut number = String::new();
    if i < text.len() && (text[i] == b'+' || text[i] == b'-') {
        number.push(text[i] as char);
        i += 1;
    }
    let mut seen_digit = false;
    if i < text.len() && text[i] == b'0' {
        i += 1;
        if i == text.len() {
            number.push('0');
            return ok(number.parse().unwrap_or(0.0));
        }
        if text[i] == b'x' || text[i] == b'X' {
            return Float {
                value: 0.0,
                unsupported: true,
            };
        }
        number.push('0');
        seen_digit = true;
    }
    while i < text.len() && text[i].is_ascii_digit() {
        number.push(text[i] as char);
        seen_digit = true;
        i += 1;
    }
    if i < text.len() && text[i] == b'.' {
        number.push('.');
        i += 1;
        while i < text.len() && text[i].is_ascii_digit() {
            number.push(text[i] as char);
            seen_digit = true;
            i += 1;
        }
    }
    if seen_digit && i < text.len() && (text[i] == b'e' || text[i] == b'E') {
        i += 1;
        seen_digit = false;
        let mut exponent = String::from("e");
        if i < text.len() && (text[i] == b'+' || text[i] == b'-') {
            exponent.push(text[i] as char);
            i += 1;
        }
        while i < text.len() && text[i].is_ascii_digit() {
            exponent.push(text[i] as char);
            seen_digit = true;
            i += 1;
        }
        number.push_str(&exponent);
    }
    if !seen_digit {
        return ok(0.0);
    }
    if number.starts_with(['+', '-']) && number[1..].starts_with('.') {
        number.insert(1, '0');
    } else if number.starts_with('.') {
        number.insert(0, '0');
    }
    ok(number.parse().unwrap_or(0.0))
}

/// `stream >> std::hex >> unsigned`: an optional sign and `0x`, then hex
/// digits; no digit gives 0, a value past 32 bits gives `UINT_MAX`, and a
/// minus sign negates as `strtoul` does.
pub fn read_hex_u32(text: &[u8]) -> u32 {
    let mut i = 0;
    while i < text.len() && is_space(text[i]) {
        i += 1;
    }
    let mut negative = false;
    if i < text.len() && (text[i] == b'+' || text[i] == b'-') {
        negative = text[i] == b'-';
        i += 1;
    }
    if i + 1 < text.len() && text[i] == b'0' && (text[i + 1] == b'x' || text[i + 1] == b'X') {
        i += 2;
    }
    let mut value: u64 = 0;
    let mut digits = 0;
    let mut overflow = false;
    while i < text.len() && text[i].is_ascii_hexdigit() {
        let d = u64::from((text[i] as char).to_digit(16).unwrap_or(0));
        value = value * 16 + d;
        if value > u64::from(u32::MAX) {
            overflow = true;
            value = u64::from(u32::MAX);
        }
        digits += 1;
        i += 1;
    }
    if digits == 0 {
        return 0;
    }
    if overflow {
        return u32::MAX;
    }
    let value = value as u32;
    if negative {
        value.wrapping_neg()
    } else {
        value
    }
}
