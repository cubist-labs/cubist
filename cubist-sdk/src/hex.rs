/// Hex encode/decode utils
pub mod serde_hex {
    use serde::{de::Error, Deserialize, Deserializer, Serializer};

    /// Serializes a vector of u8 to a hex string (without 0x prefix)
    pub fn serialize<S>(data: impl AsRef<[u8]>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("0x{}", hex::encode(data)))
    }

    /// Deserializes a vector of u8 to a hex string (without 0x prefix)
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let hex_str = String::deserialize(deserializer)?[2..].to_string();
        hex::decode(hex_str).map_err(D::Error::custom)
    }
}
