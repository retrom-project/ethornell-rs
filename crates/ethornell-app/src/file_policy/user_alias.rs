// Some BGI scripts build their user-data directory from the primary install
// root. The conventional user-data subtree, global database and native config
// slots are writable virtual aliases. BGI.gdb defaults to the install root when the
// script has not set a validated user-data root (80:80/81, sub_46B960).
// Arbitrary resource-root names remain read-only. These aliases never denote
// host IO in the resource directory and never import original save bytes.
pub(crate) fn user_data_alias(name: &str) -> String {
    let normalized = name.replace('\\', "/");
    if let Some(rest) = normalized.strip_prefix("bgi-resource/") {
        let first = rest.split('/').next().unwrap_or("");
        if first.eq_ignore_ascii_case("UserData")
            || rest.eq_ignore_ascii_case("BGI.gdb")
            || is_config_slot(rest)
        {
            return format!("bgi-user/{rest}");
        }
    }
    normalized
}

fn is_config_slot(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() >= 11
        && bytes[..3].eq_ignore_ascii_case(b"BGI")
        && bytes[bytes.len() - 4..].eq_ignore_ascii_case(b".cad")
        && bytes[3..bytes.len() - 4].iter().all(u8::is_ascii_digit)
}

#[cfg(test)]
mod tests {
    use super::user_data_alias;
    #[test]
    fn engine_slot_aliases_use_the_private_user_root() {
        for name in ["BGI0000.cad", "bgi12345.CAD"] {
            assert_eq!(
                user_data_alias(&format!("bgi-resource/{name}")),
                format!("bgi-user/{name}")
            );
        }
    }
    #[test]
    fn original_resources_and_near_matches_remain_readonly() {
        for name in [
            "story.arc",
            "nested/BGI0000.cad",
            "BGI0000.cad.bak",
            "BGIfake.cad",
            "BGI000.cad",
            "BGI0000.cad/../story.arc",
        ] {
            let path = format!("bgi-resource/{name}");
            assert_eq!(user_data_alias(&path), path);
        }
    }
}
