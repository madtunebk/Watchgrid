//! Free text typed by users (names, locations, descriptions): kept as typed,
//! minus the characters nobody types (control characters; PostgreSQL even
//! refuses a zero byte). No escaping: values are always bound as query
//! parameters, never spliced into SQL, and the UI shows them as text.
//!
//! Secrets (passwords) never pass through here: they are stored exactly as
//! typed, or refused (see `cameras::validate`).

/// One line: control characters and line breaks removed, spaces trimmed.
pub fn one_line(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).collect::<String>().trim().to_string()
}

/// Several lines: line breaks kept (`\r\n` → `\n`), tabs as spaces, other
/// control characters removed, trimmed.
pub fn multi_line(s: &str) -> String {
    s.replace("\r\n", "\n")
        .chars()
        .filter_map(|c| match c {
            '\n' => Some('\n'),
            '\t' => Some(' '),
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect::<String>()
        .trim()
        .to_string()
}

pub fn has_control(s: &str) -> bool {
    s.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_what_people_type() {
        assert_eq!(one_line("  Front door's cam \"A\" <1> ; DROP TABLE x; -- "), "Front door's cam \"A\" <1> ; DROP TABLE x; --");
        assert_eq!(one_line("Grădină ✓"), "Grădină ✓");
    }

    #[test]
    fn drops_what_nobody_types() {
        assert_eq!(one_line("Hall\0way\u{7}\n"), "Hallway");
        assert_eq!(multi_line("line 1\r\nline\t2\0\n"), "line 1\nline 2");
        assert!(has_control("pass\0word") && !has_control("p@ss'w\"ord ✓"));
    }
}
