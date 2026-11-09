//! Безопасные пути на устройстве. Проверка выполняется до постановки операции в очередь.

use crate::error::{Error, Result};

const MAX_PATH: usize = 1024;
const MAX_SEGMENT: usize = 255;
const FORBIDDEN: &[char] = &['<', '>', ':', '"', '|', '?', '*', '\\'];

/// Проверяет и нормализует путь вида `E:/Images/a.jpg` (диск Symbian) или `/Images/a.jpg`.
/// Запрещены `..`, `.`-сегменты, пустые сегменты, управляющие символы, `\`, зарезервированные
/// символы Windows/FAT, завершающие точки/пробелы.
pub fn validate_device_path(input: &str) -> Result<String> {
    let bad = |why: &str| Err(Error::UnsafePath(why.to_string()));
    if input.is_empty() {
        return bad("пустой путь");
    }
    if input.len() > MAX_PATH {
        return bad("путь слишком длинный");
    }
    if input.chars().any(|c| c.is_control()) {
        return bad("управляющие символы");
    }
    if input.contains('\\') {
        return bad("используйте / вместо \\");
    }

    let (prefix, rest) = match input.as_bytes() {
        [d, b':', b'/', ..] if d.is_ascii_alphabetic() => (
            format!("{}:/", (*d as char).to_ascii_uppercase()),
            &input[3..],
        ),
        [d, b':'] if d.is_ascii_alphabetic() => {
            return Ok(format!("{}:/", (*d as char).to_ascii_uppercase()));
        }
        [b'/', ..] => ("/".to_string(), &input[1..]),
        _ => return bad("путь должен начинаться с диска (E:/) или /"),
    };

    let mut segments = Vec::new();
    for seg in rest.split('/') {
        if seg.is_empty() {
            // допускается только завершающий '/'
            continue;
        }
        if seg == "." || seg == ".." {
            return bad("сегменты . и .. запрещены");
        }
        if seg.len() > MAX_SEGMENT {
            return bad("слишком длинное имя");
        }
        if seg.chars().any(|c| FORBIDDEN.contains(&c)) {
            return bad("недопустимый символ в имени");
        }
        if seg.ends_with('.') || seg.ends_with(' ') {
            return bad("имя не может заканчиваться точкой или пробелом");
        }
        segments.push(seg);
    }
    if rest.contains("//") {
        return bad("пустой сегмент пути");
    }
    Ok(format!("{prefix}{}", segments.join("/")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_and_normalizes_valid_paths() {
        assert_eq!(
            validate_device_path("e:/Images/a.jpg").unwrap(),
            "E:/Images/a.jpg"
        );
        assert_eq!(validate_device_path("E:/").unwrap(), "E:/");
        assert_eq!(validate_device_path("E:/Images/").unwrap(), "E:/Images");
        assert_eq!(validate_device_path("/Data/x.txt").unwrap(), "/Data/x.txt");
    }

    #[test]
    fn rejects_unsafe_paths() {
        for p in [
            "", "x.txt", "E:/../x", "E:/a/./b", "E:/a//b", "E:\\a", "E:/a\0b", "E:/a?b", "E:/a:b",
            "E:/a.", "E:/a ", "C", "../x",
        ] {
            assert!(
                matches!(validate_device_path(p), Err(Error::UnsafePath(_))),
                "должен быть отвергнут: {p:?}"
            );
        }
        assert!(validate_device_path(&format!("E:/{}", "a".repeat(300))).is_err());
    }
}
