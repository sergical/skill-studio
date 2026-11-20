//! Property test for `normalize_default_server_url`, which turns the
//! compile-time `SKILL_STUDIO_SERVER_URL` into the server base URL every
//! discovery request starts from.

use proptest::prelude::*;

use super::normalize_default_server_url;

const LOOPBACK: &str = "http://127.0.0.1:8787";

fn url() -> impl Strategy<Value = String> {
    (
        prop_oneof![Just("http://"), Just("https://")],
        "[a-z0-9.-]{1,20}(:[0-9]{1,5})?(/[a-z0-9._-]{1,8}){0,3}",
        "/{0,3}",
        "[ \t]{0,2}",
        "[ \t\n]{0,2}",
    )
        .prop_map(|(scheme, rest, slashes, lead, trail)| {
            format!("{lead}{scheme}{rest}{slashes}{trail}")
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn a_configured_url_loses_only_its_whitespace_and_trailing_slashes(raw in url()) {
        let normalized = normalize_default_server_url(Some(&raw));
        prop_assert_eq!(&normalized, raw.trim().trim_end_matches('/'));
        prop_assert!(!normalized.ends_with('/'));
        prop_assert_eq!(normalize_default_server_url(Some(&normalized)), normalized.clone());
    }

    #[test]
    fn no_url_or_only_whitespace_gives_the_local_server(blank in "[ \t\n]{0,4}", unset in any::<bool>()) {
        let value = (!unset).then_some(blank.as_str());
        prop_assert_eq!(normalize_default_server_url(value), LOOPBACK);
    }
}
