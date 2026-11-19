//! Mnemonic markers in catalog strings (`&File`) for menu labels.

/// Visible label and the activation key after an `&` marker.
pub fn menu_mnemonic_parts(text: &str) -> (String, Option<char>) {
    let mut visible = String::new();
    let mut mnemonic = None;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '&' {
            if let Some(&next) = chars.peek() {
                if next != '&' {
                    mnemonic = Some(next.to_ascii_lowercase());
                    visible.push(next);
                    chars.next();
                    continue;
                }
                chars.next();
                visible.push('&');
                continue;
            }
        }
        visible.push(ch);
    }
    (visible, mnemonic)
}

/// First character of the menu category id (e.g. `file` → `f`).
pub fn menu_category_mnemonic(category: &str) -> Option<char> {
    category.chars().next().map(|ch| ch.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ampersand_marks_the_mnemonic_letter() {
        let (label, key) = menu_mnemonic_parts("&File");
        assert_eq!(label, "File");
        assert_eq!(key, Some('f'));
    }

    #[test]
    fn a_doubled_ampersand_stays_literal() {
        let (label, key) = menu_mnemonic_parts("Copy && paste");
        assert_eq!(label, "Copy & paste");
        assert_eq!(key, None);
    }
}
