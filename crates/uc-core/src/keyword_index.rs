//! BM25 keyword search index using Tantivy.
//!
//! Provides lexical search alongside the vector index (LanceDB). At query time,
//! both are searched in parallel and results are fused with Reciprocal Rank Fusion.
//! This catches entity-specific queries ("name of my hamster", "airline on Valentine's
//! day") that embedding-based search misses.

use std::path::{Path, PathBuf};
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::*;
use tantivy::{doc, Index, IndexReader, IndexWriter, ReloadPolicy};
use thiserror::Error;
use tracing::{debug, warn};

#[derive(Debug, Error)]
pub enum KeywordIndexError {
    #[error("tantivy error: {0}")]
    Tantivy(#[from] tantivy::TantivyError),
    #[error("query parse error: {0}")]
    QueryParse(#[from] tantivy::query::QueryParserError),
}

/// Result from a BM25 keyword search.
#[derive(Debug, Clone)]
pub struct KeywordSearchResult {
    pub chunk_id: String,
    pub session_id: String,
    pub user_id: String,
    pub content: String,
    pub score: f32,
}

/// BM25 keyword index backed by Tantivy.
#[allow(dead_code)]
pub struct KeywordIndex {
    index: Index,
    reader: IndexReader,
    writer: tokio::sync::Mutex<IndexWriter>,
    schema: Schema,
    f_chunk_id: Field,
    f_session_id: Field,
    f_user_id: Field,
    f_content: Field,
    f_content_stored: Field,
}

impl KeywordIndex {
    /// Open or create a keyword index at the given path.
    pub fn open(index_path: &Path) -> Result<Self, KeywordIndexError> {
        let keyword_path = index_path.join("keywords");
        std::fs::create_dir_all(&keyword_path).ok();

        let mut schema_builder = Schema::builder();
        let f_chunk_id = schema_builder.add_text_field("chunk_id", STRING | STORED);
        let f_session_id = schema_builder.add_text_field("session_id", STRING | STORED);
        let f_user_id = schema_builder.add_text_field("user_id", STRING);
        let f_content = schema_builder.add_text_field("content", TEXT);
        let f_content_stored = schema_builder.add_text_field("content_stored", STORED);
        let schema = schema_builder.build();

        let index = if keyword_path.join("meta.json").exists() {
            Index::open_in_dir(&keyword_path)?
        } else {
            Index::create_in_dir(&keyword_path, schema.clone())?
        };

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;

        let writer = index.writer(50_000_000)?; // 50MB heap

        Ok(Self {
            index,
            reader,
            writer: tokio::sync::Mutex::new(writer),
            schema,
            f_chunk_id,
            f_session_id,
            f_user_id,
            f_content,
            f_content_stored,
        })
    }

    /// Index a chunk's text content for BM25 search.
    pub async fn index_chunk(
        &self,
        chunk_id: &str,
        session_id: &str,
        user_id: &str,
        content: &str,
    ) -> Result<(), KeywordIndexError> {
        let writer = self.writer.lock().await;
        writer.add_document(doc!(
            self.f_chunk_id => chunk_id,
            self.f_session_id => session_id,
            self.f_user_id => user_id,
            self.f_content => content,
            self.f_content_stored => content,
        ))?;
        Ok(())
    }

    /// Commit pending writes to disk. Call after a batch of inserts.
    pub async fn commit(&self) -> Result<(), KeywordIndexError> {
        let mut writer = self.writer.lock().await;
        writer.commit()?;
        Ok(())
    }

    /// Search for chunks matching the query text using BM25 scoring.
    pub fn search(
        &self,
        query_text: &str,
        user_id: &str,
        top_k: usize,
    ) -> Result<Vec<KeywordSearchResult>, KeywordIndexError> {
        let searcher = self.reader.searcher();

        // Parse query against the content field
        let query_parser = QueryParser::for_index(&self.index, vec![self.f_content]);
        let query = query_parser.parse_query(query_text)?;

        let top_docs = searcher.search(&query, &TopDocs::with_limit(top_k * 2))?;

        let mut results = Vec::new();
        for (score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_address)?;

            let uid = doc
                .get_first(self.f_user_id)
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if uid != user_id {
                continue;
            }

            let chunk_id = doc
                .get_first(self.f_chunk_id)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let session_id = doc
                .get_first(self.f_session_id)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let content = doc
                .get_first(self.f_content_stored)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            results.push(KeywordSearchResult {
                chunk_id,
                session_id,
                user_id: user_id.to_string(),
                content,
                score,
            });

            if results.len() >= top_k {
                break;
            }
        }

        debug!(query = %query_text, hits = results.len(), "BM25 keyword search");
        Ok(results)
    }

    /// Search for specific entities (proper nouns, quoted strings) extracted from the query.
    /// More targeted than full-text search — finds "Alice" or "Bali" directly.
    pub fn search_entities(
        &self,
        query_text: &str,
        user_id: &str,
        top_k: usize,
    ) -> Result<Vec<KeywordSearchResult>, KeywordIndexError> {
        // Extract potential entities: quoted strings and capitalized multi-word sequences
        let mut entities = Vec::new();

        // Quoted strings: 'X' or "X"
        let mut in_quote = false;
        let mut current = String::new();
        for c in query_text.chars() {
            if c == '\'' || c == '"' {
                if in_quote && current.len() > 2 {
                    entities.push(current.clone());
                }
                current.clear();
                in_quote = !in_quote;
            } else if in_quote {
                current.push(c);
            }
        }

        // Capitalized words (potential proper nouns), skip sentence starters
        let words: Vec<&str> = query_text.split_whitespace().collect();
        for (i, word) in words.iter().enumerate() {
            let clean = word.trim_matches(|c: char| !c.is_alphanumeric());
            if clean.len() > 2 && clean.chars().next().map_or(false, |c| c.is_uppercase()) && i > 0 {
                entities.push(clean.to_string());
            }
        }

        if entities.is_empty() {
            return Ok(Vec::new());
        }

        // Search for each entity and merge results
        let mut all_results: std::collections::HashMap<String, KeywordSearchResult> = std::collections::HashMap::new();
        let searcher = self.reader.searcher();
        let query_parser = QueryParser::for_index(&self.index, vec![self.f_content]);

        for entity in &entities {
            // Use quotes for phrase matching
            let phrase_query = format!("\"{}\"", entity);
            if let Ok(query) = query_parser.parse_query(&phrase_query) {
                if let Ok(top_docs) = searcher.search(&query, &TopDocs::with_limit(top_k)) {
                    for (score, doc_address) in top_docs {
                        if let Ok(doc) = searcher.doc::<TantivyDocument>(doc_address) {
                            let uid = doc.get_first(self.f_user_id).and_then(|v| v.as_str()).unwrap_or("");
                            if uid != user_id { continue; }

                            let chunk_id = doc.get_first(self.f_chunk_id).and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let entry = all_results.entry(chunk_id.clone()).or_insert(KeywordSearchResult {
                                chunk_id,
                                session_id: doc.get_first(self.f_session_id).and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                user_id: user_id.to_string(),
                                content: doc.get_first(self.f_content_stored).and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                score: 0.0,
                            });
                            entry.score += score; // Accumulate scores across entity matches
                        }
                    }
                }
            }
        }

        let mut results: Vec<KeywordSearchResult> = all_results.into_values().collect();
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(top_k);

        debug!(entities = ?entities, hits = results.len(), "BM25 entity search");
        Ok(results)
    }

    /// Delete all documents for a user (for index rebuilds).
    pub async fn delete_user(&self, user_id: &str) -> Result<(), KeywordIndexError> {
        let term = tantivy::Term::from_field_text(self.f_user_id, user_id);
        let mut writer = self.writer.lock().await;
        writer.delete_term(term);
        writer.commit()?;
        Ok(())
    }

    /// Delete all documents (for test/benchmark resets).
    pub async fn clear(&self) -> Result<(), KeywordIndexError> {
        let mut writer = self.writer.lock().await;
        writer.delete_all_documents()?;
        writer.commit()?;
        Ok(())
    }
}
