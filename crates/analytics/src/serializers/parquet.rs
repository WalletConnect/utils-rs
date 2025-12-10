use {
    parquet::schema::{parser::parse_message_type, types::TypePtr},
    std::sync::Arc,
};

#[cfg(feature = "parquet-native")]
pub mod native;
#[cfg(feature = "parquet-serde")]
pub mod serde;

/// Returns `parquet` schema from parsing the string.
pub fn schema_from_str(schema: &str) -> anyhow::Result<TypePtr> {
    Ok(Arc::new(parse_message_type(schema)?))
}
