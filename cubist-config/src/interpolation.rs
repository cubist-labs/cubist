use std::{fmt::Debug, str::FromStr};

use lazy_static::lazy_static;
use regex::Regex;
use schemars::JsonSchema;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    secret::{SecretKind, CUBIST_REDACTED},
    Result,
};

/// Try to interpolate a given string, optionally "percent decoding" it first.
///
/// Substrings matching `${{<KIND>.<VALUE>}}` (where `<KIND>` is either `env`,
/// `file`, or `text`, and `<VALUE>` is an arbitrary string) are subject to
/// interpolation.  More precisely:
/// - if `<KIND>` is `env`, `<VALUE>` is interpreted as environment
/// variable name, and the secret is the value of that environment variable;
/// - if `<KIND>` is `file`, `<VALUE>` is interpreted as a file name,
/// and the secret is the contents of that file;
/// - if `<KIND>` is `txt`, `<VALUE>` is interpreted as the secret itself.
///
/// See [`REGEX_STR`] for the exact regex.
///
/// # Arguments
/// * `s` - string to interpolate
/// * `percent_decode` - whether to "percent decode" `s` first (useful if `s`
///                      was obtained by converting an URI to string)
/// # Returns
/// * A secret string, since some of the interpolated values may be secret.
pub fn try_interpolate(s: &str, percent_decode: bool) -> Result<SecretString> {
    Interpolation::deserialize(if percent_decode {
        let decoded = percent_encoding::percent_decode_str(s).decode_utf8_lossy();
        json!(decoded.as_ref())
    } else {
        json!(s)
    })
    .expect("Every string should be deserializable into Interpolation")
    .load()
}

#[derive(Clone, Debug)]
enum PartKind {
    Secret(SecretKind),
    Public(String),
}

impl PartKind {
    pub fn load(&self) -> Result<SecretString> {
        match self {
            Self::Secret(s) => s.load(),
            Self::Public(p) => Ok(p.clone().into()),
        }
    }

    fn serialize(&self) -> String {
        match self {
            PartKind::Public(v) => v.clone(),
            PartKind::Secret(s) => match s {
                SecretKind::EnvVar { env } => format!("${{{{env.{env}}}}}"),
                SecretKind::File { file } => format!("${{{{file.{}}}}}", file.display()),
                SecretKind::PlainText { .. } => format!("${{{{text.{}}}}}", CUBIST_REDACTED),
            },
        }
    }
}

/// A sequence of parts, where each part is either a plain text
/// (public string) or a variable (whose value is secret).
#[derive(Clone)]
pub struct Interpolation {
    parts: Vec<PartKind>,
}

impl Debug for Interpolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for p in &self.parts {
            f.write_str(&p.serialize())?;
        }
        Ok(())
    }
}

impl Interpolation {
    /// Load all secret values, concatenate all parts, and
    /// return the final value (which is a [`SecretString`]).
    pub fn load(&self) -> Result<SecretString> {
        let mut result = String::new();
        for part in &self.parts {
            result.push_str(part.load()?.expose_secret());
        }
        Ok(result.into())
    }
}

/// Regex for matching interpolation substrings
pub const REGEX_STR: &str = r#"\$\{\{\s*(?P<kind>env|file|text)\.(?P<value>.*?)\s*\}\}"#;
lazy_static! {
    static ref REGEX: Regex = Regex::new(REGEX_STR).unwrap();
}

impl FromStr for Interpolation {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = vec![];
        let mut last_idx: usize = 0;
        // iterate through all matches; convert each match to `PartKind::Secret`
        // and all string fragments around them to `PartKind::Public`
        for c in REGEX.captures_iter(s) {
            // get the entire match (e.g., '${{env.MY_VAR}}')
            let m = c.get(0).unwrap();
            // convert the preceding text (if any) to `PartKind::Public`
            let match_start = m.start();
            if match_start > last_idx {
                parts.push(PartKind::Public(s[last_idx..m.start()].to_string()));
            }
            // convert the match to `PartKind::Secret`
            let val = s[c.name("value").unwrap().range()].to_string();
            let kind = match &s[c.name("kind").unwrap().range()] {
                "env" => SecretKind::EnvVar { env: val },
                "file" => SecretKind::File { file: val.into() },
                "text" => SecretKind::PlainText { secret: val.into() },
                x => unreachable!("ensured by the regex above: {x:?}"),
            };
            parts.push(PartKind::Secret(kind));
            last_idx = m.end();
        }
        // don't forget any text after the last match
        if s.len() > last_idx {
            parts.push(PartKind::Public(s[last_idx..].to_string()));
        }
        Ok(Self { parts })
    }
}

// =================== schemars JsonSchema impl ==================

/// Same as for `String`
impl JsonSchema for Interpolation {
    fn schema_name() -> String {
        <String as JsonSchema>::schema_name()
    }

    fn json_schema(gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        <String as JsonSchema>::json_schema(gen)
    }
}

// ==================== serde Serialize/Deserialize impl ====================

impl<'de> Deserialize<'de> for Interpolation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let string = String::deserialize(deserializer)?;
        let val: Self = string
            .parse()
            .map_err(|e| serde::de::Error::custom(format!("{e:}")))?;
        Ok(val)
    }
}

impl Serialize for Interpolation {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let s: String = self
            .parts
            .iter()
            .map(PartKind::serialize)
            .collect::<Vec<_>>()
            .join("");
        String::serialize(&s, serializer)
    }
}

#[cfg(test)]
mod test {
    use crate::{
        interpolation::{try_interpolate, PartKind},
        secret::CUBIST_REDACTED,
    };
    use rstest::rstest;
    use secrecy::ExposeSecret;
    use serde_json::json;

    use super::Interpolation;

    fn test_parts(is: &Interpolation, num_parts: usize, num_secret_parts: usize) {
        assert_eq!(
            num_parts,
            is.parts.len(),
            "unexpected number of parts: {is:?}"
        );
        assert_eq!(
            num_secret_parts,
            is.parts
                .iter()
                .map(|s| match s {
                    PartKind::Public(..) => 0,
                    PartKind::Secret(..) => 1,
                })
                .sum::<usize>(),
            "unexpected number of secret parts: {is:?}"
        );
    }

    #[rstest]
    #[case::one_public("123", 1, 0)]
    #[case::one_secret(r#"${{env.MY_ENV_VAR}}"#, 1, 1)]
    #[case::two_secrets(r#"${{env.VAR1}}${{env.VAR2}}"#, 2, 2)]
    #[case::two_secrets_env_file(r#"${{env.VAR NAME}}${{file./foo/bar/baz/file name}}"#, 2, 2)]
    #[case::two_secrets_pre(r#"Prefix ${{env.VAR1}}${{env.VAR2}}"#, 3, 2)]
    #[case::two_secrets_in(r#"${{env.VAR1}} infix ${{env.VAR2}}"#, 3, 2)]
    #[case::two_secrets_post(r#"${{env.VAR1}}${{env.VAR2}} post"#, 3, 2)]
    #[case::invalid_kind(r#"${{invalid.test}}"#, 1, 0)]
    fn test_serde(#[case] s: &str, #[case] num_parts: usize, #[case] num_secret_parts: usize) {
        let is: Interpolation = serde_json::from_value(json!(s)).expect("Should be parsable");
        test_parts(&is, num_parts, num_secret_parts);
        let ser = serde_json::to_value(&is).expect("Should be serializable");
        assert_eq!(json!(s), ser);
    }

    #[rstest]
    #[case::one_secret_new_line(
        r#"${{
           env.MY_ENV_VAR
        }}"#,
        1,
        1,
        r#"${{env.MY_ENV_VAR}}"#
    )]
    #[case::one_secret(
        r#"${{             env.MY_ENV_VAR          }}"#,
        1,
        1,
        r#"${{env.MY_ENV_VAR}}"#
    )]
    #[case::two_secrets(
        r#"${{ env.VAR1 }}${{ env.VAR2 }}"#,
        2,
        2,
        r#"${{env.VAR1}}${{env.VAR2}}"#
    )]
    #[case::two_secrets_env_file(
        r#"PRE ${{ env.VAR NAME }} IN ${{ file./foo/bar/file name }} POST"#,
        5,
        2,
        r#"PRE ${{env.VAR NAME}} IN ${{file./foo/bar/file name}} POST"#
    )]
    #[case::nested(
        r#"${{text.bar${{text.123}}.foo}}"#,
        2,
        1,
        r#"${{text.***CUBIST REDACTED SECRET***}}.foo}}"#
    )]
    fn test_serde_spaces_allowed(
        #[case] s: &str,
        #[case] num_parts: usize,
        #[case] num_secret_parts: usize,
        #[case] expected: &str,
    ) {
        let is: Interpolation = serde_json::from_value(json!(s)).expect("Should be parsable");
        test_parts(&is, num_parts, num_secret_parts);
        let ser = serde_json::to_value(&is).expect("Should be serializable");
        assert_eq!(json!(expected), ser);
    }

    #[rstest]
    #[case::one_secret(r#"${{text.MY_SECRET}}"#, 1, 1)]
    #[case::two_secrets(r#"${{text.MY_SECRET1}} TEXT ${{text.MY_SECRET2}}"#, 3, 2)]
    fn test_ser_secret(#[case] s: &str, #[case] num_parts: usize, #[case] num_secret_parts: usize) {
        let is: Interpolation = serde_json::from_value(json!(s)).expect("Should be parsable");
        test_parts(&is, num_parts, num_secret_parts);
        let ser = serde_json::to_string(&is).expect("Should be serializable");
        assert!(!ser.contains("MY_SECRET"), "Should not contain secret");
        assert!(
            ser.contains(CUBIST_REDACTED),
            "Should contain cubist redacted"
        );
    }

    #[rstest]
    #[case::one_public("123", "123")]
    #[case::one_secret(r#"${{env.VAR1}}"#, "VALUE1")]
    #[case::two_secrets(r#"${{env.VAR1}}${{env.VAR2}}"#, "VALUE1VALUE2")]
    #[case::two_secrets_pre(r#"Prefix ${{env.VAR1}}${{env.VAR2}}"#, "Prefix VALUE1VALUE2")]
    #[case::two_secrets_in(r#"${{env.VAR1}} infix ${{env.VAR2}}"#, "VALUE1 infix VALUE2")]
    #[case::two_secrets_post(r#"${{env.VAR1}}${{env.VAR2}} post"#, "VALUE1VALUE2 post")]
    fn test_load(#[case] s: &str, #[case] expected_value: &str) {
        std::env::set_var("VAR1", "VALUE1");
        std::env::set_var("VAR2", "VALUE2");
        let is: Interpolation = serde_json::from_value(json!(s)).expect("Should be parsable");
        let val = is.load().expect("Should be loadable");
        assert_eq!(expected_value, val.expose_secret());
    }

    #[test]
    fn test_try_interpolate() {
        std::env::set_var("FAKE_API_KEY", "qwerty");
        let u = url::Url::parse(
            "https://polygon-mumbai.g.alchemy.com/v2/${{ env.FAKE_API_KEY }}?x=123&y=234",
        )
        .unwrap();
        let percent_decode = true;
        let interpolated = try_interpolate(u.as_str(), percent_decode).unwrap();
        let expected_url_str = "https://polygon-mumbai.g.alchemy.com/v2/qwerty?x=123&y=234";
        assert_eq!(
            url::Url::parse(expected_url_str),
            url::Url::parse(interpolated.expose_secret())
        );
        assert_eq!(expected_url_str, interpolated.expose_secret());
    }
}
