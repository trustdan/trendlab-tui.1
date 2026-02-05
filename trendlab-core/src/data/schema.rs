use polars::prelude::*;

/// Expected schema for bar data
pub struct BarSchema;

impl BarSchema {
    /// Get the canonical bar schema
    pub fn schema() -> Schema {
        Schema::from_iter(vec![
            Field::new("timestamp".into(), DataType::Datetime(TimeUnit::Milliseconds, None)),
            Field::new("symbol".into(), DataType::String),
            Field::new("open".into(), DataType::Float64),
            Field::new("high".into(), DataType::Float64),
            Field::new("low".into(), DataType::Float64),
            Field::new("close".into(), DataType::Float64),
            Field::new("volume".into(), DataType::Float64),
        ])
    }

    /// Validate DataFrame against schema
    pub fn validate(df: &DataFrame) -> Result<(), SchemaError> {
        let expected = Self::schema();
        let actual = df.schema();

        // Check all required columns exist
        for field in expected.iter_fields() {
            if !actual.contains(field.name()) {
                return Err(SchemaError::MissingColumn(field.name().to_string()));
            }
        }

        // Check data types match
        for field in expected.iter_fields() {
            let actual_dtype = actual.get(field.name()).ok_or_else(|| {
                SchemaError::MissingColumn(field.name().to_string())
            })?;
            if actual_dtype != field.dtype() {
                return Err(SchemaError::TypeMismatch {
                    column: field.name().to_string(),
                    expected: field.dtype().clone(),
                    actual: actual_dtype.clone(),
                });
            }
        }

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SchemaError {
    #[error("Missing required column: {0}")]
    MissingColumn(String),

    #[error("Type mismatch in column {column}: expected {expected:?}, got {actual:?}")]
    TypeMismatch {
        column: String,
        expected: DataType,
        actual: DataType,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_has_all_required_columns() {
        let schema = BarSchema::schema();
        assert!(schema.contains("timestamp"));
        assert!(schema.contains("symbol"));
        assert!(schema.contains("open"));
        assert!(schema.contains("high"));
        assert!(schema.contains("low"));
        assert!(schema.contains("close"));
        assert!(schema.contains("volume"));
    }

    #[test]
    fn test_validate_accepts_valid_dataframe() {
        // Create a datetime series
        let timestamp = Series::new("timestamp".into(), &[1672531200000i64])
            .cast(&DataType::Datetime(TimeUnit::Milliseconds, None))
            .unwrap();

        let df = DataFrame::new(vec![
            Column::Series(timestamp),
            Column::Series(Series::new("symbol".into(), &["SPY"])),
            Column::Series(Series::new("open".into(), &[400.0])),
            Column::Series(Series::new("high".into(), &[405.0])),
            Column::Series(Series::new("low".into(), &[399.0])),
            Column::Series(Series::new("close".into(), &[403.0])),
            Column::Series(Series::new("volume".into(), &[1000000.0])),
        ])
        .unwrap();

        let result = BarSchema::validate(&df);
        if let Err(ref e) = result {
            eprintln!("Validation error: {:?}", e);
            eprintln!("DataFrame schema: {:?}", df.schema());
        }
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_rejects_missing_column() {
        let timestamp = Series::new("timestamp".into(), &[1672531200000i64])
            .cast(&DataType::Datetime(TimeUnit::Milliseconds, None))
            .unwrap();

        let df = DataFrame::new(vec![
            Column::Series(timestamp),
            Column::Series(Series::new("symbol".into(), &["SPY"])),
            Column::Series(Series::new("open".into(), &[400.0])),
            // Missing high, low, close, volume
        ])
        .unwrap();

        let result = BarSchema::validate(&df);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SchemaError::MissingColumn(_)));
    }

    #[test]
    fn test_validate_rejects_wrong_type() {
        let timestamp = Series::new("timestamp".into(), &[1672531200000i64])
            .cast(&DataType::Datetime(TimeUnit::Milliseconds, None))
            .unwrap();

        let df = DataFrame::new(vec![
            Column::Series(timestamp),
            Column::Series(Series::new("symbol".into(), &["SPY"])),
            Column::Series(Series::new("open".into(), &["not_a_number"])), // Wrong type
            Column::Series(Series::new("high".into(), &[405.0])),
            Column::Series(Series::new("low".into(), &[399.0])),
            Column::Series(Series::new("close".into(), &[403.0])),
            Column::Series(Series::new("volume".into(), &[1000000.0])),
        ])
        .unwrap();

        let result = BarSchema::validate(&df);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SchemaError::TypeMismatch { .. }));
    }
}
