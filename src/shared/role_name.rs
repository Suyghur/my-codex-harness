//! Role names use a portable ASCII filename alphabet on supported platforms.
use anyhow::{Result, ensure};

pub fn validate_role_name(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty()
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')),
        "非法角色名称 {name:?}：仅允许 ASCII 字母、数字、下划线和连字符"
    );
    Ok(())
}

/// Names that differ only in ASCII case can collide on supported filesystems.
pub fn portable_name_key(name: &str) -> String {
    name.to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_ascii_names_and_keys() {
        for name in ["reader", "Reader_2", "READ-ONLY", "1", "_", "-"] {
            validate_role_name(name).unwrap();
        }
        assert_eq!(portable_name_key("Reader_2"), "reader_2");
        assert_eq!(portable_name_key("READER"), portable_name_key("reader"));
    }

    #[test]
    fn nonportable_names_are_rejected() {
        for name in [
            "", ".", "..", "one\\two", "one/two", "角色", "réader", "one two",
        ] {
            assert!(validate_role_name(name).is_err(), "accepted {name:?}");
        }
    }
}
