//! Shared YAML parsing settings for skills and agent sidecars.

use serde::{Deserialize, Serialize};

pub use serde_saphyr::Error;
pub use serde_value::Value;
/// YAML mappings can have non-string keys.
pub type Mapping = std::collections::BTreeMap<Value, Value>;

/// Parse YAML with the scalar and merge rules used by skill frontmatter.
pub fn from_str<'de, T: Deserialize<'de>>(input: &'de str) -> Result<T, Error> {
    serde_saphyr::from_str_with_options(
        input,
        serde_saphyr::options! {
            strict_booleans: true,
            leading_zero_numbers_as_strings: true,
            underscored_numbers_as_strings: true,
            merge_keys: serde_saphyr::MergeKeyPolicy::AsOrdinary,
            duplicate_keys: serde_saphyr::DuplicateKeyPolicy::PassThrough,
            non_finite_float_policy: serde_saphyr::NonFiniteFloatPolicy::PassThrough,
            with_snippet: false,
        },
    )
}

/// Serialize YAML for field display, repair, or an agent sidecar.
pub fn to_string<T: Serialize + ?Sized>(value: &T) -> Result<String, serde_saphyr::SerializeError> {
    serde_saphyr::to_string(&value)
}

/// Read metadata keys as text, including the YAML null key accepted by agents.
pub fn deserialize_metadata<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<std::collections::HashMap<String, Value>>, D::Error> {
    let metadata =
        Option::<std::collections::HashMap<Option<String>, Value>>::deserialize(deserializer)?;
    Ok(metadata.map(|entries| {
        entries
            .into_iter()
            .map(|(key, value)| (key.unwrap_or_else(|| "null".into()), value))
            .collect()
    }))
}
