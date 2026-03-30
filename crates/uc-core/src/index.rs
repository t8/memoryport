use crate::models::{Chunk, ChunkType, QueryParams, SearchResult, SessionSummary};
use arrow_array::types::Float32Type;
use arrow_array::{
    Array, ArrayRef, BooleanArray, FixedSizeListArray, Float32Array, Int64Array, RecordBatch,
    RecordBatchIterator, StringArray, UInt32Array,
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
const FACTS_TABLE_NAME: &str = "facts";

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

/// Build the Arrow schema for the facts table.
pub fn build_facts_schema(dimensions: usize) -> SchemaRef {
    Arc::new(Schema::new(vec![
        Field::new(
            "vector",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                dimensions as i32,
            ),
            true,
        ),
        Field::new("fact_id", DataType::Utf8, false),
        Field::new("content", DataType::Utf8, false),
        Field::new("subject", DataType::Utf8, false),
        Field::new("predicate", DataType::Utf8, false),
        Field::new("object", DataType::Utf8, false),
        Field::new("source_chunk_id", DataType::Utf8, false),
        Field::new("session_id", DataType::Utf8, false),
        Field::new("user_id", DataType::Utf8, false),
        Field::new("document_date", DataType::Int64, false),
        Field::new("event_date", DataType::Int64, true),
        Field::new("valid", DataType::Boolean, false),
        Field::new("superseded_by", DataType::Utf8, true),
        Field::new("confidence", DataType::Float32, false),
        Field::new("created_at", DataType::Int64, false),
    ]))
}

/// Result from a vector or filtered search against the facts table.
#[derive(Debug, Clone)]
pub struct FactSearchResult {
    pub fact_id: String,
    pub content: String,
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub session_id: String,
    pub document_date: i64,
    pub event_date: Option<i64>,
    pub valid: bool,
    pub confidence: f32,
    pub score: f32,
}

/// Manages the LanceDB index for chunk storage and retrieval.
pub struct Index {
    db: lancedb::Connection,
    table: lancedb::Table,
    facts_table: Option<lancedb::Table>,
    dimensions: usize,
    #[allow(dead_code)]
    last_checkout: std::sync::atomic::AtomicU64,
    insert_count: std::sync::atomic::AtomicU32,
    /// Tracks inserts since last successful compaction.
    inserts_since_compact: std::sync::atomic::AtomicU32,
    /// Serializes compaction to prevent concurrent compact operations.
    compact_lock: tokio::sync::Mutex<()>,
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

        // Open or lazily defer creation of the facts table
        let facts_table = if table_names.contains(&FACTS_TABLE_NAME.to_string()) {
            let ft = db.open_table(FACTS_TABLE_NAME).execute().await?;
            // Create scalar indexes for fast filtered queries (idempotent)
            let _ = ft
                .create_index(&["user_id"], lancedb::index::Index::BTree(Default::default()))
                .execute()
                .await;
            let _ = ft
                .create_index(&["subject"], lancedb::index::Index::BTree(Default::default()))
                .execute()
                .await;
            let _ = ft
                .create_index(&["predicate"], lancedb::index::Index::BTree(Default::default()))
                .execute()
                .await;
            let _ = ft
                .create_index(&["valid"], lancedb::index::Index::BTree(Default::default()))
                .execute()
                .await;
            Some(ft)
        } else {
            // Don't create until first insert — keeps backward compatible
            None
        };

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
            facts_table,
            dimensions,
            last_checkout: std::sync::atomic::AtomicU64::new(0),
            insert_count: std::sync::atomic::AtomicU32::new(0),
            inserts_since_compact: std::sync::atomic::AtomicU32::new(0),
            compact_lock: tokio::sync::Mutex::new(()),
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

        // Auto-compact based on fragment buildup, not fixed insert count.
        // Each insert creates a new fragment. We compact synchronously (blocking)
        // when fragment count gets too high, preventing runaway disk growth.
        let since_compact = self.inserts_since_compact.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;

        // Compact every 100 uncompacted inserts. Synchronous to ensure it
        // actually completes before more fragments accumulate.
        if since_compact >= 100 {
            // Try to acquire the compact lock (non-blocking). If another task
            // is already compacting, skip — it'll catch up.
            if let Ok(_guard) = self.compact_lock.try_lock() {
                self.inserts_since_compact.store(0, std::sync::atomic::Ordering::Relaxed);

                // Step 1: Compact fragments into larger files
                match self.table.optimize(lancedb::table::OptimizeAction::Compact {
                    options: Default::default(),
                    remap_options: None,
                }).await {
                    Ok(_) => debug!("auto-compaction complete (after {} inserts)", since_compact),
                    Err(e) => tracing::warn!(error = %e, "auto-compaction failed"),
                }

                // Step 2: Prune old versions to reclaim disk space.
                // Without pruning, every compaction leaves old fragment files on disk.
                let _ = self.table.optimize(lancedb::table::OptimizeAction::Prune {
                    older_than: Some(chrono::TimeDelta::seconds(30)),
                    delete_unverified: Some(true),
                    error_if_tagged_old_versions: Some(false),
                }).await;
            }
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

    /// Compact fragmented data files. Merges small fragments into larger ones,
    /// dramatically improving query performance and reclaiming disk space.
    pub async fn optimize(&self) -> Result<(), IndexError> {
        let _guard = self.compact_lock.lock().await;

        // Compact + prune chunks table
        self.table.optimize(lancedb::table::OptimizeAction::Compact {
            options: Default::default(),
            remap_options: None,
        }).await?;
        let _ = self.table.optimize(lancedb::table::OptimizeAction::Prune {
            older_than: Some(chrono::TimeDelta::seconds(1)),
            delete_unverified: Some(true),
            error_if_tagged_old_versions: Some(false),
        }).await;
        self.inserts_since_compact.store(0, std::sync::atomic::Ordering::Relaxed);

        // Compact + prune facts table
        if let Some(ref ft) = self.facts_table {
            let _ = ft.optimize(lancedb::table::OptimizeAction::Compact {
                options: Default::default(),
                remap_options: None,
            }).await;
            let _ = ft.optimize(lancedb::table::OptimizeAction::Prune {
                older_than: Some(chrono::TimeDelta::seconds(1)),
                delete_unverified: Some(true),
                error_if_tagged_old_versions: Some(false),
            }).await;
        }

        tracing::info!("manual compaction + prune complete");
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Facts table operations
    // -----------------------------------------------------------------------

    /// Lazily ensure the facts table exists, creating it on first use.
    async fn ensure_facts_table(&self) -> Result<lancedb::Table, IndexError> {
        if let Some(ref ft) = self.facts_table {
            return Ok(ft.clone());
        }
        let schema = build_facts_schema(self.dimensions);
        let ft = self.db.create_empty_table(FACTS_TABLE_NAME, schema).execute().await?;
        // Create scalar indexes for fast filtered queries
        let _ = ft
            .create_index(&["user_id"], lancedb::index::Index::BTree(Default::default()))
            .execute()
            .await;
        let _ = ft
            .create_index(&["subject"], lancedb::index::Index::BTree(Default::default()))
            .execute()
            .await;
        let _ = ft
            .create_index(&["predicate"], lancedb::index::Index::BTree(Default::default()))
            .execute()
            .await;
        let _ = ft
            .create_index(&["valid"], lancedb::index::Index::BTree(Default::default()))
            .execute()
            .await;
        Ok(ft)
    }

    /// Insert facts with their embedding vectors into the facts table.
    pub async fn insert_facts(
        &self,
        facts: &[crate::facts::Fact],
        vectors: &[Vec<f32>],
    ) -> Result<(), IndexError> {
        if facts.is_empty() {
            return Ok(());
        }

        let ft = self.ensure_facts_table().await?;
        let schema = build_facts_schema(self.dimensions);
        let batch = build_facts_record_batch(facts, vectors, &schema, self.dimensions)?;

        let batches = RecordBatchIterator::new(vec![Ok(batch)], schema);
        ft.add(batches).execute().await?;

        debug!(count = facts.len(), "inserted facts into index");
        Ok(())
    }

    /// Vector similarity search against the facts table.
    pub async fn search_facts(
        &self,
        query_vector: &[f32],
        user_id: &str,
        top_k: usize,
        valid_only: bool,
    ) -> Result<Vec<FactSearchResult>, IndexError> {
        let ft = match &self.facts_table {
            Some(ft) => ft,
            None => return Ok(Vec::new()),
        };

        let mut filter = format!("user_id = '{}'", sanitize_sql(user_id));
        if valid_only {
            filter.push_str(" AND valid = true");
        }

        ft.checkout_latest().await?;
        let results: Vec<RecordBatch> = ft
            .query()
            .nearest_to(query_vector)?
            .only_if(filter)
            .limit(top_k)
            .select(lancedb::query::Select::columns(&[
                "fact_id", "content", "subject", "predicate", "object",
                "session_id", "document_date", "event_date", "valid",
                "confidence",
            ]))
            .execute()
            .await?
            .try_collect()
            .await?;

        let mut search_results = Vec::new();
        for batch in &results {
            let parsed = parse_fact_search_results(batch)?;
            search_results.extend(parsed);
        }

        Ok(search_results)
    }

    /// Mark a fact as superseded by a newer fact.
    pub async fn mark_fact_superseded(
        &self,
        fact_id: &str,
        superseded_by: &str,
    ) -> Result<(), IndexError> {
        let ft = match &self.facts_table {
            Some(ft) => ft,
            None => return Err(IndexError::NoResults),
        };

        ft.checkout_latest().await?;

        // Read the existing row
        let filter = format!("fact_id = '{}'", sanitize_sql(fact_id));
        let rows: Vec<RecordBatch> = ft
            .query()
            .only_if(&filter)
            .limit(1)
            .execute()
            .await?
            .try_collect()
            .await?;

        if rows.is_empty() || rows[0].num_rows() == 0 {
            return Err(IndexError::NoResults);
        }

        // Delete the old row and re-insert with updated fields
        ft.delete(&filter).await?;

        let old = &rows[0];
        let n = old.num_rows();
        // Rebuild the batch with valid=false and superseded_by set
        let schema = build_facts_schema(self.dimensions);

        // Carry forward all columns from the old batch
        let vector_col = old.column_by_name("vector").unwrap().clone();
        let fact_id_col = old.column_by_name("fact_id").unwrap().clone();
        let content_col = old.column_by_name("content").unwrap().clone();
        let subject_col = old.column_by_name("subject").unwrap().clone();
        let predicate_col = old.column_by_name("predicate").unwrap().clone();
        let object_col = old.column_by_name("object").unwrap().clone();
        let source_chunk_id_col = old.column_by_name("source_chunk_id").unwrap().clone();
        let session_id_col = old.column_by_name("session_id").unwrap().clone();
        let user_id_col = old.column_by_name("user_id").unwrap().clone();
        let document_date_col = old.column_by_name("document_date").unwrap().clone();
        let event_date_col = old.column_by_name("event_date").unwrap().clone();
        let confidence_col = old.column_by_name("confidence").unwrap().clone();
        let created_at_col = old.column_by_name("created_at").unwrap().clone();

        // Build new valid and superseded_by columns
        let valid_arr = Arc::new(BooleanArray::from(vec![false; n])) as ArrayRef;
        let superseded_arr = Arc::new(StringArray::from(
            (0..n).map(|_| Some(superseded_by)).collect::<Vec<Option<&str>>>(),
        )) as ArrayRef;

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                vector_col,
                fact_id_col,
                content_col,
                subject_col,
                predicate_col,
                object_col,
                source_chunk_id_col,
                session_id_col,
                user_id_col,
                document_date_col,
                event_date_col,
                valid_arr,
                superseded_arr,
                confidence_col,
                created_at_col,
            ],
        ).map_err(|e| IndexError::Arrow(e))?;

        let batches = RecordBatchIterator::new(vec![Ok(batch)], schema);
        ft.add(batches).execute().await?;

        debug!(fact_id, superseded_by, "marked fact as superseded");
        Ok(())
    }

    /// Search facts by subject+predicate for contradiction detection.
    pub async fn search_facts_by_predicate(
        &self,
        user_id: &str,
        subject: &str,
        predicate: &str,
        valid_only: bool,
    ) -> Result<Vec<FactSearchResult>, IndexError> {
        let ft = match &self.facts_table {
            Some(ft) => ft,
            None => return Ok(Vec::new()),
        };

        let mut filter = format!(
            "user_id = '{}' AND subject = '{}' AND predicate = '{}'",
            sanitize_sql(user_id),
            sanitize_sql(subject),
            sanitize_sql(predicate),
        );
        if valid_only {
            filter.push_str(" AND valid = true");
        }

        ft.checkout_latest().await?;
        let results: Vec<RecordBatch> = ft
            .query()
            .only_if(filter)
            .limit(1000)
            .select(lancedb::query::Select::columns(&[
                "fact_id", "content", "subject", "predicate", "object",
                "session_id", "document_date", "event_date", "valid",
                "confidence",
            ]))
            .execute()
            .await?
            .try_collect()
            .await?;

        let mut search_results = Vec::new();
        for batch in &results {
            let parsed = parse_fact_search_results(batch)?;
            search_results.extend(parsed);
        }

        Ok(search_results)
    }

    /// Migrate chunks from an old user_id to a new one.
    /// Used to transition data from "default" to wallet-based user_id.
    pub async fn migrate_user_id(&self, old_id: &str, new_id: &str) -> Result<usize, IndexError> {
        self.checkout_latest().await?;
        let filter = format!("user_id = '{}'", sanitize_sql(old_id));
        let count = self.table.query()
            .only_if(&filter)
            .limit(1)
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?
            .iter()
            .map(|b| b.num_rows())
            .sum::<usize>();

        if count == 0 {
            return Ok(0);
        }

        // LanceDB update: set user_id = new_id where user_id = old_id
        self.table
            .update()
            .only_if(filter)
            .column("user_id", format!("'{}'", sanitize_sql(new_id)))
            .execute()
            .await?;

        let total = self.table.count_rows(Some(format!("user_id = '{}'", sanitize_sql(new_id)))).await.unwrap_or(0) as usize;
        tracing::info!(old_id, new_id, migrated = total, "migrated user_id");
        Ok(total)
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

/// Build an Arrow RecordBatch from fact entries and their embedding vectors.
fn build_facts_record_batch(
    facts: &[crate::facts::Fact],
    vectors: &[Vec<f32>],
    schema: &SchemaRef,
    dimensions: usize,
) -> Result<RecordBatch, IndexError> {
    let vector_array = FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
        vectors.iter().map(|vec| {
            Some(vec.iter().copied().map(Some).collect::<Vec<_>>())
        }),
        dimensions as i32,
    );

    let fact_ids: Vec<String> = facts.iter().map(|f| f.id.to_string()).collect();
    let contents: Vec<&str> = facts.iter().map(|f| f.content.as_str()).collect();
    let subjects: Vec<&str> = facts.iter().map(|f| f.subject.as_str()).collect();
    let predicates: Vec<&str> = facts.iter().map(|f| f.predicate.as_str()).collect();
    let objects: Vec<&str> = facts.iter().map(|f| f.object.as_str()).collect();
    let source_chunk_ids: Vec<&str> = facts.iter().map(|f| f.source_chunk_id.as_str()).collect();
    let session_ids: Vec<&str> = facts.iter().map(|f| f.session_id.as_str()).collect();
    let user_ids: Vec<&str> = facts.iter().map(|f| f.user_id.as_str()).collect();
    let document_dates: Vec<i64> = facts.iter().map(|f| f.document_date).collect();
    let event_dates: Vec<Option<i64>> = facts.iter().map(|f| f.event_date).collect();
    let valids: Vec<bool> = facts.iter().map(|f| f.valid).collect();
    let superseded_bys: Vec<Option<&str>> = facts.iter().map(|f| f.superseded_by.as_deref()).collect();
    let confidences: Vec<f32> = facts.iter().map(|f| f.confidence).collect();
    let created_ats: Vec<i64> = facts.iter().map(|f| f.created_at).collect();

    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(vector_array) as ArrayRef,
            Arc::new(StringArray::from_iter_values(fact_ids.iter().map(|s| s.as_str()))) as ArrayRef,
            Arc::new(StringArray::from_iter_values(contents.iter().copied())) as ArrayRef,
            Arc::new(StringArray::from_iter_values(subjects.iter().copied())) as ArrayRef,
            Arc::new(StringArray::from_iter_values(predicates.iter().copied())) as ArrayRef,
            Arc::new(StringArray::from_iter_values(objects.iter().copied())) as ArrayRef,
            Arc::new(StringArray::from_iter_values(source_chunk_ids.iter().copied())) as ArrayRef,
            Arc::new(StringArray::from_iter_values(session_ids.iter().copied())) as ArrayRef,
            Arc::new(StringArray::from_iter_values(user_ids.iter().copied())) as ArrayRef,
            Arc::new(Int64Array::from(document_dates)) as ArrayRef,
            Arc::new(Int64Array::from(event_dates)) as ArrayRef,
            Arc::new(BooleanArray::from(valids)) as ArrayRef,
            Arc::new(StringArray::from(superseded_bys)) as ArrayRef,
            Arc::new(Float32Array::from(confidences)) as ArrayRef,
            Arc::new(Int64Array::from(created_ats)) as ArrayRef,
        ],
    ).map_err(|e| IndexError::Arrow(e))?;

    Ok(batch)
}

/// Parse fact search result RecordBatches into FactSearchResult structs.
fn parse_fact_search_results(batch: &RecordBatch) -> Result<Vec<FactSearchResult>, IndexError> {
    let n = batch.num_rows();
    if n == 0 {
        return Ok(Vec::new());
    }

    let fact_ids = batch
        .column_by_name("fact_id")
        .and_then(|c| c.as_any().downcast_ref::<StringArray>())
        .ok_or(IndexError::NoResults)?;
    let contents = batch
        .column_by_name("content")
        .and_then(|c| c.as_any().downcast_ref::<StringArray>())
        .ok_or(IndexError::NoResults)?;
    let subjects = batch
        .column_by_name("subject")
        .and_then(|c| c.as_any().downcast_ref::<StringArray>())
        .ok_or(IndexError::NoResults)?;
    let predicates = batch
        .column_by_name("predicate")
        .and_then(|c| c.as_any().downcast_ref::<StringArray>())
        .ok_or(IndexError::NoResults)?;
    let objects = batch
        .column_by_name("object")
        .and_then(|c| c.as_any().downcast_ref::<StringArray>())
        .ok_or(IndexError::NoResults)?;
    let session_ids = batch
        .column_by_name("session_id")
        .and_then(|c| c.as_any().downcast_ref::<StringArray>())
        .ok_or(IndexError::NoResults)?;
    let document_dates = batch
        .column_by_name("document_date")
        .and_then(|c| c.as_any().downcast_ref::<Int64Array>())
        .ok_or(IndexError::NoResults)?;
    let event_dates = batch
        .column_by_name("event_date")
        .and_then(|c| c.as_any().downcast_ref::<Int64Array>());
    let valids = batch
        .column_by_name("valid")
        .and_then(|c| c.as_any().downcast_ref::<BooleanArray>())
        .ok_or(IndexError::NoResults)?;
    let confidences = batch
        .column_by_name("confidence")
        .and_then(|c| c.as_any().downcast_ref::<Float32Array>())
        .ok_or(IndexError::NoResults)?;

    // LanceDB adds a _distance column for vector search results
    let distances = batch
        .column_by_name("_distance")
        .and_then(|c| c.as_any().downcast_ref::<Float32Array>());

    let mut results = Vec::with_capacity(n);
    for i in 0..n {
        let event_date = event_dates
            .and_then(|ed| if ed.is_null(i) { None } else { Some(ed.value(i)) });
        let score = distances.map(|d| 1.0 - d.value(i)).unwrap_or(0.0);

        results.push(FactSearchResult {
            fact_id: fact_ids.value(i).to_string(),
            content: contents.value(i).to_string(),
            subject: subjects.value(i).to_string(),
            predicate: predicates.value(i).to_string(),
            object: objects.value(i).to_string(),
            session_id: session_ids.value(i).to_string(),
            document_date: document_dates.value(i),
            event_date,
            valid: valids.value(i),
            confidence: confidences.value(i),
            score,
        });
    }

    Ok(results)
}
