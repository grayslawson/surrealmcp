use surrealdb::types::{Array, Object, Number, Value, Table, RecordId};
use surrealdb::types::ToSql;
use surrealdb::{Surreal, engine::any::Any};

/// Generate a unique connection ID
pub fn generate_connection_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let random = rand::random::<u32>();
    format!("conn_{timestamp:x}_{random:x}")
}

/// Format duration in a human-readable way
pub fn format_duration(duration: std::time::Duration) -> String {
    let total_secs = duration.as_secs();
    let millis = duration.subsec_millis();

    if total_secs == 0 {
        format!("{millis}ms")
    } else if total_secs < 60 {
        format!("{total_secs}.{millis:03}s")
    } else if total_secs < 3600 {
        let minutes = total_secs / 60;
        let seconds = total_secs % 60;
        format!("{minutes}m {seconds}s")
    } else {
        let hours = total_secs / 3600;
        let minutes = (total_secs % 3600) / 60;
        let seconds = total_secs % 60;
        format!("{hours}h {minutes}m {seconds}s")
    }
}

/// Check the health and version of the SurrealDB instance
///
/// This function performs an 'INFO FOR ROOT' query to verify connectivity
/// and checks the version to ensure it's a 3.x instance.
#[allow(dead_code)]
pub async fn check_health(db: &Surreal<Any>) -> anyhow::Result<(bool, String)> {
    // Perform INFO FOR ROOT; query
    let _response = db.query("INFO FOR ROOT;").await?;

    // In SurrealDB v3, INFO FOR ROOT should succeed if we are authenticated as root.
    // However, if we are not authenticated, it might fail.
    // A simpler way to get the version is db.version().
    let version = db.version().await?;
    let version_str = version.to_string();

    // Check if it's a 3.x instance
    let is_v3 = version_str.starts_with('3');

    if is_v3 {
        Ok((true, version_str))
    } else {
        Ok((false, format!("Unsupported SurrealDB version: {version_str}. Expected 3.x")))
    }
}

/// Convert various types to SurrealDB Value
///
/// This function safely converts serde_json::Value or String to a SurrealDB Value,
/// providing detailed error messages for conversion failures.
///
/// # Arguments
/// * `value` - The value to convert (serde_json::Value or String)
/// * `name` - The name of the parameter being converted (for error messages)
///
/// # Returns
/// * `Ok(Value)` - The converted SurrealDB Value
/// * `Err(String)` - Error message if conversion fails
///
/// # Examples
/// ```
/// use surrealmcp::utils;
///
/// // Convert JSON value
/// let json_val = serde_json::json!({"name": "John"});
/// let surreal_val = utils::convert_json_to_surreal(json_val, "user_data").unwrap();
///
/// // Convert string directly
/// let string_val = serde_json::Value::String("table_name".to_string());
/// let surreal_val = utils::convert_json_to_surreal(string_val, "table").unwrap();
/// ```
pub fn convert_json_to_surreal(
    value: impl Into<serde_json::Value>,
    name: &str,
) -> Result<Value, String> {
    convert_json_to_surreal_recursive(value.into())
        .map_err(|e| format!("Failed to convert parameter '{name}{e}'"))
}

/// Internal recursive conversion function that avoids eager string formatting for paths.
/// String paths are only constructed on the error path using `map_err`.
fn convert_json_to_surreal_recursive(
    json_value: serde_json::Value,
) -> Result<Value, String> {
    match json_value {
        serde_json::Value::Null => Ok(Value::None),
        serde_json::Value::Bool(b) => Ok(Value::Bool(b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Value::Number(Number::from(i)))
            } else if let Some(f) = n.as_f64() {
                Ok(Value::Number(Number::from(f)))
            } else {
                Ok(Value::None)
            }
        }
        serde_json::Value::String(s) => Ok(Value::String(s)),
        serde_json::Value::Array(a) => {
            let mut vals = Vec::with_capacity(a.len());
            for (i, v) in a.into_iter().enumerate() {
                vals.push(convert_json_to_surreal_recursive(v)
                    .map_err(|e| format!("[{i}]{e}"))?);
            }
            Ok(Value::Array(Array::from(vals)))
        }
        serde_json::Value::Object(o) => {
            let mut map = std::collections::BTreeMap::new();
            for (k, v) in o {
                let val = convert_json_to_surreal_recursive(v)
                    .map_err(|e| format!(".{k}{e}"))?;
                map.insert(k, val);
            }
            Ok(Value::Object(Object::from(map)))
        }
    }
}

/// Convert a SurrealDB Value to a SurrealQL-compatible string
pub fn to_surrealql(value: &Value) -> String {
    value.to_sql()
}

/// Parse a single item into a SurrealQL Value
pub fn parse_target(value: String) -> Result<String, String> {
    if value.contains(':') {
        // Try parsing as simple record ID (table:id)
        if let Ok(rid) = RecordId::parse_simple(&value) {
            return Ok(Value::RecordId(rid).to_sql());
        }
    }
    // If not a record ID, treat as table name for common operations
    // or just a string if it's really intended as one.
    // Table::from(s).to_sql() will return the identifier or quoted table name.
    Ok(Value::Table(Table::from(value)).to_sql())
}

/// Convert a SurrealDB Value to a clean JSON representation (flattened)
pub fn surreal_to_json(value: Value) -> serde_json::Value {
    match value {
        Value::None | Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(b),
        Value::Number(n) => {
            match n {
                Number::Int(i) => i.into(),
                Number::Float(f) => f.into(),
                Number::Decimal(d) => d.to_string().into(), // Decimal as string to preserve precision
            }
        }
        Value::String(s) => serde_json::Value::String(s),
        Value::Array(a) => serde_json::Value::Array(a.into_iter().map(surreal_to_json).collect()),
        Value::Object(o) => {
            let mut map = serde_json::Map::new();
            for (k, v) in o {
                map.insert(k, surreal_to_json(v));
            }
            serde_json::Value::Object(map)
        }
        _ => {
            // For other types (RecordId, Geometry, etc.), use the SQL representation
            serde_json::Value::String(value.to_sql())
        }
    }
}

/// Parse a list of items into a list of SurrealQL Values
///
/// This function takes a list of strings and attempts to parse them into SurrealQL Values.
/// If a string cannot be parsed as a SurrealQL Value, an error is returned.
///
/// # Arguments
/// * `value` - A vector of strings to parse
pub fn parse_targets(values: Vec<String>) -> Result<String, String> {
    let mut items = Vec::new();
    for v in values {
        items.push(parse_target(v)?);
    }
    Ok(items.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_convert_json_to_surreal_with_object() {
        let json_val = json!({"name": "Alice", "age": 30, "active": true});
        let result = convert_json_to_surreal(json_val, "user_data");
        assert!(result.is_ok());
        let val = result.unwrap();
        // Convert back to string to verify the content
        println!("val: {val:?}");
        let val_str = format!("{:?}", val);
        assert!(val_str.contains("Alice"));
        assert!(val_str.contains("30"));
        assert!(val_str.contains("true"));
        assert!(val_str.contains("Object"));
    }

    #[test]
    fn test_convert_json_to_surreal_with_array() {
        let json_val = json!([1, 2, 3, "hello"]);
        let result = convert_json_to_surreal(json_val, "numbers");
        assert!(result.is_ok());
        let val = result.unwrap();
        let val_str = format!("{:?}", val);
        assert!(val_str.contains("1"));
        assert!(val_str.contains("2"));
        assert!(val_str.contains("hello"));
        assert!(val_str.contains("Array"));
    }

    #[test]
    fn test_convert_json_to_surreal_with_string() {
        let string_val = "table_name".to_string();
        let result = convert_json_to_surreal(string_val, "table");
        assert!(result.is_ok());
        let val = result.unwrap();
        assert_eq!(to_surrealql(&val), "'table_name'");
    }

    #[test]
    fn test_convert_json_to_surreal_with_number() {
        let number_val = json!(42);
        let result = convert_json_to_surreal(number_val, "count");
        assert!(result.is_ok());
        let val = result.unwrap();
        assert_eq!(to_surrealql(&val), "42");
    }

    #[test]
    fn test_convert_json_to_surreal_with_boolean() {
        let bool_val = json!(true);
        let result = convert_json_to_surreal(bool_val, "flag");
        assert!(result.is_ok());
        let val = result.unwrap();
        assert_eq!(to_surrealql(&val), "true");
    }

    #[test]
    fn test_convert_json_to_surreal_with_null() {
        let null_val = json!(null);
        let result = convert_json_to_surreal(null_val, "empty");
        assert!(result.is_ok());
        let val = result.unwrap();
        assert_eq!(to_surrealql(&val), "NONE");
    }

    #[test]
    fn test_convert_json_to_surreal_with_empty_object() {
        let json_val = json!({});
        let result = convert_json_to_surreal(json_val, "empty_obj");
        assert!(result.is_ok());
        let val = result.unwrap();
        assert_eq!(to_surrealql(&val), "{  }");
    }

    #[test]
    fn test_convert_json_to_surreal_with_nested_object() {
        let json_val = json!({
            "user": {
                "name": "Bob",
                "address": {
                    "street": "123 Main St",
                    "city": "Anytown"
                }
            }
        });
        let result = convert_json_to_surreal(json_val, "nested_data");
        assert!(result.is_ok());
        let val = result.unwrap();
        let val_str = format!("{:?}", val);
        println!("DEBUG: Complex Type Result: {}", val_str);
        assert!(val_str.contains("Bob"));
        assert!(val_str.contains("123 Main St"));
        assert!(val_str.contains("Anytown"));
    }

    #[test]
    fn test_convert_json_to_surreal_with_empty_array() {
        let json_val = json!([]);
        let result = convert_json_to_surreal(json_val, "empty_arr");
        assert!(result.is_ok());
        let val = result.unwrap();
        assert_eq!(to_surrealql(&val), "[]");
    }

    #[test]
    fn test_convert_json_to_surreal_with_special_characters() {
        let json_val = json!("Hello\nWorld\t\"quoted\"");
        let result = convert_json_to_surreal(json_val, "special");
        assert!(result.is_ok());
        let val = result.unwrap();
        let val_str = format!("{:?}", val);
        assert!(val_str.contains("Hello"));
        assert!(val_str.contains("World"));
    }

    #[test]
    fn test_convert_json_to_surreal_with_unicode() {
        let json_val = json!("Hello 世界 🌍");
        let result = convert_json_to_surreal(json_val, "unicode");
        assert!(result.is_ok());
        let val = result.unwrap();
        let val_str = format!("{:?}", val);
        assert!(val_str.contains("Hello"));
        assert!(val_str.contains("世界"));
    }

    #[test]
    fn test_convert_json_to_surreal_with_mixed_types() {
        let json_val = json!({
            "string": "hello",
            "number": 42,
            "boolean": false,
            "null": null,
            "array": [1, "two", true],
            "object": {"nested": "value"}
        });
        let result = convert_json_to_surreal(json_val, "mixed");
        assert!(result.is_ok());
        let val = result.unwrap();
        let val_str = to_surrealql(&val);
        assert!(val_str.contains("'hello'"));
        assert!(val_str.contains("42"));
        assert!(val_str.contains("false"));
        assert!(val_str.contains("NONE"));
        assert!(val_str.contains("1"));
        assert!(val_str.contains("'two'"));
        assert!(val_str.contains("true"));
        assert!(val_str.contains("nested"));
        assert!(val_str.contains("'value'"));
    }

    #[test]
    fn test_convert_json_to_surreal_error_message_format() {
        let malformed = serde_json::Value::String("invalid json {".to_string());
        let result = convert_json_to_surreal(malformed, "test_param");
        if let Ok(val) = result {
            assert_eq!(to_surrealql(&val), "'invalid json {'");
        } else {
            let error = result.unwrap_err();
            assert!(error.contains("Failed to convert parameter 'test_param'"));
        }
    }

    #[test]
    fn test_parse_target_diagnostic() {
        println!("Table person -> {}", parse_target("person".to_string()).unwrap());
        println!("Record person:john -> {}", parse_target("person:john".to_string()).unwrap());
        println!("String target -> {}", to_surrealql(&Value::String("table_name".to_string())));
    }
}
