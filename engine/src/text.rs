//! The canonical text forms the C++ oracle prints, so dumps and digests from
//! the engine compare byte for byte with the oracle's.

use std::fmt::Write;

/// `printf("%.9g", double(value))` as the MSVC runtime prints it. Nine
/// significant digits round-trip any `f32`.
pub fn format_float(value: f32) -> String {
    const PRECISION: i32 = 9;
    let value = f64::from(value);
    if value.is_nan() {
        return if value.is_sign_negative() {
            "-nan(ind)"
        } else {
            "nan"
        }
        .to_owned();
    }
    if value.is_infinite() {
        return if value < 0.0 { "-inf" } else { "inf" }.to_owned();
    }
    if value == 0.0 {
        return if value.is_sign_negative() { "-0" } else { "0" }.to_owned();
    }
    let scientific = format!("{:.*e}", (PRECISION - 1) as usize, value);
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((&scientific, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    if (-4..PRECISION).contains(&exponent) {
        let decimals = (PRECISION - 1 - exponent) as usize;
        strip_zeros(&format!("{value:.decimals$}")).to_owned()
    } else {
        let mut result = strip_zeros(mantissa).to_owned();
        let sign = if exponent < 0 { '-' } else { '+' };
        let _ = write!(result, "e{sign}{:02}", exponent.abs());
        result
    }
}

fn strip_zeros(number: &str) -> &str {
    if number.contains('.') {
        number.trim_end_matches('0').trim_end_matches('.')
    } else {
        number
    }
}

/// UTF-16 code units as the oracle prints them: `[5c 66 72]`.
pub fn format_units(units: impl IntoIterator<Item = u16>) -> String {
    let mut result = String::from("[");
    for (index, unit) in units.into_iter().enumerate() {
        if index > 0 {
            result.push(' ');
        }
        let _ = write!(result, "{unit:x}");
    }
    result.push(']');
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_like_printf() {
        assert_eq!(format_float(0.0), "0");
        assert_eq!(format_float(-0.0), "-0");
        assert_eq!(format_float(1.0), "1");
        assert_eq!(format_float(1024.0), "1024");
        assert_eq!(format_float(0.430555), "0.430554986");
        assert_eq!(format_float(0.0001), "9.99999975e-05");
        assert_eq!(format_float(123456789.0), "123456792");
        assert_eq!(format_float(1.0e10), "1e+10");
        assert_eq!(format_float(-2.5e-7), "-2.49999999e-07");
        assert_eq!(format_float(f32::INFINITY), "inf");
    }

    #[test]
    fn formats_units() {
        assert_eq!(format_units([0x5c, 0x66]), "[5c 66]");
        assert_eq!(format_units([]), "[]");
    }
}
