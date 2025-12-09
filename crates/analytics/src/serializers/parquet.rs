use {
    ::serde::{de::DeserializeOwned, Serialize},
    arrow::datatypes::FieldRef,
    parquet::{
        arrow::ArrowSchemaConverter,
        record::RecordWriter,
        schema::{parser::parse_message_type, types::TypePtr},
    },
    serde_arrow::schema::{SchemaLike as _, TracingOptions},
    std::sync::Arc,
};

pub mod native;
pub mod serde;

/// Returns `parquet` schema from parsing the string.
pub fn schema_from_str(schema: &str) -> anyhow::Result<TypePtr> {
    Ok(Arc::new(parse_message_type(schema)?))
}

/// Returns `parquet` schema based on the [`RecordWriter<T>`] implementation.
pub fn schema_from_native_writer<T>() -> anyhow::Result<TypePtr>
where
    for<'a> &'a [T]: RecordWriter<T>,
{
    Ok((&[] as &[T]).schema()?)
}

/// Returns `parquet` schema generated from [`serde_arrow`].
pub fn schema_from_serde_arrow<T>(
    name: &str,
    tracing_options: TracingOptions,
) -> anyhow::Result<TypePtr>
where
    T: Serialize + DeserializeOwned,
{
    let fields = Vec::<FieldRef>::from_type::<T>(tracing_options)?;
    let arrow_schema = serde_arrow::to_record_batch::<Vec<T>>(&fields, &Vec::new())?.schema();
    let parquet_schema = ArrowSchemaConverter::new()
        .schema_root(name)
        .convert(&arrow_schema)?;

    Ok(parquet_schema.root_schema_ptr())
}
