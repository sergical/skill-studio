pub(crate) fn validate_run_id(run_id: &str) -> Result<(), String> {
    if run_id.is_empty() || run_id.len() > 64 {
        return Err("Run id must be 1-64 characters".to_string());
    }
    if !run_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err("Run id must contain only letters, digits, and '-'".to_string());
    }
    Ok(())
}

pub(crate) fn validate_skill_dir_name(name: &str) -> Result<&str, String> {
    if name.is_empty() || name == "." || name == ".." {
        return Err(format!("Invalid skill name: {name:?}"));
    }
    if name.len() > 128 {
        return Err(format!("Skill name too long: {name:?}"));
    }
    if name.contains('/') || name.contains('\\') || name.contains('\0') {
        return Err(format!("Invalid skill name: {name:?}"));
    }
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validate_skill_dir_name_rejects_traversal_and_bad_shapes() {
        assert!(validate_skill_dir_name("../x").is_err());
        assert!(validate_skill_dir_name("/abs").is_err());
        assert!(validate_skill_dir_name("a/b").is_err());
        assert!(validate_skill_dir_name("").is_err());
        assert!(validate_skill_dir_name(".").is_err());
        assert!(validate_skill_dir_name("..").is_err());
        assert!(validate_skill_dir_name(&"a".repeat(129)).is_err());
        assert!(validate_skill_dir_name("say-banana").is_ok());
    }
}
