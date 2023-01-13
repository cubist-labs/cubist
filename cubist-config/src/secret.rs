use std::{
    fmt::{Debug, Display},
    marker::PhantomData,
    path::PathBuf,
};

use coins_bip39::{Mnemonic, Wordlist};
use k256::SecretKey;
use schemars::JsonSchema;
use secrecy::{ExposeSecret, SecretString};
use serde::{de::Error, Deserialize, Deserializer, Serialize, Serializer};
use url::Url;

use crate::{interpolation::try_interpolate, ConfigError, Result};

/// Different ways to provide a secret value
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
#[serde(deny_unknown_fields)]
pub(crate) enum SecretKind {
    /// Secret value is the value of an environment variable.
    /// If found, .env file is automatically loaded.
    EnvVar {
        /// Name of the environment variable
        env: String,
    },
    /// Secret value is the contents of a file
    File {
        /// File path. If the path is relative, it is resolved against
        /// the **current working directory**, not project root
        /// directory.  (TODO: consider changing this)
        file: PathBuf,
    },
    /// Secret saved as plain text
    PlainText {
        /// The secret value
        #[serde(serialize_with = "ser_secret", deserialize_with = "de_secret")]
        #[schemars(with = "String")]
        secret: SecretString,
    },
}

/// Marker for redacted text
pub(crate) const CUBIST_REDACTED: &str = "***CUBIST REDACTED SECRET***";

/// Deserialize from a plain string
fn de_secret<'a, D>(deserializer: D) -> Result<SecretString, D::Error>
where
    D: Deserializer<'a>,
{
    let s = String::deserialize(deserializer)?;
    Ok(SecretString::new(s))
}

/// Redact the actual secret value
fn ser_secret<S>(_key: &SecretString, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(CUBIST_REDACTED)
}

impl SecretKind {
    /// Loads and returns secret value
    pub fn load(&self) -> Result<SecretString> {
        let sec = match self {
            Self::PlainText { secret } => secret.clone(),
            Self::EnvVar { env } => dotenv::var(&env)
                .map_err(|e| ConfigError::SecretReadFromEnv(env.clone(), e))?
                .into(),
            Self::File { file } => std::fs::read_to_string(file)
                .map_err(|e| ConfigError::SecretReadFromFile(file.clone(), e))?
                .into(),
        };
        Ok(sec)
    }
}

/// An extension point used to validate different types of secrets
pub trait ValidateSecret<T> {
    /// Returns whether `val` is a valid secret of type `T`, i.e.,
    /// `None` if it valid, or `Some(String)` if it is not (the value
    /// is the reason why it is not valid).
    fn validate(val: &str) -> Option<String>;
}

/// Thin wrapper around [`Secret`] that additionally validates that
/// the secret is a valid bip39 mnemonic.
#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Secret<T = String>
where
    T: ValidateSecret<T>,
{
    #[serde(flatten)]
    pub(crate) inner: SecretKind,
    #[serde(skip)]
    phantom: PhantomData<T>,
}

impl<T: ValidateSecret<T>> Secret<T> {
    fn new(inner: SecretKind) -> Self {
        Self {
            inner,
            phantom: Default::default(),
        }
    }

    /// Loads and returns secret value
    pub fn load(&self) -> Result<SecretString> {
        self.inner.load()
    }
}

pub(crate) const INVALID_MNEMONIC_ERR: &str = "Invalid BIP39 mnemonic";
pub(crate) const INVALID_PRIVATE_KEY_HEX_ERR: &str =
    "Invalid private key; expected hex string without leading '0x'";
pub(crate) const INVALID_PRIVATE_KEY_ERR: &str = "Invalid K-256 secret key";

// ==================== ValidateSecret impls ====================

impl ValidateSecret<String> for String {
    fn validate(_val: &str) -> Option<String> {
        None
    }
}

/// Secret validation for `SecretKey`
impl ValidateSecret<SecretKey> for SecretKey {
    fn validate(val: &str) -> Option<String> {
        match hex::decode(val) {
            Ok(decoded) => match SecretKey::from_be_bytes(&decoded) {
                Ok(_) => None,
                Err(e) => Some(format!("{INVALID_PRIVATE_KEY_ERR}: {e}")),
            },
            Err(e) => Some(format!("{INVALID_PRIVATE_KEY_HEX_ERR}: {e}")),
        }
    }
}

impl<T: Wordlist> ValidateSecret<Mnemonic<T>> for Mnemonic<T> {
    fn validate(val: &str) -> Option<String> {
        match Mnemonic::<T>::new_from_phrase(val) {
            Ok(_) => None,
            Err(e) => Some(format!("{INVALID_MNEMONIC_ERR}: {e}")),
        }
    }
}

// ==================== serde Deserialize impl ====================

impl<'de, T: ValidateSecret<T>> Deserialize<'de> for Secret<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let inner: SecretKind = SecretKind::deserialize(deserializer)?;
        match inner.load() {
            Ok(val) => match T::validate(val.expose_secret()) {
                None => Ok(Secret::new(inner)),
                Some(e) => Err(Error::custom(e)),
            },
            // don't fail eagerly if not set at all (maybe this secret won't be needed)
            Err(_) => Ok(Secret::new(inner)),
        }
    }
}

// =================== schemars JsonSchema impl ==================

impl<T: ValidateSecret<T>> JsonSchema for Secret<T> {
    fn schema_name() -> String {
        <SecretKind as JsonSchema>::schema_name()
    }

    fn json_schema(gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        <SecretKind as JsonSchema>::json_schema(gen)
    }
}

// ==================== From conversions impls ====================

impl<T: ValidateSecret<T>> From<SecretKind> for Secret<T> {
    fn from(s: SecretKind) -> Self {
        Self {
            inner: s,
            phantom: Default::default(),
        }
    }
}

impl From<String> for SecretKind {
    fn from(s: String) -> Self {
        SecretKind::PlainText {
            secret: SecretString::new(s),
        }
    }
}

impl<T: ValidateSecret<T>> From<String> for Secret<T> {
    fn from(s: String) -> Self {
        SecretKind::from(s).into()
    }
}

// ==================== SecretUrl ====================

/// A URL that may embed secrets via special interpolation variables.
///
/// For example: `https://polygon-mumbai.g.alchemy.com/v2/${{env.ALCHEMY_API_KEY}}`.
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
/// See [`REGEX_STR`](crate::interpolation::REGEX_STR) for the exact
/// regex.
///
/// The secret values are loaded only on demand via the [`Self::load`] and
/// [`Self::expose_url`] methods.
#[derive(Clone, Debug)]
pub struct SecretUrl {
    /// Url that may contain interpolation
    url: Url,
}

impl Display for SecretUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let url_str = self.url.to_string();
        let decoded = percent_encoding::percent_decode_str(&url_str).decode_utf8_lossy();
        Display::fmt(&decoded, f)
    }
}

impl From<Url> for SecretUrl {
    fn from(url: Url) -> Self {
        Self { url }
    }
}

impl TryFrom<&str> for SecretUrl {
    type Error = url::ParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Ok(Url::parse(value)?.into())
    }
}

impl SecretUrl {
    /// Returns the URL port, if any.
    pub fn port(&self) -> Option<u16> {
        self.url.port()
    }

    /// Loads and embeds any secret values into this URL and returns a secret string.
    pub fn load(&self) -> Result<SecretString> {
        let ustr = self.url.as_str();
        let percent_decode = true;
        try_interpolate(ustr, percent_decode)
    }

    /// Returns the URL scheme
    pub fn scheme(&self) -> &str {
        self.url.scheme()
    }

    /// Loads and embeds any secret values into this URL, then
    /// converts the result into [`url::Url`].
    pub fn expose_url(&self) -> Result<Url> {
        self.expose_url_and_update(None, None, None)
    }

    /// Loads and embeds any secret values into this URL, then
    /// converts the result into [`url::Url`], optionally updating the
    /// "scheme", "port", and "path" properties of the final URL.
    ///
    /// # Arguments
    /// * `set_scheme` - if some, then update the scheme of the exposed URL before returning it
    /// * `set_port` - if some, then update the port of the exposed URL before returning it
    /// * `set_path` - if some, then update the path of the exposed URL before returning it
    pub fn expose_url_and_update(
        &self,
        set_scheme: Option<&str>,
        set_port: Option<u16>,
        set_path: Option<&str>,
    ) -> Result<Url> {
        let secret = self.load()?;
        let mut exposed = Url::parse(secret.expose_secret())
            .map_err(|_| ConfigError::UrlInterpolate(self.url.to_string()))?;
        if let Some(scheme) = set_scheme {
            exposed
                .set_scheme(scheme)
                .map_err(|_| ConfigError::UrlInvalidScheme(scheme.to_owned()))?;
        }
        if set_port.is_some() {
            exposed.set_port(set_port).expect("Invalid port number");
        }
        if let Some(path) = set_path {
            exposed.set_path(path);
        }
        Ok(exposed)
    }
}

/// Same as for [`url::Url`]
impl<'de> Deserialize<'de> for SecretUrl {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let url = url::Url::deserialize(deserializer)
            .map_err(|e| serde::de::Error::custom(format!("{e}")))?;
        Ok(Self { url })
    }
}

/// Same as for [`url::Url`]
impl Serialize for SecretUrl {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.url.serialize(serializer)
    }
}

/// Same as for [`url::Url`]
impl JsonSchema for SecretUrl {
    fn schema_name() -> String {
        <url::Url as JsonSchema>::schema_name()
    }

    fn json_schema(gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        <url::Url as JsonSchema>::json_schema(gen)
    }
}

// ==================== unit tests ====================

#[cfg(test)]
mod test {
    use crate::secret::{SecretKind, CUBIST_REDACTED};

    use super::Secret;
    use secrecy::ExposeSecret;
    use serde_json::json;

    #[test]
    fn test_secret_debug() {
        let val = "qwerty";
        let sec: SecretKind = val.to_string().into();
        assert!(matches!(sec, SecretKind::PlainText { .. }));
        let dbg = format!("{sec:?}");
        assert!(!dbg.contains(val));
        assert_eq!(
            "PlainText { secret: Secret([REDACTED alloc::string::String]) }",
            dbg
        );
    }

    #[test]
    fn test_secret_clone() {
        let val = "qwerty";
        let sec1: SecretKind = val.to_string().into();
        assert!(matches!(sec1, SecretKind::PlainText { .. }));
        let sec2 = sec1.clone();
        let loaded1 = sec1.load().unwrap();
        let loaded2 = sec2.load().unwrap();
        assert_eq!(loaded1.expose_secret(), loaded2.expose_secret());
        assert_eq!(val, loaded2.expose_secret());
    }

    #[test]
    fn load_secret_from_plain_text() {
        let val = "qwerty";
        let sec: SecretKind = val.to_string().into();
        assert!(matches!(sec, SecretKind::PlainText { .. }));
        let loaded = sec.load().unwrap();
        assert_eq!(val, loaded.expose_secret());
    }

    #[test]
    fn load_secret_from_env_var() {
        let env_var = "load_secret_from_env_var_test";
        let sec = SecretKind::EnvVar {
            env: env_var.into(),
        };
        let val = "qwerty";
        std::env::set_var(env_var, val);
        let loaded = sec.load().unwrap();
        assert_eq!(val, loaded.expose_secret());
    }

    #[test]
    fn load_secret_from_file() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("test");
        let val = "qwerty";
        std::fs::write(&file, val).unwrap();
        let sec = SecretKind::File { file };
        let loaded = sec.load().unwrap();
        assert_eq!(val, loaded.expose_secret());
    }

    #[test]
    fn secret_serde() {
        let val = "qwerty";
        let json = json!({ "secret": val });
        let sec: Secret = serde_json::from_value(json).unwrap();
        let loaded = sec.load().unwrap();
        assert_eq!(val, loaded.expose_secret());
        let ser: serde_json::Value = serde_json::to_value(&sec).unwrap();
        let secret_val = ser.get("secret").unwrap();
        assert!(!secret_val.to_string().contains(val));
        assert_eq!(&json!(CUBIST_REDACTED), secret_val);
    }
}
