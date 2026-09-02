// Version derivation for the loom binary.
//
// Pure logic, `std`-only, shared between `build.rs` (via `include!`) and the
// test suite below. Kept out of `build.rs` itself because build scripts are
// not test targets.
//
// This file is `include!`d into `build.rs` after other items, so it cannot
// use inner (`//!`) doc comments — those are only valid as the first thing
// in a file or module.

/// Derive a semver-valid version string from git describe output.
///
/// Branches, tried in order:
/// 1. `describe_exact` (a tag build) is used verbatim, minus a leading `v`,
///    if it validates as semver; otherwise falls through to branch 2.
/// 2. `describe` (tag plus commits-since plus short sha) is parsed into a
///    `-dev.N+sha` prerelease on top of a bumped patch version.
/// 3. `short_sha` alone produces a placeholder `0.0.0-dev+sha` version.
/// 4. Otherwise, an unknown placeholder.
pub fn derive_version(
    describe_exact: Option<&str>,
    describe: Option<&str>,
    short_sha: Option<&str>,
) -> String {
    if let Some(tag) = describe_exact {
        let stripped = strip_v_prefix(tag);
        if is_semver(stripped) {
            return stripped.to_string();
        }
    }

    if let Some(describe) = describe {
        if let Some(version) = derive_from_describe(describe) {
            return version;
        }
    }

    if let Some(sha) = short_sha {
        return format!("0.0.0-dev+{sha}");
    }

    "0.0.0-dev+unknown".to_string()
}

fn strip_v_prefix(tag: &str) -> &str {
    tag.strip_prefix('v').unwrap_or(tag)
}

/// Check that a (post `v`-strip) tag is real semver: exactly three
/// `.`-separated integers, optionally followed by `-prerelease` and/or
/// `+build` metadata. Build metadata is stripped first, then prerelease,
/// since a `-` may otherwise appear inside `+build`.
fn is_semver(tag: &str) -> bool {
    let without_build = tag.split('+').next().unwrap_or(tag);
    let core = without_build
        .split_once('-')
        .map(|(core, _)| core)
        .unwrap_or(without_build);

    let parts: Vec<&str> = core.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
}

/// Parse a `git describe --tags` string of the form `<tag>-<count>-g<sha>`
/// into `{major}.{minor}.{patch+1}-dev.{count}+{sha}`.
///
/// Returns `None` on any parse failure so the caller can fall through to the
/// next branch rather than panic.
fn derive_from_describe(describe: &str) -> Option<String> {
    let g_idx = describe.rfind("-g")?;
    let sha = &describe[g_idx + 2..];
    if sha.is_empty() {
        return None;
    }
    let remainder = &describe[..g_idx];

    let dash_idx = remainder.rfind('-')?;
    let count_str = &remainder[dash_idx + 1..];
    let tag = &remainder[..dash_idx];
    let count: u64 = count_str.parse().ok()?;

    let tag = strip_v_prefix(tag);
    let numeric_core = tag.split('-').next()?;
    let mut parts = numeric_core.split('.');
    let major: u64 = parts.next()?.parse().ok()?;
    let minor: u64 = parts.next()?.parse().ok()?;
    let patch: u64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }

    Some(format!("{major}.{minor}.{}-dev.{count}+{sha}", patch + 1))
}

/// Convert a day count since the Unix epoch (1970-01-01) into a `YYYY-MM-DD`
/// UTC date string, via the standard days-from-civil inverse algorithm.
///
/// Pure math, kept out of `build.rs` (which owns only the clock read) so it
/// can be exercised by the test suite below.
pub fn civil_date_from_days(days: i64) -> String {
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097); // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_tag_build_uses_the_tag_verbatim() {
        assert_eq!(derive_version(Some("v1.2.3"), None, None), "1.2.3");
    }

    #[test]
    fn describe_with_commits_since_bumps_patch_and_embeds_sha() {
        assert_eq!(
            derive_version(None, Some("v0.2.0-5-gabc1234"), None),
            "0.2.1-dev.5+abc1234"
        );
    }

    #[test]
    fn describe_with_prerelease_tag_uses_only_the_numeric_core() {
        assert_eq!(
            derive_version(None, Some("v0.2.0-rc1-5-gabc1234"), None),
            "0.2.1-dev.5+abc1234"
        );
    }

    #[test]
    fn malformed_describe_falls_through_to_short_sha() {
        assert_eq!(
            derive_version(None, Some("not-a-git-describe-string"), Some("deadbee")),
            "0.0.0-dev+deadbee"
        );
    }

    #[test]
    fn short_sha_only_produces_a_placeholder_version() {
        assert_eq!(
            derive_version(None, None, Some("abc1234")),
            "0.0.0-dev+abc1234"
        );
    }

    #[test]
    fn nothing_available_falls_back_to_unknown() {
        assert_eq!(derive_version(None, None, None), "0.0.0-dev+unknown");
    }

    #[test]
    fn malformed_exact_tag_falls_through_to_short_sha() {
        assert_eq!(
            derive_version(Some("checkpoint1"), None, Some("deadbee")),
            "0.0.0-dev+deadbee"
        );
    }

    #[test]
    fn prerelease_exact_tag_uses_the_tag_verbatim() {
        assert_eq!(derive_version(Some("v1.2.0-rc1"), None, None), "1.2.0-rc1");
    }

    // civil_date_from_days: expected values derived independently via
    // `date -u -r $((days * 86400)) +%Y-%m-%d` on macOS, not from the
    // algorithm under test.
    #[test]
    fn civil_date_from_days_matches_known_utc_dates() {
        let cases: &[(i64, &str)] = &[
            (0, "1970-01-01"),
            (18321, "2020-02-29"),
            (18627, "2020-12-31"),
            (18628, "2021-01-01"),
        ];
        for &(days, expected) in cases {
            assert_eq!(civil_date_from_days(days), expected, "days = {days}");
        }
    }
}
