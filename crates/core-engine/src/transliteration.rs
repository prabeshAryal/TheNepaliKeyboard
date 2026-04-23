use std::borrow::Cow;

use unicode_normalization::UnicodeNormalization;

pub fn normalize_nepali_word(word: &str) -> String {
    word.nfc().collect::<String>().trim().to_string()
}

pub fn romanize_nepali_word(word: &str) -> String {
    let chars: Vec<char> = normalize_nepali_word(word).chars().collect();
    let mut result = String::new();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];

        if let Some(vowel) = independent_vowel(ch) {
            result.push_str(vowel);
            i += 1;
            continue;
        }

        if let Some(consonant) = consonant(ch) {
            result.push_str(consonant);

            if matches!(chars.get(i + 1), Some('्')) {
                i += 2;
                continue;
            }

            if let Some(sign) = chars.get(i + 1).and_then(|next| vowel_sign(*next)) {
                result.push_str(sign);
                i += 2;
                continue;
            }

            result.push('a');
            i += 1;
            continue;
        }

        if let Some(mark) = standalone_mark(ch) {
            result.push_str(mark);
        } else if ch.is_whitespace() {
            result.push(' ');
        }

        i += 1;
    }

    result
}

pub fn latin_input_key(input: &str) -> String {
    let folded = input.to_lowercase();
    let chars: Vec<char> = folded.chars().collect();
    let mut tokens: Vec<char> = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];
        if ch.is_whitespace() {
            if !matches!(tokens.last(), Some(' ')) {
                tokens.push(' ');
            }
            i += 1;
            continue;
        }

        if !ch.is_ascii_alphabetic() {
            i += 1;
            continue;
        }

        let remaining = &chars[i..];
        let token = if starts_with(remaining, &['x', 'a']) {
            i += 2;
            'C'
        } else if starts_with(remaining, &['c', 'h', 'h']) {
            i += 3;
            'C'
        } else if starts_with(remaining, &['s', 'h']) {
            i += 2;
            'S'
        } else if starts_with(remaining, &['c', 'h']) {
            i += 2;
            'C'
        } else if starts_with(remaining, &['k', 's', 'h']) {
            i += 3;
            'X'
        } else if starts_with(remaining, &['g', 'y']) {
            i += 2;
            'G'
        } else if starts_with(remaining, &['k', 'h']) {
            i += 2;
            'K'
        } else if starts_with(remaining, &['g', 'h']) {
            i += 2;
            'H'
        } else if starts_with(remaining, &['t', 'h']) {
            i += 2;
            'T'
        } else if starts_with(remaining, &['d', 'h']) {
            i += 2;
            'D'
        } else if starts_with(remaining, &['p', 'h']) {
            i += 2;
            'F'
        } else if starts_with(remaining, &['b', 'h']) {
            i += 2;
            'B'
        } else if starts_with(remaining, &['a', 'a']) {
            i += 2;
            'A'
        } else if starts_with(remaining, &['i', 'i']) || starts_with(remaining, &['e', 'e']) {
            i += 2;
            'I'
        } else if starts_with(remaining, &['u', 'u']) || starts_with(remaining, &['o', 'o']) {
            i += 2;
            'U'
        } else if starts_with(remaining, &['a', 'i']) {
            i += 2;
            'E'
        } else if starts_with(remaining, &['a', 'u']) {
            i += 2;
            'O'
        } else if starts_with(remaining, &['r', 'i']) {
            i += 2;
            'R'
        } else {
            i += 1;
            canonical_char(ch)
        };

        if token == '\0' {
            continue;
        }

        if Some(&token) != tokens.last() {
            tokens.push(token);
        }
    }

    let mut reduced = Vec::with_capacity(tokens.len());
    for (idx, token) in tokens.iter().copied().enumerate() {
        if token == 'A' && is_optional_schwa(&tokens, idx) {
            continue;
        }

        reduced.push(token);
    }

    reduced.into_iter().collect::<String>().trim().to_string()
}

pub fn transliteration_key_for_word(word: &str) -> (String, String) {
    let normalized_word = normalize_nepali_word(word);
    let romanized = romanize_nepali_word(&normalized_word);
    let key = latin_input_key(&romanized);
    (romanized, key)
}

pub fn transliterate_latin_fallback(input: &str) -> String {
    let chars: Vec<char> = input.to_lowercase().chars().collect();
    let mut out = String::new();
    let mut i = 0;
    let mut previous_was_consonant = false;

    while i < chars.len() {
        let remaining = &chars[i..];
        if chars[i].is_whitespace() {
            out.push(' ');
            previous_was_consonant = false;
            i += 1;
            continue;
        }

        if let Some((advance, vowel)) = consume_vowel(remaining) {
            if previous_was_consonant {
                let is_terminal = is_terminal_vowel(&chars, i + advance);
                out.push_str(vowel_sign_or_inherent(vowel, is_terminal));
            } else {
                out.push_str(independent_vowel_text(vowel));
            }
            previous_was_consonant = false;
            i += advance;
            continue;
        }

        if let Some((advance, consonant)) = consume_consonant(remaining) {
            out.push_str(consonant);
            previous_was_consonant = true;
            i += advance;
            continue;
        }

        previous_was_consonant = false;
        i += 1;
    }

    out.trim().to_string()
}

fn starts_with(slice: &[char], prefix: &[char]) -> bool {
    slice.len() >= prefix.len() && &slice[..prefix.len()] == prefix
}

fn is_terminal_vowel(chars: &[char], next_index: usize) -> bool {
    chars
        .get(next_index..)
        .is_none_or(|remaining| remaining.iter().all(|ch| ch.is_whitespace()))
}

fn consume_vowel(slice: &[char]) -> Option<(usize, &'static str)> {
    if starts_with(slice, &['a', 'a']) {
        Some((2, "aa"))
    } else if starts_with(slice, &['a', 'i']) {
        Some((2, "ai"))
    } else if starts_with(slice, &['a', 'u']) {
        Some((2, "au"))
    } else if starts_with(slice, &['r', 'i']) {
        Some((2, "ri"))
    } else if starts_with(slice, &['e', 'e']) || starts_with(slice, &['i', 'i']) {
        Some((2, "ii"))
    } else if starts_with(slice, &['o', 'o']) || starts_with(slice, &['u', 'u']) {
        Some((2, "uu"))
    } else {
        match slice.first().copied() {
            Some('a') => Some((1, "a")),
            Some('i') => Some((1, "i")),
            Some('u') => Some((1, "u")),
            Some('e') => Some((1, "e")),
            Some('o') => Some((1, "o")),
            _ => None,
        }
    }
}

fn consume_consonant(slice: &[char]) -> Option<(usize, &'static str)> {
    if starts_with(slice, &['c', 'h', 'h', 'y', 'a']) {
        Some((5, "छ्या"))
    } else if starts_with(slice, &['c', 'h', 'h', 'y']) {
        Some((4, "छ्य"))
    } else if starts_with(slice, &['x', 'a']) {
        Some((2, "छ"))
    } else if starts_with(slice, &['c', 'h', 'h']) {
        Some((3, "छ"))
    } else if starts_with(slice, &['s', 'h']) {
        Some((2, "श"))
    } else if starts_with(slice, &['c', 'h']) {
        Some((2, "च"))
    } else if starts_with(slice, &['k', 's', 'h']) {
        Some((3, "क्ष"))
    } else if starts_with(slice, &['g', 'y']) {
        Some((2, "ज्ञ"))
    } else if starts_with(slice, &['k', 'h']) {
        Some((2, "ख"))
    } else if starts_with(slice, &['g', 'h']) {
        Some((2, "घ"))
    } else if starts_with(slice, &['t', 'h']) {
        Some((2, "थ"))
    } else if starts_with(slice, &['d', 'h']) {
        Some((2, "ध"))
    } else if starts_with(slice, &['p', 'h']) {
        Some((2, "फ"))
    } else if starts_with(slice, &['b', 'h']) {
        Some((2, "भ"))
    } else if starts_with(slice, &['n', 'g']) {
        Some((2, "ङ"))
    } else if starts_with(slice, &['n', 'y']) {
        Some((2, "ञ"))
    } else {
        match slice.first().copied() {
            Some('k') | Some('q') => Some((1, "क")),
            Some('g') => Some((1, "ग")),
            Some('c') => Some((1, "क")),
            Some('j') | Some('z') => Some((1, "ज")),
            Some('t') => Some((1, "त")),
            Some('d') => Some((1, "द")),
            Some('n') => Some((1, "न")),
            Some('p') => Some((1, "प")),
            Some('b') => Some((1, "ब")),
            Some('m') => Some((1, "म")),
            Some('y') => Some((1, "य")),
            Some('r') => Some((1, "र")),
            Some('l') => Some((1, "ल")),
            Some('v') | Some('w') => Some((1, "व")),
            Some('s') => Some((1, "स")),
            Some('h') => Some((1, "ह")),
            Some('f') => Some((1, "फ")),
            Some('x') => Some((1, "क्ष")),
            _ => None,
        }
    }
}

fn independent_vowel_text(vowel: &str) -> &'static str {
    match vowel {
        "a" => "अ",
        "aa" => "आ",
        "i" => "इ",
        "ii" => "ई",
        "u" => "उ",
        "uu" => "ऊ",
        "e" => "ए",
        "o" => "ओ",
        "ai" => "ऐ",
        "au" => "औ",
        "ri" => "ऋ",
        _ => "",
    }
}

fn vowel_sign_or_inherent(vowel: &str, is_terminal: bool) -> &'static str {
    match vowel {
        "a" if is_terminal => "ा",
        "a" => "",
        "aa" => "ा",
        "i" => "ि",
        "ii" => "ी",
        "u" => "ु",
        "uu" => "ू",
        "e" => "े",
        "o" => "ो",
        "ai" => "ै",
        "au" => "ौ",
        "ri" => "ृ",
        _ => "",
    }
}

fn canonical_char(ch: char) -> char {
    match ch {
        'a' => 'A',
        'i' | 'e' => 'I',
        'u' | 'o' => 'U',
        'v' | 'w' | 'b' => 'B',
        's' => 'S',
        'r' => 'R',
        'y' => 'Y',
        'k' | 'q' => 'K',
        'g' => 'G',
        't' => 'T',
        'd' => 'D',
        'p' => 'P',
        'f' => 'F',
        'm' => 'M',
        'n' => 'N',
        'l' => 'L',
        'h' => 'H',
        'j' | 'z' => 'J',
        'x' => 'X',
        'c' => 'C',
        _ => '\0',
    }
}

fn is_optional_schwa(tokens: &[char], idx: usize) -> bool {
    if tokens[idx] != 'A' {
        return false;
    }

    let prev = idx.checked_sub(1).and_then(|pos| tokens.get(pos)).copied();
    let next = tokens.get(idx + 1).copied();

    prev.is_some_and(is_consonant_token)
        && next.is_none_or(|token| token == ' ' || is_consonant_token(token))
}

fn is_consonant_token(token: char) -> bool {
    !matches!(token, 'A' | 'I' | 'U' | 'E' | 'O' | ' ')
}

fn independent_vowel(ch: char) -> Option<&'static str> {
    match ch {
        'अ' => Some("a"),
        'आ' => Some("aa"),
        'इ' => Some("i"),
        'ई' => Some("ii"),
        'उ' => Some("u"),
        'ऊ' => Some("uu"),
        'ऋ' => Some("ri"),
        'ए' => Some("e"),
        'ऐ' => Some("ai"),
        'ओ' => Some("o"),
        'औ' => Some("au"),
        _ => None,
    }
}

fn vowel_sign(ch: char) -> Option<&'static str> {
    match ch {
        'ा' => Some("aa"),
        'ि' => Some("i"),
        'ी' => Some("ii"),
        'ु' => Some("u"),
        'ू' => Some("uu"),
        'ृ' => Some("ri"),
        'े' => Some("e"),
        'ै' => Some("ai"),
        'ो' => Some("o"),
        'ौ' => Some("au"),
        _ => None,
    }
}

fn consonant(ch: char) -> Option<&'static str> {
    match ch {
        'क' => Some("k"),
        'ख' => Some("kh"),
        'ग' => Some("g"),
        'घ' => Some("gh"),
        'ङ' => Some("ng"),
        'च' => Some("ch"),
        'छ' => Some("chh"),
        'ज' => Some("j"),
        'झ' => Some("jh"),
        'ञ' => Some("ny"),
        'ट' => Some("t"),
        'ठ' => Some("th"),
        'ड' => Some("d"),
        'ढ' => Some("dh"),
        'ण' => Some("n"),
        'त' => Some("t"),
        'थ' => Some("th"),
        'द' => Some("d"),
        'ध' => Some("dh"),
        'न' => Some("n"),
        'प' => Some("p"),
        'फ' => Some("ph"),
        'ब' => Some("b"),
        'भ' => Some("bh"),
        'म' => Some("m"),
        'य' => Some("y"),
        'र' => Some("r"),
        'ल' => Some("l"),
        'व' => Some("v"),
        'श' | 'ष' => Some("sh"),
        'स' => Some("s"),
        'ह' => Some("h"),
        _ => None,
    }
}

fn standalone_mark(ch: char) -> Option<&'static str> {
    match ch {
        'ं' => Some("m"),
        'ँ' => Some("n"),
        'ः' => Some("h"),
        'ऽ' => Some(""),
        _ => None,
    }
}

pub fn edit_distance_units(left: &str, right: &str) -> usize {
    let left_units = tokenize_key(left);
    let right_units = tokenize_key(right);
    levenshtein(&left_units, &right_units)
}

fn tokenize_key(input: &str) -> Vec<Cow<'_, str>> {
    input.chars().map(|ch| Cow::Owned(ch.to_string())).collect()
}

fn levenshtein(left: &[Cow<'_, str>], right: &[Cow<'_, str>]) -> usize {
    if left.is_empty() {
        return right.len();
    }
    if right.is_empty() {
        return left.len();
    }

    let mut prev: Vec<usize> = (0..=right.len()).collect();
    let mut curr = vec![0; right.len() + 1];

    for (i, left_unit) in left.iter().enumerate() {
        curr[0] = i + 1;

        for (j, right_unit) in right.iter().enumerate() {
            let substitution = if left_unit == right_unit { 0 } else { 1 };
            curr[j + 1] = (prev[j + 1] + 1)
                .min(curr[j] + 1)
                .min(prev[j] + substitution);
        }

        prev.clone_from_slice(&curr);
    }

    prev[right.len()]
}

#[cfg(test)]
mod tests {
    use super::{
        edit_distance_units, latin_input_key, transliterate_latin_fallback,
        transliteration_key_for_word,
    };

    #[test]
    fn creates_a_stable_key_for_fuzzy_input() {
        assert_eq!(latin_input_key("prabesh"), latin_input_key("parbesh"));
        assert_eq!(latin_input_key("shikshya"), latin_input_key("sikshya"));
        assert_eq!(latin_input_key("chha"), latin_input_key("cha"));
        assert_eq!(latin_input_key("xa"), latin_input_key("chha"));
    }

    #[test]
    fn romanizes_known_words() {
        let (romanized, key) = transliteration_key_for_word("प्रवेश");
        assert_eq!(romanized, "pravesha");
        assert_eq!(key, latin_input_key("prabesh"));
    }

    #[test]
    fn edit_distance_works_on_transliteration_units() {
        assert!(edit_distance_units(&latin_input_key("prabesh"), &latin_input_key("pravesh")) <= 1);
    }

    #[test]
    fn builds_a_fallback_for_unknown_input() {
        assert_eq!(transliterate_latin_fallback("moiz"), "मोइज");
        assert_eq!(transliterate_latin_fallback("prabesh"), "परबेश");
        assert_eq!(transliterate_latin_fallback("xa"), "छ");
        assert_eq!(transliterate_latin_fallback("chhya"), "छ्या");
        assert_eq!(transliterate_latin_fallback("x"), "क्ष");
        assert_eq!(transliterate_latin_fallback("rama"), "रमा");
    }
}
