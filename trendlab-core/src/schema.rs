//! Parquet schema contract — the boundary between Track A (data) and Track B (engine).
//!
//! Defines the exact column names, data types, sort order, and timezone convention
//! that both tracks must conform to. Used for validation when loading data.

use serde::{Deserialize, Serialize};

/// Expected data types in the Parquet schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SchemaType {
    Date,
    Float64,
    UInt64,
}

/// A single field in the expected Parquet schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaField {
    pub name: &'static str,
    pub dtype: SchemaType,
}

/// The canonical Parquet schema contract.
///
/// - Columns: date, open, high, low, close, volume, adj_close
/// - Sort order: ascending by date within each symbol partition
/// - Timezone: market-local (US Eastern for US equities)
/// - Partitioning: Hive-style `symbol=XXX/year=YYYY/` directories
/// - Missing bars: strict NaN (no forward-fill of tradable price data)
pub const PARQUET_SCHEMA: &[SchemaField] = &[
    SchemaField {
        name: "date",
        dtype: SchemaType::Date,
    },
    SchemaField {
        name: "open",
        dtype: SchemaType::Float64,
    },
    SchemaField {
        name: "high",
        dtype: SchemaType::Float64,
    },
    SchemaField {
        name: "low",
        dtype: SchemaType::Float64,
    },
    SchemaField {
        name: "close",
        dtype: SchemaType::Float64,
    },
    SchemaField {
        name: "volume",
        dtype: SchemaType::UInt64,
    },
    SchemaField {
        name: "adj_close",
        dtype: SchemaType::Float64,
    },
];

/// Result of schema validation.
#[derive(Debug, Clone)]
pub struct SchemaValidation {
    pub is_valid: bool,
    pub errors: Vec<String>,
}

/// Validate a set of (column_name, column_type) pairs against the Parquet schema contract.
///
/// This is a lightweight validation that doesn't require Polars. The actual DataFrame
/// validation (using Polars) is built in Phase 4 on top of this contract.
pub fn validate_schema(columns: &[(&str, SchemaType)]) -> SchemaValidation {
    let mut errors = Vec::new();

    // Check all required columns are present with correct types
    for expected in PARQUET_SCHEMA {
        match columns.iter().find(|(name, _)| *name == expected.name) {
            Some((_, dtype)) if *dtype == expected.dtype => {}
            Some((_, dtype)) => {
                errors.push(format!(
                    "column '{}': expected {:?}, got {:?}",
                    expected.name, expected.dtype, dtype
                ));
            }
            None => {
                errors.push(format!("missing required column '{}'", expected.name));
            }
        }
    }

    // Check for unexpected columns (warning, not error)
    for (name, _) in columns {
        if !PARQUET_SCHEMA.iter().any(|f| f.name == *name) {
            errors.push(format!("unexpected column '{}' (not in schema)", name));
        }
    }

    SchemaValidation {
        is_valid: errors.is_empty(),
        errors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_schema_passes() {
        let columns = vec![
            ("date", SchemaType::Date),
            ("open", SchemaType::Float64),
            ("high", SchemaType::Float64),
            ("low", SchemaType::Float64),
            ("close", SchemaType::Float64),
            ("volume", SchemaType::UInt64),
            ("adj_close", SchemaType::Float64),
        ];
        let result = validate_schema(&columns);
        assert!(result.is_valid, "errors: {:?}", result.errors);
    }

    #[test]
    fn missing_column_fails() {
        let columns = vec![
            ("date", SchemaType::Date),
            ("open", SchemaType::Float64),
            ("high", SchemaType::Float64),
            ("low", SchemaType::Float64),
            ("close", SchemaType::Float64),
            // missing volume and adj_close
        ];
        let result = validate_schema(&columns);
        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| e.contains("volume")));
        assert!(result.errors.iter().any(|e| e.contains("adj_close")));
    }

    #[test]
    fn wrong_type_fails() {
        let columns = vec![
            ("date", SchemaType::Date),
            ("open", SchemaType::Float64),
            ("high", SchemaType::Float64),
            ("low", SchemaType::Float64),
            ("close", SchemaType::Float64),
            ("volume", SchemaType::Float64), // wrong: should be UInt64
            ("adj_close", SchemaType::Float64),
        ];
        let result = validate_schema(&columns);
        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| e.contains("volume")));
    }

    #[test]
    fn extra_column_flagged() {
        let columns = vec![
            ("date", SchemaType::Date),
            ("open", SchemaType::Float64),
            ("high", SchemaType::Float64),
            ("low", SchemaType::Float64),
            ("close", SchemaType::Float64),
            ("volume", SchemaType::UInt64),
            ("adj_close", SchemaType::Float64),
            ("extra_col", SchemaType::Float64),
        ];
        let result = validate_schema(&columns);
        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| e.contains("extra_col")));
    }

    #[test]
    fn schema_has_seven_fields() {
        assert_eq!(PARQUET_SCHEMA.len(), 7);
    }
}
