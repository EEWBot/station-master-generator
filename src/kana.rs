//! Hiragana to katakana normalization.
//!
//! The code table spells readings in hiragana while station.json uses katakana.
//! This is a fixed per-character mapping, not a guess: every hiragana in
//! U+3041..=U+3096 has a katakana counterpart exactly 0x60 code points higher.
//! Characters outside that range (including the iteration marks handled below)
//! are left alone.

/// Convert every hiragana in `s` to its katakana counterpart.
pub fn to_katakana(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            // Ordinary hiragana block, small kana included.
            '\u{3041}'..='\u{3096}' => char::from_u32(c as u32 + 0x60).unwrap_or(c),
            // Iteration marks live just outside that range but pair up the same way.
            '\u{309D}' => '\u{30FD}',
            '\u{309E}' => '\u{30FE}',
            other => other,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::to_katakana;

    #[test]
    fn converts_plain_hiragana() {
        assert_eq!(to_katakana("こうのしやまかわ"), "コウノシヤマカワ");
    }

    #[test]
    fn converts_small_kana_and_voiced_marks() {
        assert_eq!(
            to_katakana("しんしのつむらだいよんじゅうななせん"),
            "シンシノツムラダイヨンジュウナナセン"
        );
        assert_eq!(to_katakana("こうのしばんなぐろ"), "コウノシバンナグロ");
    }

    #[test]
    fn leaves_katakana_and_other_characters_untouched() {
        assert_eq!(to_katakana("コウノシヤマカワ"), "コウノシヤマカワ");
        assert_eq!(to_katakana("ABC 123 甲野市"), "ABC 123 甲野市");
    }

    #[test]
    fn converts_iteration_marks() {
        assert_eq!(to_katakana("ゝゞ"), "ヽヾ");
    }

    #[test]
    fn is_idempotent() {
        let once = to_katakana("こうのしやまかわ");
        assert_eq!(to_katakana(&once), once);
    }
}
