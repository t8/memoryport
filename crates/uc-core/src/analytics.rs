use crate::index::Index;
use serde::Serialize;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AnalyticsError {
    #[error("index error: {0}")]
    Index(#[from] crate::index::IndexError),
    #[error("lance error: {0}")]
    Lance(#[from] lancedb::error::Error),
}

#[derive(Debug, Clone, Serialize)]
pub struct AnalyticsData {
    /// Chunks per day: [{ date: "2026-03-25", count: 12 }]
    pub activity: Vec<ActivityPoint>,
    /// Breakdown by chunk type
    pub by_type: HashMap<String, usize>,
    /// Breakdown by source integration
    pub by_source: HashMap<String, usize>,
    /// Breakdown by model
    pub by_model: HashMap<String, usize>,
    /// Sync status breakdown
    pub sync_status: SyncStatus,
    pub total_chunks: usize,
    pub total_sessions: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActivityPoint {
    pub date: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncStatus {
    pub synced: usize,
    pub local: usize,
}

/// Compute analytics aggregates for a user by scanning all their chunks.
pub async fn compute_analytics(
    index: &Index,
    user_id: &str,
) -> Result<AnalyticsData, AnalyticsError> {
    let all_chunks = index.get_all_chunks(user_id).await?;

    let mut activity_map: HashMap<String, usize> = HashMap::new();
    let mut by_type: HashMap<String, usize> = HashMap::new();
    let by_source: HashMap<String, usize> = HashMap::new();
    let by_model: HashMap<String, usize> = HashMap::new();
    let mut sessions: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut synced = 0usize;
    let mut local = 0usize;

    for chunk in &all_chunks {
        // Activity by date
        let date = timestamp_to_date(chunk.timestamp);
        *activity_map.entry(date).or_default() += 1;

        // By type
        *by_type.entry(chunk.chunk_type.as_str().to_string()).or_default() += 1;

        // Sessions
        sessions.insert(chunk.session_id.clone());

        // Sync status
        if chunk.arweave_tx_id.starts_with("local_") {
            local += 1;
        } else {
            synced += 1;
        }
    }

    // Parse metadata for source info (best-effort from metadata_json in search results)
    // Note: SearchResult doesn't carry metadata_json, so source tagging
    // will only show in results once we add it to the query response.
    // For now, populate from what we have.

    // Sort activity by date
    let mut activity: Vec<ActivityPoint> = activity_map
        .into_iter()
        .map(|(date, count)| ActivityPoint { date, count })
        .collect();
    activity.sort_by(|a, b| a.date.cmp(&b.date));

    Ok(AnalyticsData {
        activity,
        by_type,
        by_source,
        by_model,
        sync_status: SyncStatus { synced, local },
        total_chunks: all_chunks.len(),
        total_sessions: sessions.len(),
    })
}

fn timestamp_to_date(ts_ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ts_ms)
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "unknown".into())
}
