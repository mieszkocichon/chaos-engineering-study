//! JavaScript-compatible number formatting and JSON printing (research R7).
//! Output files and CLI text are consumed by scripts written against Node's
//! `Number.prototype.toString`, `toFixed` and `JSON.stringify(x, null, 2)`.

use serde_json::Value;

/// `String(x)` for a JS number.
pub fn js_number(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    if x == 0.0 {
        return "0".into(); // also -0
    }
    let sign = if x < 0.0 { "-" } else { "" };
    let (digits, n) = shortest_digits(x.abs());
    let k = digits.len() as i32;
    let body = if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let e = n - 1;
        let es = if e >= 0 { format!("+{e}") } else { e.to_string() };
        if k == 1 {
            format!("{digits}e{es}")
        } else {
            format!("{}.{}e{es}", &digits[..1], &digits[1..])
        }
    };
    format!("{sign}{body}")
}

/// Shortest round-trip digits (no leading/trailing zeros) and the decimal point
/// position `n` such that x = 0.digits × 10^n (ECMAScript Number::toString terms).
fn shortest_digits(x: f64) -> (String, i32) {
    let mut buf = ryu::Buffer::new();
    let s = buf.format_finite(x);
    let (mant, exp) = match s.split_once('e') {
        Some((m, e)) => (m, e.parse::<i32>().unwrap()),
        None => (s, 0),
    };
    let (int_part, frac_part) = mant.split_once('.').unwrap_or((mant, ""));
    let all = format!("{int_part}{frac_part}");
    let lead = all.len() - all.trim_start_matches('0').len();
    let digits = all.trim_start_matches('0').trim_end_matches('0').to_string();
    let n = int_part.len() as i32 - lead as i32 + exp;
    (digits, n)
}

/// `x.toFixed(f)`: like `{:.f}` but ties (exact binary halfway values) round up in magnitude.
pub fn js_to_fixed(x: f64, f: usize) -> String {
    if !x.is_finite() || x.abs() >= 1e21 {
        return js_number(x);
    }
    let sign = if x < 0.0 { "-" } else { "" };
    let a = x.abs();
    // 1100 fractional digits is exact for any f64; detect an exact tie at digit f+1.
    let exact = format!("{a:.1100}");
    let frac = exact.split_once('.').map(|(_, f)| f).unwrap_or("");
    let tail = &frac[f..];
    let is_tie = tail.starts_with('5') && tail[1..].bytes().all(|b| b == b'0');
    let s = if is_tie {
        // truncate to f digits, then add one unit in the last place (decimal string arithmetic)
        let int = exact.split_once('.').map(|(i, _)| i).unwrap_or(&exact);
        let mut d: Vec<u8> = format!("{int}{}", &frac[..f]).into_bytes();
        let mut i = d.len();
        loop {
            if i == 0 {
                d.insert(0, b'1');
                break;
            }
            i -= 1;
            if d[i] == b'9' {
                d[i] = b'0';
            } else {
                d[i] += 1;
                break;
            }
        }
        let d = String::from_utf8(d).unwrap();
        let split = d.len() - f;
        if f == 0 { d } else { format!("{}.{}", &d[..split], &d[split..]) }
    } else {
        format!("{a:.f$}")
    };
    format!("{sign}{s}")
}

/// `JSON.stringify(v, null, 2)`.
pub fn to_js_json_pretty(v: &Value) -> String {
    let mut out = String::new();
    write_value(v, 0, &mut out);
    out
}

fn write_value(v: &Value, depth: usize, out: &mut String) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                out.push_str(&i.to_string())
            } else if let Some(u) = n.as_u64() {
                out.push_str(&u.to_string())
            } else {
                out.push_str(&js_number(n.as_f64().unwrap()))
            }
        }
        Value::String(s) => out.push_str(&serde_json::to_string(s).unwrap()),
        Value::Array(a) if a.is_empty() => out.push_str("[]"),
        Value::Object(o) if o.is_empty() => out.push_str("{}"),
        Value::Array(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                newline(depth + 1, out);
                write_value(x, depth + 1, out);
            }
            newline(depth, out);
            out.push(']');
        }
        Value::Object(o) => {
            out.push('{');
            for (i, (k, x)) in o.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                newline(depth + 1, out);
                out.push_str(&serde_json::to_string(k).unwrap());
                out.push_str(": ");
                write_value(x, depth + 1, out);
            }
            newline(depth, out);
            out.push('}');
        }
    }
}

fn newline(depth: usize, out: &mut String) {
    out.push('\n');
    for _ in 0..depth {
        out.push_str("  ");
    }
}

/// `Math.round`: halves round toward +∞ (`Math.round(-2.5) === -2`), unlike `f64::round`.
pub fn js_round(x: f64) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let f = x.floor();
    if x - f >= 0.5 { f + 1.0 } else { f }
}

/// `Number(x.toFixed(f))`.
pub fn js_fixed_num(x: f64, f: usize) -> f64 {
    js_to_fixed(x, f).parse().unwrap()
}

/// `Number(s)` for a CLI flag value (`true` when the flag had no value → 1).
pub fn js_to_number(s: &str) -> f64 {
    let t = s.trim();
    if t.is_empty() {
        return 0.0;
    }
    match t {
        "Infinity" | "+Infinity" => f64::INFINITY,
        "-Infinity" => f64::NEG_INFINITY,
        _ if t.contains(|c: char| c.is_ascii_alphabetic() && c != 'e' && c != 'E') => f64::NAN,
        _ => t.parse().unwrap_or(f64::NAN),
    }
}

/// `parseInt(s, 10)`: optional sign + leading decimal digits after whitespace; `None` = NaN.
pub fn js_parse_int(s: &str) -> Option<f64> {
    let t = s.trim_start();
    let (sign, rest) = match t.strip_prefix('-') {
        Some(r) => (-1.0, r),
        None => (1.0, t.strip_prefix('+').unwrap_or(t)),
    };
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() { None } else { digits.parse::<f64>().ok().map(|v| sign * v) }
}
