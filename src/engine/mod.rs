use anyhow::Result;
use metrics::{counter, histogram};
use rmcp::model::{Content, RawContent};
use std::time::Instant;
use std::{collections::HashMap, time::Duration};
use surrealdb::{Surreal, engine::any::Any};
use surrealdb::types::Value;
use tracing::{debug, error, info};
use crate::utils;

/// Type alias for SurrealDB response which supports indexed access in v3
pub type IndexedResults = surrealdb::IndexedResults;

/// Response from executing a SurrealDB query
#[derive(Debug)]
#[allow(dead_code)]
pub struct Response {
    /// Query ID for tracking
    pub query_id: u64,
    /// The query that was executed
    pub query: String,
    /// Duration of the query execution
    pub duration: Duration,
    /// Error message if the query failed
    pub error: Option<String>,
    /// The result of the query
    pub result: Option<IndexedResults>,
}

impl Response {
    /// Convert the response to an MCP Tool Result
    pub fn into_mcp_result(mut self) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        if let Some(res) = self.result.as_mut() {
            // Take the first result from the response (SurrealDB v3)
            let value: surrealdb::types::Value = res
                .take(0)
                .map_err(|e: surrealdb::Error| rmcp::ErrorData::internal_error(e.to_string(), None))?;
            
            let json_value = utils::surreal_to_json(value);
            Ok(rmcp::model::CallToolResult {
                content: vec![Content::text(serde_json::to_string_pretty(&json_value).unwrap_or_default())],
                is_error: None,
                meta: None,
                structured_content: None,
            })
        } else {
            let error_msg = self
                .error
                .unwrap_or_else(|| "Unknown error".to_string());
            Err(rmcp::ErrorData::internal_error(error_msg, None))
        }
    }
}

/// Execute a SurrealQL query against the specified SurrealDB endpoint
///
/// This function executes a SurrealQL query against the provided SurrealDB client.
/// It handles parameter binding, query execution, and result formatting.
///
/// # Arguments
/// * `db` - The SurrealDB client instance
/// * `query_string` - The SurrealQL query to execute
/// * `parameters` - Optional parameters to bind to the query
/// * `query_id` - Unique identifier for tracking this query
/// * `connection_id` - Connection ID for logging purposes
///
/// # Returns
/// * `Result<Response, anyhow::Error>` - The query response or an error
pub async fn execute_query(
    db: &Surreal<Any>,
    query_id: u64,
    query_string: String,
    parameters: Option<HashMap<String, Value>>,
    connection_id: &str,
) -> Response {
    // Start the measurement timer
    let start_time = Instant::now();
    // Output debugging information
    debug!(
        connection_id = %connection_id,
        query_id,
        query_string = %query_string,
        "Executing SurrealQL query"
    );
    // Build the query string
    let mut query = db.query(&query_string);
    // Bind any parameters
    if let Some(params) = parameters {
        for (key, value) in params {
            query = query.bind((key, value));
        }
    }
    // Execute the query
    match query.await {
        Ok(res) => {
            // Get the duration of the query
            let duration = start_time.elapsed();
            // Output debugging information
            info!(
                connection_id = %connection_id,
                query_id,
                query = %query_string,
                duration_ms = duration.as_millis(),
                "Query execution succeeded"
            );
            // Update query metrics
            counter!("surrealmcp.total_queries").increment(1);
            histogram!("surrealmcp.query_duration_ms").record(duration.as_millis() as f64);
            // Return the response
            Response {
                query: query_string,
                result: Some(res),
                error: None,
                duration,
                query_id,
            }
        }
        Err(e) => {
            // Get the duration of the query
            let duration = start_time.elapsed();
            // Output debugging information
            error!(
                connection_id = %connection_id,
                query_id,
                query = %query_string,
                duration_ms = duration.as_millis(),
                error = %e,
                "Query execution failed"
            );
            // Update query metrics
            counter!("surrealmcp.total_query_errors").increment(1);
            histogram!("surrealmcp.query_duration_ms").record(duration.as_millis() as f64);
            // Return the response
            Response {
                query: query_string,
                result: None,
                error: Some(e.to_string()),
                duration,
                query_id,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::utils;

    async fn setup_db() -> Surreal<Any> {
        db::create_client_connection(
            "mem://",
            None,
            None,
            Some("test"),
            Some("test"),
        )
        .await
        .expect("Failed to connect to in-memory SurrealDB")
    }

    #[tokio::test]
    async fn test_check_health_logic() {
        let db = setup_db().await;
        // Verify healthy result
        let (healthy, version) = utils::check_health(&db).await.expect("Health check failed");
        assert!(healthy, "Instance should be healthy");
        assert!(version.starts_with('3'), "Version should be 3.x, got {}", version);
    }

    #[tokio::test]
    async fn test_complex_type_validation() {
        let db = setup_db().await;
        
        // Cleanup and Define table separately from the test query
        db.query("REMOVE TABLE IF EXISTS ComplexTypes;").await.unwrap();
        
        // Test query: First statement is CREATE
        let query = "
            CREATE ComplexTypes:['north', 'sector', 1] CONTENT {
                name: 'Composite Record',
                location: (10.0, 20.0),
                delay: 5s
            };
            SELECT *, id FROM ComplexTypes:['north', 'sector', 1];
        ";
        
        let response = execute_query(&db, 1, query.to_string(), None, "test_conn").await;
        let mcp_result = response.into_mcp_result().expect("Failed to convert to MCP result");
        
        // Parse the JSON result to get namespace names
        if let RawContent::Text(text_content) = &mcp_result.content[0].raw {
            let _json: serde_json::Value = serde_json::from_str(&text_content.text)
                .map_err(|e| rmcp::ErrorData::internal_error(format!("Failed to parse JSON: {}", e), None))
                .expect("Failed to parse JSON from MCP result"); // Added expect for test context
            
            // Verify composite ID parts are preserved in JSON
            assert!(text_content.text.contains("north"), "Missing 'north' in ID");
            assert!(text_content.text.contains("sector"), "Missing 'sector' in ID");
            assert!(text_content.text.contains("1"), "Missing '1' in ID");
            // Verify Geometry (SurrealDB 3.0 may use (10f, 20f) or (10.0, 20.0))
            assert!(text_content.text.contains("10"), "Missing longitude");
            assert!(text_content.text.contains("20"), "Missing latitude");
            // Verify Duration
            assert!(text_content.text.contains("5s") || text_content.text.contains("duration") || text_content.text.contains("secs"), "Missing or malformed duration");
        }
    }

    #[tokio::test]
    async fn test_take_zero_multi_statement_logic() {
        let db = setup_db().await;
        db.query("REMOVE TABLE IF EXISTS person;").await.unwrap();
        
        // Multi-statement query: First statement is CREATE, second is SELECT
        // MCP single-result goal: take(0) should return the CREATE result.
        let query = "CREATE person:john SET name = 'John'; SELECT * FROM person;";
        let response = execute_query(&db, 2, query.to_string(), None, "test_conn").await;
        
        let mcp_result = response.into_mcp_result().expect("Failed to convert multi-statement result");
        let content = &mcp_result.content[0];
        if let rmcp::model::RawContent::Text(raw_text) = &content.raw {
            let text = &raw_text.text;
            println!("DEBUG: Multi-statement result: {}", text);
            // It should contain the record we just created in the first statement
            // In v3 JSON, this is { "id": { "RecordId": { "key": { "String": "john" }, "table": "person" } }, "name": { "String": "John" } }
            assert!(text.contains("John"), "Result should be from the first statement (CREATE)");
            assert!(text.contains("john") && text.contains("person"), "Result should include the record ID parts");
        }
    }
}
