//! Number parsing as MicroTeX's `valueof()` does it: `std::stringstream >>`
//! with the MSVC standard library in the classic locale.

/// Classic-locale `isspace`.
fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t'..=b'\r')
}

/// `stream >> float`. Fields are gathered as `num_get` gathers them: white
/// space, a sign, digits with one decimal point, then an exponent. Without a
/// digit (in the mantissa, or after an exponent marker) the read fails and
/// gives 0. Otherwise the value is the correctly rounded `strtof` of the
/// field, infinity on overflow included. A `0x` prefix reads a hexadecimal
/// float with a binary `p` exponent the same way.
pub fn read_float(text: &[u8]) -> f32 {
    let ok = |value: f32| value;
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
            let value = hex_float(&text[i + 1..]);
            return if number.starts_with('-') {
                -value
            } else {
                value
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

/// The hexadecimal digits and `p` exponent after a `0x`, correctly rounded
/// to `f32` as `strtof` rounds them: half to even, with the digits past the
/// precision kept as a sticky bit, overflow to infinity and underflow
/// through the subnormals to 0.
fn hex_float(text: &[u8]) -> f32 {
    let digit = |b: u8| char::from(b).to_digit(16).map(u128::from);
    let mut mantissa: u128 = 0;
    let mut sticky = false;
    let mut exponent: i64 = 0;
    let mut digits = 0;
    let mut i = 0;
    let mut fraction = false;
    loop {
        if i < text.len() && text[i] == b'.' && !fraction {
            fraction = true;
            i += 1;
            continue;
        }
        let Some(d) = text.get(i).copied().and_then(digit) else {
            break;
        };
        digits += 1;
        if mantissa < 1 << 120 {
            mantissa = mantissa * 16 + d;
            if fraction {
                exponent -= 4;
            }
        } else {
            sticky |= d != 0;
            if !fraction {
                exponent += 4;
            }
        }
        i += 1;
    }
    if digits == 0 {
        return 0.0;
    }
    if i < text.len() && (text[i] == b'p' || text[i] == b'P') {
        i += 1;
        let mut negative = false;
        if i < text.len() && (text[i] == b'+' || text[i] == b'-') {
            negative = text[i] == b'-';
            i += 1;
        }
        let mut value: i64 = 0;
        let mut seen = false;
        while i < text.len() && text[i].is_ascii_digit() {
            value = (value * 10 + i64::from(text[i] - b'0')).min(1 << 40);
            seen = true;
            i += 1;
        }
        if !seen {
            return 0.0;
        }
        exponent += if negative { -value } else { value };
    }
    if mantissa == 0 {
        return 0.0;
    }
    let bits = i64::from(128 - mantissa.leading_zeros());
    let top = bits - 1 + exponent;
    if top > 127 {
        return f32::INFINITY;
    }
    if top < -151 {
        return 0.0;
    }
    let lsb = (top - 23).max(-149);
    let shift = lsb - exponent;
    let rounded = if shift <= 0 {
        mantissa << (-shift)
    } else if shift > 128 {
        0
    } else {
        let quotient = if shift == 128 { 0 } else { mantissa >> shift };
        let remainder = if shift == 128 {
            mantissa
        } else {
            mantissa & ((1u128 << shift) - 1)
        };
        let half = 1u128 << (shift - 1);
        let up = remainder > half || (remainder == half && (sticky || quotient & 1 == 1));
        quotient + u128::from(up)
    };
    ((rounded as f64) * 2f64.powi(lsb as i32)) as f32
}
