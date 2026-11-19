//! Lyrics → what a voicebank is asked for. A note carries free text; a UTAU
//! bank wants a kana alias (`か`), so Latin (romaji) and Cyrillic input is
//! converted, katakana folds into hiragana, and `+` / `ー` mean "keep the
//! vowel of the previous note".

/// What one note's lyric asks for.
#[derive(Clone, Debug, PartialEq)]
pub enum Token {
    /// Silence: `-`, `R`, `r`, empty.
    Rest,
    /// Keep singing the previous syllable (`+`, `ー`, `~`).
    Hold,
    /// Syllables sung inside the note, one after another.
    Sing(Vec<String>),
}

const VOWELS: [&str; 5] = ["a", "i", "u", "e", "o"];

/// Gojūon rows: consonant → kana for a i u e o.
const ROWS: &[(&str, [&str; 5])] = &[
    ("", ["あ", "い", "う", "え", "お"]),
    ("k", ["か", "き", "く", "け", "こ"]),
    ("g", ["が", "ぎ", "ぐ", "げ", "ご"]),
    ("s", ["さ", "し", "す", "せ", "そ"]),
    ("z", ["ざ", "じ", "ず", "ぜ", "ぞ"]),
    ("t", ["た", "ち", "つ", "て", "と"]),
    ("d", ["だ", "ぢ", "づ", "で", "ど"]),
    ("n", ["な", "に", "ぬ", "ね", "の"]),
    ("h", ["は", "ひ", "ふ", "へ", "ほ"]),
    ("b", ["ば", "び", "ぶ", "べ", "ぼ"]),
    ("p", ["ぱ", "ぴ", "ぷ", "ぺ", "ぽ"]),
    ("m", ["ま", "み", "む", "め", "も"]),
    ("r", ["ら", "り", "る", "れ", "ろ"]),
    ("y", ["や", "い", "ゆ", "いぇ", "よ"]),
    ("w", ["わ", "うぃ", "う", "うぇ", "を"]),
    ("f", ["ふぁ", "ふぃ", "ふ", "ふぇ", "ふぉ"]),
    ("v", ["ゔぁ", "ゔぃ", "ゔ", "ゔぇ", "ゔぉ"]),
    ("j", ["じゃ", "じ", "じゅ", "じぇ", "じょ"]),
    ("sh", ["しゃ", "し", "しゅ", "しぇ", "しょ"]),
    ("ch", ["ちゃ", "ち", "ちゅ", "ちぇ", "ちょ"]),
    ("ts", ["つぁ", "つぃ", "つ", "つぇ", "つぉ"]),
    ("th", ["てゃ", "てぃ", "てゅ", "てぇ", "てょ"]),
    ("dh", ["でゃ", "でぃ", "でゅ", "でぇ", "でょ"]),
    ("l", ["ら", "り", "る", "れ", "ろ"]),
    ("x", ["ぁ", "ぃ", "ぅ", "ぇ", "ぉ"]),
];

/// Consonant + y + a/u/o digraphs: the i-row kana plus a small ya/yu/yo.
const DIGRAPH_BASES: &[(&str, &str)] = &[
    ("ky", "き"),
    ("gy", "ぎ"),
    ("ny", "に"),
    ("hy", "ひ"),
    ("by", "び"),
    ("py", "ぴ"),
    ("my", "み"),
    ("ry", "り"),
    ("ly", "り"),
    ("sy", "し"),
    ("zy", "じ"),
    ("ty", "ち"),
    ("dy", "ぢ"),
    ("fy", "ふ"),
    ("vy", "ゔ"),
    ("jy", "じ"),
];

/// Kana for a romaji syllable at the start of `s`, and how many bytes it took.
fn romaji_head(s: &str) -> Option<(String, usize)> {
    // Digraphs: kya, gyu …
    for (cons, base) in DIGRAPH_BASES {
        if let Some(rest) = s.strip_prefix(cons) {
            for (v, small) in [("a", "ゃ"), ("u", "ゅ"), ("o", "ょ"), ("e", "ぇ")] {
                if rest.starts_with(v) {
                    return Some((format!("{base}{small}"), cons.len() + 1));
                }
            }
        }
    }
    // Longest consonant first (sh, ch, ts, th, dh before s, c, t, d, h).
    let mut rows: Vec<&(&str, [&str; 5])> = ROWS.iter().filter(|(c, _)| !c.is_empty()).collect();
    rows.sort_by_key(|(c, _)| std::cmp::Reverse(c.len()));
    for (cons, kana) in rows {
        if let Some(rest) = s.strip_prefix(cons) {
            for (i, v) in VOWELS.iter().enumerate() {
                if rest.starts_with(v) {
                    return Some((kana[i].to_string(), cons.len() + 1));
                }
            }
        }
    }
    // Aliases: si → shi, ti → chi, tu → tsu, hu → fu, zi → ji, c + vowel.
    for (alias, kana) in [
        ("si", "し"),
        ("ti", "ち"),
        ("tu", "つ"),
        ("hu", "ふ"),
        ("zi", "じ"),
        ("ca", "か"),
        ("ci", "し"),
        ("cu", "く"),
        ("ce", "せ"),
        ("co", "こ"),
        ("qa", "くぁ"),
        ("q", "く"),
    ] {
        if s.starts_with(alias) {
            return Some((kana.to_string(), alias.len()));
        }
    }
    for (i, v) in VOWELS.iter().enumerate() {
        if s.starts_with(v) {
            return Some((ROWS[0].1[i].to_string(), 1));
        }
    }
    None
}

/// Ukrainian / Russian letters → romaji, then the same path. Approximate by
/// nature (Japanese has no `l`, `v` or consonant clusters); good enough to
/// get a recognisable line out of a Japanese bank.
fn cyrillic_to_romaji(s: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = s.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        let lower: String = c.to_lowercase().collect();
        let r = match lower.as_str() {
            "а" => "a",
            "б" => "b",
            "в" => "v",
            "г" | "ґ" => "g",
            "д" => "d",
            "е" | "э" | "є" => {
                if matches!(lower.as_str(), "є") {
                    "ye"
                } else {
                    "e"
                }
            }
            "ё" => "yo",
            "ж" => "ji",
            "з" => "z",
            "и" | "і" | "ы" => "i",
            "ї" => "yi",
            "й" => "i",
            "к" => "k",
            "л" => "r",
            "м" => "m",
            "н" => "n",
            "о" => "o",
            "п" => "p",
            "р" => "r",
            "с" => "s",
            "т" => "t",
            "у" => "u",
            "ф" => "f",
            "х" => "h",
            "ц" => "tsu",
            "ч" => "chi",
            "ш" | "щ" => "shi",
            "ю" => "yu",
            "я" => "ya",
            "ь" | "ъ" | "'" | "’" => "",
            _ => "",
        };
        out.push_str(r);
        // A consonant with no vowel after it gets one, so it can be sung.
        let consonant_only = matches!(
            r,
            "b" | "d" | "g" | "k" | "m" | "n" | "p" | "r" | "s" | "t" | "v" | "z" | "f" | "h"
        );
        if consonant_only {
            let next_is_vowel = chars
                .get(i + 1)
                .is_some_and(|n| "аеёєиіїоуюяэы".contains(n.to_lowercase().next().unwrap_or(' ')));
            let next_is_soft = chars.get(i + 1).is_some_and(|n| matches!(n, 'ь' | 'ъ'));
            if !next_is_vowel && r != "n" {
                out.push(if matches!(r, "t" | "d") { 'o' } else { 'u' });
            } else if !next_is_vowel && r == "n" && (i + 1 < chars.len()) && !next_is_soft {
                // «н» before another consonant is ん: leave it bare, the splitter turns it into ん.
            }
        }
    }
    out
}

fn is_cyrillic(c: char) -> bool {
    ('\u{0400}'..='\u{04FF}').contains(&c)
}

fn katakana_to_hiragana(c: char) -> char {
    match c {
        '\u{30A1}'..='\u{30F6}' => char::from_u32(c as u32 - 0x60).unwrap_or(c),
        _ => c,
    }
}

fn is_kana(c: char) -> bool {
    matches!(c, '\u{3041}'..='\u{3096}' | '\u{30A1}'..='\u{30FA}' | 'ー')
}

fn is_small(c: char) -> bool {
    matches!(c, 'ゃ' | 'ゅ' | 'ょ' | 'ぁ' | 'ぃ' | 'ぅ' | 'ぇ' | 'ぉ')
}

/// A bank may spell the same sound `ヴ`, `ゔ` or `ぶ`; the first spelling that
/// exists wins, so each syllable carries its variants (hiragana first).
pub fn spellings(kana: &str) -> Vec<String> {
    let kata: String = kana
        .chars()
        .map(|c| match c {
            '\u{3041}'..='\u{3096}' => char::from_u32(c as u32 + 0x60).unwrap_or(c),
            _ => c,
        })
        .collect();
    let mut v = vec![kana.to_string()];
    if kata != kana {
        v.push(kata);
    }
    v
}

/// Split free text into syllables the bank can be asked for.
fn syllables(text: &str) -> Vec<String> {
    let mut latin = String::new();
    let mut out: Vec<String> = Vec::new();
    let flush = |latin: &mut String, out: &mut Vec<String>| {
        let mut s = std::mem::take(latin).to_lowercase();
        while !s.is_empty() {
            if let Some((k, n)) = romaji_head(&s) {
                out.push(k);
                s = s[n..].to_string();
            } else if s.starts_with("nn") {
                // konnichiha: the second n belongs to the next syllable (ni); a lone nn is ん.
                out.push("ん".into());
                let rest_is_syllable = s[2..].starts_with(['a', 'i', 'u', 'e', 'o', 'y']);
                s = s[if rest_is_syllable { 1 } else { 2 }..].to_string();
            } else if s.starts_with('n') {
                out.push("ん".into());
                s = s[1..].to_string();
            } else {
                let c = s.chars().next().unwrap();
                // A doubled consonant (kk, tt) is a small tsu: no sound of its own.
                s = s[c.len_utf8()..].to_string();
            }
        }
    };
    let text: String = if text.chars().any(is_cyrillic) {
        text.chars()
            .map(|c| {
                if is_cyrillic(c) {
                    cyrillic_to_romaji(&c.to_string())
                } else {
                    c.to_string()
                }
            })
            .collect::<String>()
    } else {
        text.to_string()
    };
    // Cyrillic runs are transliterated per word so consonant clusters get their vowels.
    let text = text.replace('\'', "");
    for c in text.chars() {
        if c.is_ascii_alphabetic() {
            latin.push(c);
            continue;
        }
        flush(&mut latin, &mut out);
        if c == 'っ'
            || c == 'ッ'
            || c.is_whitespace()
            || c == '-'
            || c == '_'
            || c == ','
            || c == '.'
        {
            continue;
        }
        if c == 'ー' {
            out.push("ー".into());
            continue;
        }
        let h = katakana_to_hiragana(c);
        if is_small(h)
            && let Some(last) = out.last_mut()
            && last != "ー"
        {
            // きゃ: the small kana joins the one before it.
            if !last.ends_with(|x| is_small(x)) || matches!(h, 'ゃ' | 'ゅ' | 'ょ') {
                last.push(h);
                continue;
            }
        }
        if is_kana(c) || is_kana(h) || ('\u{3040}'..='\u{309F}').contains(&h) {
            out.push(h.to_string());
        } else {
            // Kanji or anything else: pass through, the bank decides if it knows it.
            out.push(c.to_string());
        }
    }
    flush(&mut latin, &mut out);
    out
}

pub fn parse(lyric: &str) -> Token {
    let t = lyric.trim();
    match t {
        "" | "-" | "R" | "r" | "rest" | "Rest" | "・" => return Token::Rest,
        "+" | "ー" | "~" | "+~" | "＋" => return Token::Hold,
        _ => {}
    }
    let s = syllables(t);
    // A leading ー inside a text means hold too.
    let s: Vec<String> = s;
    if s.is_empty() {
        return Token::Rest;
    }
    Token::Sing(s)
}

/// The vowel a kana ends on: what a VCV bank puts before the next syllable.
pub fn vowel_of(kana: &str) -> Option<&'static str> {
    let c = kana.chars().last()?;
    let h = katakana_to_hiragana(c);
    let v = match h {
        'あ' | 'か' | 'が' | 'さ' | 'ざ' | 'た' | 'だ' | 'な' | 'は' | 'ば' | 'ぱ' | 'ま'
        | 'や' | 'ら' | 'わ' | 'ゃ' | 'ぁ' => "a",
        'い' | 'き' | 'ぎ' | 'し' | 'じ' | 'ち' | 'ぢ' | 'に' | 'ひ' | 'び' | 'ぴ' | 'み'
        | 'り' | 'ぃ' => "i",
        'う' | 'く' | 'ぐ' | 'す' | 'ず' | 'つ' | 'づ' | 'ぬ' | 'ふ' | 'ぶ' | 'ぷ' | 'む'
        | 'ゆ' | 'る' | 'ゅ' | 'ぅ' | 'ゔ' => "u",
        'え' | 'け' | 'げ' | 'せ' | 'ぜ' | 'て' | 'で' | 'ね' | 'へ' | 'べ' | 'ぺ' | 'め'
        | 'れ' | 'ぇ' => "e",
        'お' | 'こ' | 'ご' | 'そ' | 'ぞ' | 'と' | 'ど' | 'の' | 'ほ' | 'ぼ' | 'ぽ' | 'も'
        | 'よ' | 'ろ' | 'を' | 'ょ' | 'ぉ' => "o",
        'ん' => "n",
        _ => return None,
    };
    Some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sing(s: &str) -> Vec<String> {
        match parse(s) {
            Token::Sing(v) => v,
            t => panic!("{t:?}"),
        }
    }

    #[test]
    fn kana_passes_and_katakana_folds() {
        assert_eq!(sing("か"), ["か"]);
        assert_eq!(sing("カ"), ["か"]);
        assert_eq!(sing("きゃ"), ["きゃ"]);
        assert_eq!(sing("キャ"), ["きゃ"]);
        assert_eq!(sing("こんにちは"), ["こ", "ん", "に", "ち", "は"]);
    }

    #[test]
    fn romaji_becomes_kana() {
        assert_eq!(sing("ka"), ["か"]);
        assert_eq!(sing("shi"), ["し"]);
        assert_eq!(sing("tsu"), ["つ"]);
        assert_eq!(sing("kya"), ["きゃ"]);
        assert_eq!(sing("konnichiha"), ["こ", "ん", "に", "ち", "は"]);
        assert_eq!(sing("a"), ["あ"]);
        assert_eq!(sing("n"), ["ん"]);
        assert_eq!(sing("chu"), ["ちゅ"]);
        assert_eq!(sing("fu"), ["ふ"]);
    }

    #[test]
    fn rests_and_holds() {
        assert_eq!(parse("-"), Token::Rest);
        assert_eq!(parse("R"), Token::Rest);
        assert_eq!(parse(""), Token::Rest);
        assert_eq!(parse("+"), Token::Hold);
        assert_eq!(parse("ー"), Token::Hold);
    }

    #[test]
    fn cyrillic_gets_a_sung_approximation() {
        let v = sing("привіт");
        assert!(!v.is_empty());
        assert!(v.iter().all(|s| s.chars().all(|c| is_kana(c))), "{v:?}");
    }

    #[test]
    fn vowels() {
        assert_eq!(vowel_of("か"), Some("a"));
        assert_eq!(vowel_of("きゃ"), Some("a"));
        assert_eq!(vowel_of("し"), Some("i"));
    }
}
