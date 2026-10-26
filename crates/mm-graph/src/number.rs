//! Real parameter literals, written exactly as Python's `repr(float)` writes them. The library's
//! CXF was generated that way, so matching it lets our emitter reproduce library files byte for
//! byte. Rust's `{:e}` gives the same shortest round-trip digits; only the layout differs.

/// Formats a finite `f64` like Python's `repr`: `86400.0`, `0.3`, `1e-05`, `1e+16`.
/// Returns `None` for NaN and the infinities, which CXF parameters must not carry.
pub fn python_float_repr(value: f64) -> Option<String> {
    if !value.is_finite() {
        return None;
    }
    let sci = format!("{value:e}");
    let (mantissa, exponent) = sci.split_once('e')?;
    let exponent: i32 = exponent.parse().ok()?;
    let (sign, mantissa) = match mantissa.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", mantissa),
    };
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();

    if !(-4..16).contains(&exponent) {
        let (lead, rest) = digits.split_at_checked(1)?;
        let frac = if rest.is_empty() {
            String::new()
        } else {
            format!(".{rest}")
        };
        let exp_sign = if exponent < 0 { '-' } else { '+' };
        return Some(format!(
            "{sign}{lead}{frac}e{exp_sign}{:02}",
            exponent.unsigned_abs()
        ));
    }

    let point = usize::try_from(exponent + 1).unwrap_or(0);
    let text = if exponent < 0 {
        let zeros = "0".repeat(usize::try_from(-exponent - 1).unwrap_or(0));
        format!("0.{zeros}{digits}")
    } else if digits.len() <= point {
        format!("{digits}{}.0", "0".repeat(point - digits.len()))
    } else {
        let (int, frac) = digits.split_at_checked(point)?;
        format!("{int}.{frac}")
    };
    Some(format!("{sign}{text}"))
}

#[cfg(test)]
mod tests {
    use super::python_float_repr;

    #[test]
    fn matches_python_repr() {
        // Expected strings are what CPython's repr() prints for each value.
        let cases: &[(f64, &str)] = &[
            (0.0, "0.0"),
            (-0.0, "-0.0"),
            (1.0, "1.0"),
            (86400.0, "86400.0"),
            (0.3, "0.3"),
            (0.1 + 0.2, "0.30000000000000004"),
            (-273.15, "-273.15"),
            (1e-4, "0.0001"),
            (1e-5, "1e-05"),
            (1.5e-7, "1.5e-07"),
            (123456789012345.6, "123456789012345.6"),
            (1e15, "1000000000000000.0"),
            (1e16, "1e+16"),
            (1.2345e20, "1.2345e+20"),
            (1e100, "1e+100"),
            (f64::MAX, "1.7976931348623157e+308"),
            (5e-324, "5e-324"),
            (19_130_000.0, "19130000.0"),
        ];
        for (value, want) in cases {
            assert_eq!(
                python_float_repr(*value).as_deref(),
                Some(*want),
                "{value:e}"
            );
        }
    }

    #[test]
    fn output_round_trips() {
        for value in [0.1, 2.5e-9, 6.02214076e23, 1.0 / 3.0, -42.125, 7e15] {
            let text = python_float_repr(value).unwrap();
            assert_eq!(text.parse::<f64>().unwrap().to_bits(), value.to_bits());
        }
    }

    #[test]
    fn non_finite_is_refused() {
        assert_eq!(python_float_repr(f64::NAN), None);
        assert_eq!(python_float_repr(f64::INFINITY), None);
        assert_eq!(python_float_repr(f64::NEG_INFINITY), None);
    }
}
