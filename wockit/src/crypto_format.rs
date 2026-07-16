//! Shared `$WOC` formatting kept independent from panel presentation.

pub fn price(raw: &str) -> String {
    format!("${raw}")
}

pub fn signed_change(change: f64) -> String {
    let formatted = one_decimal(change);
    if change >= 0.0 {
        format!("+{formatted}%")
    } else {
        format!("{formatted}%")
    }
}

pub(crate) fn one_decimal(value: f64) -> String {
    if value.is_nan() {
        "nan".into()
    } else {
        format!("{value:.1}")
    }
}

/// Four significant figures in fixed point, capped at twelve decimal places.
pub fn chart_price(value: f64) -> String {
    if !(value > 0.0 && value.is_finite()) {
        return four_significant_general(value);
    }
    let exponent = value.log10().floor() as i32;
    let decimals = (3 - exponent).clamp(0, 12) as usize;
    format!("{value:.decimals$}")
}

/// The observable output of C/Foundation's `%.4g`, used by the Swift fallback path.
fn four_significant_general(value: f64) -> String {
    if value.is_nan() {
        return "nan".into();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-inf"
        } else {
            "inf"
        }
        .into();
    }
    if value == 0.0 {
        return if value.is_sign_negative() { "-0" } else { "0" }.into();
    }

    let exponent = value.abs().log10().floor() as i32;
    if (-4..4).contains(&exponent) {
        let decimals = (3 - exponent).max(0) as usize;
        return format!("{value:.decimals$}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_owned();
    }

    let scientific = format!("{value:.3e}");
    let (mantissa, exponent) = scientific.split_once('e').expect("Rust scientific format");
    let mantissa = mantissa.trim_end_matches('0').trim_end_matches('.');
    let exponent_value: i32 = exponent.parse().expect("Rust scientific exponent");
    format!("{mantissa}e{exponent_value:+03}")
}
