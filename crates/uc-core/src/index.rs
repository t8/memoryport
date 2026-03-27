use crate::models::{Chunk, ChunkType, QueryParams, SearchResult, SessionSummary};
use arrow_array::types::Float32Type;
use arrow_array::{
    Array, ArrayRef, FixedSizeListArray, Int64Array, RecordBatch, RecordBatchIterator,
    StringArray, UInt32Array,
};
use arrow_schema::{DataType, Field, Schema, SchemaRef};
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;
use tracing::debug;

#[derive(Debug, Error)]
pub enum IndexError {
    #[error("LanceDB error: {0}")]
    Lance(#[from] lancedb::error::Error),
    #[error("Arrow error: {0}")]
    Arrow(#[from] arrow_schema::ArrowError),
    #[error("no results")]
    NoResults,
}

const TABLE_NAME: &str = "chunks";

/// Build the Arrow schema for the chunks table.
pub fn build_schema(dimensions: usize) -> SchemaRef {
    Arc::new(Schema::new(vec![
        Field::new(
            "vector",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                dimensions as i32,
            ),
            true,
        ),
        Field::new("chunk_id", DataType::Utf8, false),
        Field::new("arweave_tx_id", DataType::Utf8, false),
        Field::new("batch_index", DataType::UInt32, false),
        Field::new("user_id", DataType::Utf8, false),
        Field::new("session_id", DataType::Utf8, false),
        Field::new("chunk_type", DataType::Utf8, false),
        Field::new("role", DataType::Utf8, true),
        Field::new("timestamp", DataType::Int64, false),
        Field::new("content", DataType::Utf8, false),
        Field::new("token_count", DataType::UInt32, false),
        Field::new("metadata_json", DataType::Utf8, true),
    ]))
}

/// Manages the LanceDB index for chunk storage and retrieval.
pub struct Index {
    #[allow(dead_code)]
    db: lancedb::Connection,
    table: lancedb::Table,
    dimensions: usize,
    last_checkout: std::sync::atomic::AtomicU64,
    insert_count: std::sync::atomic::AtomicU32,
}

impl Index {
    /// Ensure we're reading the latest version of the table.
    /// Throttled to once per second to avoid excessive metadata reads at scale.
    async fn checkout_latest(&self) -> Result<(), IndexError> {
        // Always checkout — cross-process writes (proxy → server) require fresh snapshots
        self.table.checkout_latest().await?;
        Ok(())
    }
}

impl Index {
    /// Open or create the LanceDB database and chunks table.
    pub async fn open(db_path: impl AsRef<Path>, dimensions: usize) -> Result<Self, IndexError> {
        let db_path_str = db_path.as_ref().to_string_lossy().to_string();
        let db = lancedb::connect(&db_path_str).execute().await?;

        let table_names = db.table_names().execute().await?;
        let table = if table_names.contains(&TABLE_NAME.to_string()) {
            db.open_table(TABLE_NAME).execute().await?
        } else {
            let schema = build_schema(dimensions);
            db.create_empty_table(TABLE_NAME, schema).execute().await?
        };

        // Create scalar indexes for fast filtered queries (idempotent — no-op if they exist)
        let _ = table
            .create_index(&["user_id"], lancedb::index::Index::BTree(Default::default()))
            .execute()
            .await;
        let _ = table
            .create_index(&["session_id"], lancedb::index::Index::BTree(Default::default()))
            .execute()
            .await;
        let _ = table
            .create_index(&["timestamp"], lancedb::index::Index::BTree(Default::default()))
            .execute()
            .await;

        // Compact fragmented data on startup if needed.
        // Each insert creates a new fragment; too many fragments degrades query performance.
        let row_count = table.count_rows(None).await.unwrap_or(0);
        if row_count > 0 {
            let bg_table = table.clone();
            tokio::spawn(async move {
                match bg_table.optimize(lancedb::table::OptimizeAction::Compact { options: Default::default(), remap_options: None }).await {
                    Ok(_) => tracing::info!("compaction complete"),
                    Err(e) => tracing::warn!(error = %e, "compaction failed (non-fatal)"),
                }
            });
        }

        debug!(path = %db_path_str, dimensions, rows = row_count, "opened LanceDB index");

        Ok(Self {
            db,
            table,
            dimensions,
            last_checkout: std::sync::atomic::AtomicU64::new(0),
            insert_count: std::sync::atomic::AtomicU32::new(0),
        })
    }

    /// Insert chunks with their embedding vectors into the index.
    pub async fn insert(
        &self,
        entries: &[(Chunk, Vec<f32>, String, u32)],
        user_id: &str,
    ) -> Result<(), IndexError> {
        if entries.is_empty() {
            return Ok(());
        }

        let schema = build_schema(self.dimensions);
        let batch = build_record_batch(entries, user_id, &schema, self.dimensions)?;

        let batches = RecordBatchIterator::new(vec![Ok(batch)], schema);
        self.table.add(batches).execute().await?;

        let count = self.insert_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        debug!(count = entries.len(), inserts = count, "inserted chunks into index");

        // Auto-compact every 100 inserts to prevent fragment buildup
        if count % 100 == 0 {
            let bg_table = self.table.clone();
            tokio::spawn(async move {
                match bg_table.optimize(lancedb::table::OptimizeAction::Compact { options: Default::default(), remap_options: None }).await {
                    Ok(_) => tracing::debug!("periodic compaction complete"),
                    Err(e) => tracing::warn!(error = %e, "periodic compaction failed"),
                }
            });
        }

        Ok(())
    }

    /// Vector similarity search with metadata filtering.
    pub async fn search(
        &self,
        query_vector: &[f32],
        params: &QueryParams,
    ) -> Result<Vec<SearchResult>, IndexError> {
        let mut filter = format!("user_id = '{}'", sanitize_sql(&params.user_id));

        if let Some(ref sid) = params.session_id {
            filter.push_str(&format!(" AND session_id = '{}'", sanitize_sql(sid)));
        }
        if let Some(ref ct) = params.chunk_type {
            filter.push_str(&format!(" AND chunk_type = '{}'", ct.as_str()));
        }
        if let Some((start, end)) = params.time_range {
            filter.push_str(&format!(" AND timestamp >= {start} AND timestamp <= {end}"));
        }

        self.checkout_latest().await?;
        let results: Vec<RecordBatch> = self.table
            .query()
            .nearest_to(query_vector)?
            // Brute force is faster than IVF_FLAT at this scale with compacted data
            .only_if(filter)
            .limit(params.top_k)
            .select(lancedb::query::Select::columns(&[
                "chunk_id", "session_id", "chunk_type", "role",
                "timestamp", "content", "arweave_tx_id",
            ]))
            .execute()
            .await?
            .try_collect()
            .await?;

        let mut search_results = Vec::new();
        for batch in &results {
            let parsed = parse_search_results(batch)?;
            search_results.extend(parsed);
        }

        Ok(search_results)
    }

    /// Get the most recent chunks for a user+session (no vector search).
    pub async fn get_recent(
        &self,
        user_id: &str,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, IndexError> {
        let filter = format!(
            "user_id = '{}' AND session_id = '{}'",
            sanitize_sql(user_id),
            sanitize_sql(session_id)
        );

        self.checkout_latest().await?;
        let results: Vec<RecordBatch> = self.table
            .query()
            .only_if(filter)
            .limit(limit)
            .execute()
            .await?
            .try_collect()
            .await?;

        let mut search_results = Vec::new();
        for batch in &results {
            let parsed = parse_search_results(batch)?;
            search_results.extend(parsed);
        }

        // Sort by timestamp descending (most recent first)
        search_results.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        Ok(search_results)
    }

    /// Get all chunks for a specific session, ordered by timestamp.
    /// Skips checkout_latest for speed — session data doesn't change after creation.
    pub async fn get_all_for_session(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> Result<Vec<SearchResult>, IndexError> {
        let filter = format!(
            "user_id = '{}' AND session_id = '{}'",
            sanitize_sql(user_id),
            sanitize_sql(session_id)
        );

        self.checkout_latest().await?;
        let results: Vec<RecordBatch> = self.table
            .query()
            .only_if(filter)
            .limit(1_000_000) // No implicit limit — return all chunks in session
            .execute()
            .await?
            .try_collect()
            .await?;

        let mut search_results = Vec::new();
        for batch in &results {
            let parsed = parse_search_results(batch)?;
            search_results.extend(parsed);
        }

        search_results.sort_by_key(|r| r.timestamp);
        Ok(search_results)
    }

    /// Get all chunks for a user (for analytics aggregation).
    pub async fn get_all_chunks(&self, user_id: &str) -> Result<Vec<SearchResult>, IndexError> {
        let filter = format!("user_id = '{}'", sanitize_sql(user_id));
        self.checkout_latest().await?;
        let results: Vec<RecordBatch> = self.table
            .query()
            .only_if(filter)
            .limit(1_000_000) // No implicit limit
            .execute()
            .await?
            .try_collect()
            .await?;

        let mut all = Vec::new();
        for batch in &results {
            let parsed = parse_search_results(batch)?;
            all.extend(parsed);
        }
        Ok(all)
    }

    /// List all distinct sessions for a user with summary info.
    pub async fn list_sessions(&self, user_id: &str) -> Result<Vec<SessionSummary>, IndexError> {
        let filter = format!("user_id = '{}'", sanitize_sql(user_id));

        self.checkout_latest().await?;
        let row_count = self.table.count_rows(None).await.unwrap_or(10000) as usize;
        let results: Vec<RecordBatch> = self.table
            .query()
            .only_if(filter)
            .select(lancedb::query::Select::columns(&["session_id", "timestamp"]))
            .limit(row_count + 1000) // Ensure we get all rows (no implicit limit)
            .execute()
            .await?
            .try_collect()
            .await?;

        let mut sessions: std::collections::HashMap<String, SessionSummary> =
            std::collections::HashMap::new();

        for batch in &results {
            let session_ids = batch
                .column_by_name("session_id")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());
            let timestamps = batch
                .column_by_name("timestamp")
                .and_then(|c| c.as_any().downcast_ref::<Int64Array>());

            if let (Some(sids), Some(tss)) = (session_ids, timestamps) {
                for i in 0..batch.num_rows() {
                    let sid = sids.value(i).to_string();
                    let ts = tss.value(i);
                    let entry = sessions.entry(sid.clone()).or_insert(SessionSummary {
                        session_id: sid,
                        chunk_count: 0,
                        first_timestamp: ts,
                        last_timestamp: ts,
                    });
                    entry.chunk_count += 1;
                    entry.first_timestamp = entry.first_timestamp.min(ts);
                    entry.last_timestamp = entry.last_timestamp.max(ts);
                }
            }
        }

        let mut summaries: Vec<SessionSummary> = sessions.into_values().collect();
        summaries.sort_by(|a, b| b.last_timestamp.cmp(&a.last_timestamp));
        Ok(summaries)
    }

    /// Count total rows, optionally filtered by user_id.
    pub async fn count(&self, user_id: Option<&str>) -> Result<usize, IndexError> {
        self.checkout_latest().await?;
        let filter = user_id.map(|uid| format!("user_id = '{}'", sanitize_sql(uid)));
        let count = self.table.count_rows(filter).await?;
        Ok(count)
    }

    /// Compact fragmented data files. Merges small fragments into larger ones
    /// and prunes old versions, dramatically improving query performance.
    pub async fn optimize(&self) -> Result<(), IndexError> {
        self.table.optimize(lancedb::table::OptimizeAction::Compact { options: Default::default(), remap_options: None }).await?;
        Ok(())
    }
}

/// Build an Arrow RecordBatch from chunk entries.
fn build_record_batch(
    entries: &[(Chunk, Vec<f32>, String, u32)],
    user_id: &str,
    schema: &SchemaRef,
    dimensions: usize,
) -> Result<RecordBatch, IndexError> {
    let vector_array = FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
        entries.iter().map(|(_, vec, _, _)| {
            Some(vec.iter().copied().map(Some).collect::<Vec<_>>())
        }),
        dimensions as i32,
    );

    let chunk_id_strings: Vec<String> = entries.iter().map(|(c, _, _, _)| c.id.to_string()).collect();
    let tx_id_strings: Vec<&str> = entries.iter().map(|(_, _, tx, _)| tx.as_str()).collect();
    let batch_indices: Vec<u32> = entries.iter().map(|(_, _, _, idx)| *idx).collect();
    let session_ids: Vec<String> = entries.iter().map(|(c, _, _, _)| c.session_id.clone()).collect();
    let chunk_types: Vec<&str> = entries.iter().map(|(c, _, _, _)| c.chunk_type.as_str()).collect();
    let roles: Vec<Option<&str>> = entries.iter().map(|(c, _, _, _)| c.role.map(|r| r.as_str())).collect();
    let timestamps: Vec<i64> = entries.iter().map(|(c, _, _, _)| c.timestamp).collect();
    let contents: Vec<&str> = entries.iter().map(|(c, _, _, _)| c.content.as_str()).collect();
    let token_counts: Vec<u32> = entries.iter().map(|(c, _, _, _)| c.metadata.token_count).collect();
    let metadata_jsons: Vec<Option<String>> = entries
        .iter()
        .map(|(c, _, _, _)| serde_json::to_string(&c.metadata).ok())
        .collect();

    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(vector_array) as ArrayRef,
            Arc::new(StringArray::from_iter_values(chunk_id_strings.iter().map(|s| s.as_str()))) as ArrayRef,
            Arc::new(StringArray::from_iter_values(tx_id_strings.iter().copied())) as ArrayRef,
            Arc::new(UInt32Array::from(batch_indices)) as ArrayRef,
            Arc::new(StringArray::from_iter_values(entries.iter().map(|_| user_id))) as ArrayRef,
            Arc::new(StringArray::from_iter_values(session_ids.iter().map(|s| s.as_str()))) as ArrayRef,
            Arc::new(StringArray::from_iter_values(chunk_types.iter().copied())) as ArrayRef,
            Arc::new(StringArray::from(roles.iter().map(|r| *r).collect::<Vec<Option<&str>>>())) as ArrayRef,
            Arc::new(Int64Array::from(timestamps)) as ArrayRef,
            Arc::new(StringArray::from_iter_values(contents.iter().copied())) as ArrayRef,
            Arc::new(UInt32Array::from(token_counts)) as ArrayRef,
            Arc::new(StringArray::from(
                metadata_jsons.iter().map(|s| s.as_deref()).collect::<Vec<Option<&str>>>(),
            )) as ArrayRef,
        ],
    ).map_err(|e| IndexError::Arrow(e))?;

    Ok(batch)
}

/// Parse search result RecordBatches into SearchResult structs.
fn parse_search_results(batch: &RecordBatch) -> Result<Vec<SearchResult>, IndexError> {
    let n = batch.num_rows();
    if n == 0 {
        return Ok(Vec::new());
    }

    let chunk_ids = batch
        .column_by_name("chunk_id")
        .and_then(|c| c.as_any().downcast_ref::<StringArray>())
        .ok_or(IndexError::NoResults)?;
    let session_ids = batch
        .column_by_name("session_id")
        .and_then(|c| c.as_any().downcast_ref::<StringArray>())
        .ok_or(IndexError::NoResults)?;
    let chunk_types = batch
        .column_by_name("chunk_type")
        .and_then(|c| c.as_any().downcast_ref::<StringArray>())
        .ok_or(IndexError::NoResults)?;
    let roles = batch
        .column_by_name("role")
        .and_then(|c| c.as_any().downcast_ref::<StringArray>());
    let timestamps = batch
        .column_by_name("timestamp")
        .and_then(|c| c.as_any().downcast_ref::<Int64Array>())
        .ok_or(IndexError::NoResults)?;
    let contents = batch
        .column_by_name("content")
        .and_then(|c| c.as_any().downcast_ref::<StringArray>())
        .ok_or(IndexError::NoResults)?;
    let tx_ids = batch
        .column_by_name("arweave_tx_id")
        .and_then(|c| c.as_any().downcast_ref::<StringArray>())
        .ok_or(IndexError::NoResults)?;

    // LanceDB adds a _distance column for vector search results
    let distances = batch
        .column_by_name("_distance")
        .and_then(|c| c.as_any().downcast_ref::<arrow_array::Float32Array>());

    // Parse source info from metadata_json column
    let metadata_jsons = batch
        .column_by_name("metadata_json")
        .and_then(|c| c.as_any().downcast_ref::<StringArray>());

    let mut results = Vec::with_capacity(n);
    for i in 0..n {
        let chunk_type: ChunkType = chunk_types.value(i).parse().unwrap_or(ChunkType::Conversation);
        let role = roles
            .and_then(|r| if r.is_null(i) { None } else { Some(r.value(i)) })
            .and_then(|s| s.parse().ok());
        let score = distances.map(|d| 1.0 - d.value(i)).unwrap_or(0.0);

        // Extract source from metadata JSON
        let (source_integration, source_model) = metadata_jsons
            .and_then(|m| if m.is_null(i) { None } else { Some(m.value(i)) })
            .and_then(|json_str| serde_json::from_str::<serde_json::Value>(json_str).ok())
            .map(|v| {
                let si = v.get("source_integration").and_then(|s| s.as_str()).map(|s| s.to_string());
                let sm = v.get("source_model").and_then(|s| s.as_str()).map(|s| s.to_string());
                (si, sm)
            })
            .unwrap_or((None, None));

        results.push(SearchResult {
            chunk_id: chunk_ids.value(i).to_string(),
            session_id: session_ids.value(i).to_string(),
            chunk_type,
            role,
            timestamp: timestamps.value(i),
            content: contents.value(i).to_string(),
            score,
            arweave_tx_id: tx_ids.value(i).to_string(),
            source_integration,
            source_model,
        });
    }

    Ok(results)
}

/// Basic SQL string sanitization to prevent injection.
fn sanitize_sql(s: &str) -> String {
    s.replace('\'', "''")
}
