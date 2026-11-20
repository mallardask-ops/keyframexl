//! CLDR cardinal plural rules for English and Ukrainian, plus safe named
//! placeholder substitution (`fill`).

use super::Language;

/// Unicode CLDR cardinal plural category.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Plural {
    One,
    Few,
    Many,
    Other,
}

/// One message with all four CLDR cardinal forms.
#[derive(Clone, Copy, Debug)]
pub struct PluralStr {
    pub one: &'static str,
    pub few: &'static str,
    pub many: &'static str,
    pub other: &'static str,
}

impl PluralStr {
    pub const fn en(one: &'static str, other: &'static str) -> Self {
        Self {
            one,
            few: other,
            many: other,
            other,
        }
    }

    pub const fn uk(
        one: &'static str,
        few: &'static str,
        many: &'static str,
        other: &'static str,
    ) -> Self {
        Self {
            one,
            few,
            many,
            other,
        }
    }

    pub const fn pick(&self, lang: Language, n: u64) -> &'static str {
        match lang.plural(n) {
            Plural::One => self.one,
            Plural::Few => self.few,
            Plural::Many => self.many,
            Plural::Other => self.other,
        }
    }

    pub fn pick_f64(&self, lang: Language, n: f64) -> &'static str {
        match lang.plural_f64(n) {
            Plural::One => self.one,
            Plural::Few => self.few,
            Plural::Many => self.many,
            Plural::Other => self.other,
        }
    }

    pub fn fmt(&self, lang: Language, n: usize) -> String {
        let count = n.to_string();
        fill(self.pick(lang, n as u64), &[("n", &count)])
    }
}

impl Language {
    /// CLDR cardinal plural category for a non-negative integer.
    pub const fn plural(self, n: u64) -> Plural {
        match self {
            Language::English => {
                if n == 1 {
                    Plural::One
                } else {
                    Plural::Other
                }
            }
            Language::Ukrainian => {
                let (m10, m100) = (n % 10, n % 100);
                if m10 == 1 && m100 != 11 {
                    Plural::One
                } else if matches!(m10, 2..=4) && !matches!(m100, 12..=14) {
                    Plural::Few
                } else if m10 == 0 || matches!(m10, 5..=9) || matches!(m100, 11..=14) {
                    Plural::Many
                } else {
                    Plural::Other
                }
            }
        }
    }

    /// CLDR cardinal plural category for a floating-point quantity (e.g. `1.5` → `Other`).
    pub fn plural_f64(self, n: f64) -> Plural {
        let a = n.abs();
        if !a.is_finite() || a.fract() != 0.0 {
            Plural::Other
        } else {
            self.plural(a as u64)
        }
    }
}

/// Replace `{key}` placeholders in `template` using `args`.
/// Unknown placeholders are kept verbatim and substituted values are never re-scanned.
pub fn fill(template: &str, args: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            out.push_str(&rest[open..]);
            return out;
        };
        let key = &after[..close];
        match args.iter().find(|(k, _)| *k == key) {
            Some((_, value)) => out.push_str(value),
            None => {
                out.push('{');
                out.push_str(key);
                out.push('}');
            }
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}
