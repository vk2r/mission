use serde_json::Number;

pub fn float_repr(value: f64) -> String {
    if value == 0.0 {
        return if value.is_sign_negative() { "-0.0".to_string() } else { "0.0".to_string() };
    }
    let text = format!("{value}");
    if text.contains('.') { text } else { format!("{text}.0") }
}

pub fn number_str(number: &Number) -> String {
    if number.is_i64() || number.is_u64() {
        number.to_string()
    } else {
        float_repr(number.as_f64().unwrap_or_default())
    }
}

pub fn fixed_two(value: f64) -> String {
    format!("{value:.2}")
}

pub fn round_digits(value: f64, digits: u32) -> f64 {
    if !value.is_finite() {
        return value;
    }
    if let Some(rounded) = exact_round(value, digits) {
        return rounded;
    }
    let factor = 10f64.powi(digits as i32);
    (value * factor).round_ties_even() / factor
}

fn exact_parts(value: f64) -> (bool, u64, i32) {
    let bits = value.to_bits();
    let negative = bits >> 63 == 1;
    let biased = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1u64 << 52) - 1);
    if biased == 0 { (negative, fraction, -1074) } else { (negative, fraction | (1u64 << 52), biased - 1075) }
}

fn exact_round(value: f64, digits: u32) -> Option<f64> {
    let (negative, mantissa, exponent) = exact_parts(value);
    if !(-80..=20).contains(&exponent) || mantissa == 0 {
        return None;
    }

    let scale = 10u128.checked_pow(digits)?;
    let numerator = u128::from(mantissa).checked_mul(scale)?;
    let (scaled, denominator) = if exponent >= 0 {
        (numerator.checked_mul(1u128.checked_shl(exponent as u32)?)?, 1u128)
    } else {
        (numerator, 1u128.checked_shl(u32::try_from(-exponent).ok()?)?)
    };

    let quotient = scaled / denominator;
    let remainder = scaled % denominator;
    let doubled = remainder.checked_mul(2)?;
    let rounded =
        if doubled > denominator || (doubled == denominator && quotient % 2 == 1) { quotient + 1 } else { quotient };

    let magnitude = rounded as f64 / scale as f64;
    Some(if negative { -magnitude } else { magnitude })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_float_keeps_pythons_trailing_zero() {
        assert_eq!(float_repr(3.0), "3.0");
        assert_eq!(float_repr(-0.0), "-0.0");
        assert_eq!(float_repr(0.0), "0.0");
        assert_eq!(float_repr(1.5), "1.5");
        assert_eq!(float_repr(-68.1612), "-68.1612");
        assert_eq!(float_repr(1.0 / 0.593918), format!("{}", 1.0 / 0.593918));
    }

    #[test]
    fn a_catalogue_integer_stays_an_integer() {
        assert_eq!(number_str(json!(85).as_number().expect("number")), "85");
        assert_eq!(number_str(json!(85.54).as_number().expect("number")), "85.54");
        assert_eq!(number_str(json!(85.0).as_number().expect("number")), "85.0");
        assert_eq!(number_str(json!(-0.0).as_number().expect("number")), "-0.0");
    }

    #[test]
    fn two_decimals_match_python() {
        assert_eq!(fixed_two(1.005), "1.00");
        assert_eq!(fixed_two(2.675), "2.67");
        assert_eq!(fixed_two(-0.0), "-0.00");
        assert_eq!(fixed_two(-0.001), "-0.00");
        assert_eq!(fixed_two(85.539), "85.54");
    }

    #[test]
    fn rounding_to_four_places_is_pythons() {
        assert_eq!(round_digits(0.00005, 4), 0.0001);
        assert_eq!(round_digits(0.00015, 4), 0.0001);
        assert_eq!(round_digits(2.5, 0), 2.0);
        assert_eq!(round_digits(3.5, 0), 4.0);
        assert_eq!(round_digits(2.675, 2), 2.67);
        assert!(round_digits(-0.00004, 4).is_sign_negative());
        assert_eq!(fixed_two(round_digits(1.23456789, 4)), "1.23");
    }
}
