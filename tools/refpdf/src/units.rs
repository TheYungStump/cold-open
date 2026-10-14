//! Fixed-point helpers. Every coordinate the tool stores is an integer
//! number of hundredths of a point ("hpt"), so output never depends on
//! float formatting or rounding differences between platforms.

/// Converts points to hundredths of a point, rounding half away from zero.
/// Non-finite input maps to `None`.
pub fn to_hpt(points: f64) -> Option<i64> {
    let v = (points * 100.0).round();
    if v.is_finite() && v.abs() < 1e15 {
        Some(v as i64)
    } else {
        None
    }
}

/// `n / d` rounded half away from zero. `d` must be positive.
pub fn div_round(n: i64, d: i64) -> i64 {
    debug_assert!(d > 0);
    if n >= 0 {
        (2 * n + d) / (2 * d)
    } else {
        -((-2 * n + d) / (2 * d))
    }
}

/// `n / d` rounded half up (towards +infinity on ties). `d` must be positive.
pub fn div_round_half_up(n: i64, d: i64) -> i64 {
    debug_assert!(d > 0);
    (2 * n + d).div_euclid(2 * d)
}

/// Formats hundredths of a point as a decimal number of points: `1234` → `12.34`.
pub fn fmt_pt(hpt: i64) -> String {
    let sign = if hpt < 0 { "-" } else { "" };
    let a = hpt.unsigned_abs();
    format!("{sign}{}.{:02}", a / 100, a % 100)
}

/// Parses a positive decimal number of points with at most two decimals
/// into hundredths of a point, without going through floats.
pub fn parse_pt(s: &str) -> Result<i64, String> {
    let bad =
        || format!("expected a positive number of points with at most two decimals, got {s:?}");
    let (int, frac) = match s.split_once('.') {
        Some((i, f)) => (i, f),
        None => (s, ""),
    };
    if int.is_empty() && frac.is_empty() {
        return Err(bad());
    }
    if !int.bytes().all(|b| b.is_ascii_digit()) || !frac.bytes().all(|b| b.is_ascii_digit()) {
        return Err(bad());
    }
    if frac.len() > 2 || int.len() > 9 {
        return Err(bad());
    }
    let int: i64 = if int.is_empty() {
        0
    } else {
        int.parse().map_err(|_| bad())?
    };
    let mut f: i64 = if frac.is_empty() {
        0
    } else {
        frac.parse().map_err(|_| bad())?
    };
    if frac.len() == 1 {
        f *= 10;
    }
    let v = int * 100 + f;
    if v <= 0 {
        return Err(bad());
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding_is_half_away_from_zero() {
        assert_eq!(to_hpt(1.005_000_1), Some(101));
        assert_eq!(to_hpt(-0.125), Some(-13));
        assert_eq!(to_hpt(0.125), Some(13));
        assert_eq!(to_hpt(f64::NAN), None);
        assert_eq!(to_hpt(f64::INFINITY), None);
        assert_eq!(div_round(5, 2), 3);
        assert_eq!(div_round(-5, 2), -3);
        assert_eq!(div_round(4, 3), 1);
        assert_eq!(div_round(-4, 3), -1);
        assert_eq!(div_round_half_up(-5, 2), -2);
        assert_eq!(div_round_half_up(5, 2), 3);
        assert_eq!(div_round_half_up(360, 720), 1);
        assert_eq!(div_round_half_up(359, 720), 0);
    }

    #[test]
    fn points_parse_and_format() {
        assert_eq!(parse_pt("7.2"), Ok(720));
        assert_eq!(parse_pt("12"), Ok(1200));
        assert_eq!(parse_pt("7.25"), Ok(725));
        assert_eq!(parse_pt(".5"), Ok(50));
        assert!(parse_pt("7.255").is_err());
        assert!(parse_pt("-7").is_err());
        assert!(parse_pt("0").is_err());
        assert!(parse_pt("").is_err());
        assert!(parse_pt("1e3").is_err());
        assert_eq!(fmt_pt(720), "7.20");
        assert_eq!(fmt_pt(-5), "-0.05");
        assert_eq!(fmt_pt(79200), "792.00");
    }
}
