//! Flag parser identical to `args()` in the JS CLIs: `--k v`, or `--k` alone (or before
//! another `--flag`) means `true`.

use std::collections::HashMap;

use crate::js::js_to_number;

#[derive(Clone, Debug, PartialEq)]
pub enum Arg {
    Flag,
    Value(String),
}

pub struct Args(HashMap<String, Arg>);

impl Args {
    pub fn parse(argv: impl IntoIterator<Item = String>) -> Self {
        let a: Vec<String> = argv.into_iter().collect();
        let mut o = HashMap::new();
        let mut i = 0;
        while i < a.len() {
            if let Some(k) = a[i].strip_prefix("--") {
                if i + 1 < a.len() && !a[i + 1].starts_with("--") {
                    o.insert(k.to_string(), Arg::Value(a[i + 1].clone()));
                    i += 1;
                } else {
                    o.insert(k.to_string(), Arg::Flag);
                }
            }
            i += 1;
        }
        Args(o)
    }

    pub fn from_env() -> Self {
        Self::parse(std::env::args().skip(1))
    }

    /// `num(k, d)`: `Number(o[k])` or the default.
    pub fn num(&self, k: &str, d: f64) -> f64 {
        match self.0.get(k) {
            None => d,
            Some(Arg::Flag) => 1.0,
            Some(Arg::Value(v)) => js_to_number(v),
        }
    }

    /// Truthiness of `o[k]` (any present flag/value except the empty string).
    pub fn is_set(&self, k: &str) -> bool {
        match self.0.get(k) {
            None => false,
            Some(Arg::Flag) => true,
            Some(Arg::Value(v)) => !v.is_empty(),
        }
    }

    /// `o[k] === 'true' || o[k] === true`.
    pub fn is_true(&self, k: &str) -> bool {
        matches!(self.0.get(k), Some(Arg::Flag)) || matches!(self.0.get(k), Some(Arg::Value(v)) if v == "true")
    }

    pub fn str_or<'a>(&'a self, k: &str, d: &'a str) -> &'a str {
        match self.0.get(k) {
            Some(Arg::Value(v)) if !v.is_empty() => v,
            _ => d,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_like_js() {
        let a = Args::parse(["--seed", "3", "--json", "--surge", "--x", "-5", "stray"].map(String::from));
        assert_eq!(a.num("seed", 1.0), 3.0);
        assert!(a.is_set("json"));
        assert_eq!(a.num("surge", 7.0), 1.0); // `--surge` followed by `--x` → true → Number(true)
        assert_eq!(a.num("x", 0.0), -5.0);
        assert_eq!(a.num("missing", 9.0), 9.0);
        assert!(!a.is_true("seed"));
    }
}
