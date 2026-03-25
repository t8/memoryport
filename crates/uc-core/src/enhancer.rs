use async_trait::async_trait;
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, warn};
use uc_embeddings::llm::{LlmError, LlmProvider};

#[derive(Debug, Error)]
pub enum EnhancerError {
    #[error("LLM generation failed: {0}")]
    Llm(#[from] LlmError),
}

/// Enhanced query with expanded search terms and/or a hypothetical document.
#[derive(Debug, Clone)]
pub struct EnhancedQuery {
    /// The original query.
    pub original: String,
    /// Alternative phrasings / related search terms.
    pub expanded_queries: Vec<String>,
    /// A hypothetical ideal answer (for HyDE embedding).
    pub hyde_document: Option<String>,
}

/// Trait for query enhancement strategies.
#[async_trait]
pub trait QueryEnhancer: Send + Sync {
    async fn enhance(&self, query: &str) -> Result<EnhancedQuery, EnhancerError>;
}

/// No-op enhancer that returns the original query unchanged.
pub struct NoopEnhancer;

#[async_trait]
impl QueryEnhancer for NoopEnhancer {
    async fn enhance(&self, query: &str) -> Result<EnhancedQuery, EnhancerError> {
        Ok(EnhancedQuery {
            original: query.to_string(),
            expanded_queries: Vec::new(),
            hyde_document: None,
        })
    }
}

/// LLM-powered enhancer that generates query expansions and HyDE documents.
pub struct LlmQueryEnhancer {
    llm: Arc<dyn LlmProvider>,
    query_expansion: bool,
    hyde: bool,
}

impl LlmQueryEnhancer {
    pub fn new(llm: Arc<dyn LlmProvider>, query_expansion: bool, hyde: bool) -> Self {
        Self {
            llm,
            query_expansion,
            hyde,
        }
    }
}

#[async_trait]
impl QueryEnhancer for LlmQueryEnhancer {
    async fn enhance(&self, query: &str) -> Result<EnhancedQuery, EnhancerError> {
        let mut enhanced = EnhancedQuery {
            original: query.to_string(),
            expanded_queries: Vec::new(),
            hyde_document: None,
        };

        // Run expansion and HyDE concurrently if both enabled
        let expansion_fut = if self.query_expansion {
            Some(generate_expansions(&self.llm, query))
        } else {
            None
        };

        let hyde_fut = if self.hyde {
            Some(generate_hyde(&self.llm, query))
        } else {
            None
        };

        if let Some(fut) = expansion_fut {
            match fut.await {
                Ok(queries) => {
                    debug!(count = queries.len(), "generated query expansions");
                    enhanced.expanded_queries = queries;
                }
                Err(e) => {
                    warn!(error = %e, "query expansion failed, continuing with original");
                }
            }
        }

        if let Some(fut) = hyde_fut {
            match fut.await {
                Ok(doc) => {
                    debug!(len = doc.len(), "generated HyDE document");
                    enhanced.hyde_document = Some(doc);
                }
                Err(e) => {
                    warn!(error = %e, "HyDE generation failed, continuing with original");
                }
            }
        }

        Ok(enhanced)
    }
}

const EXPANSION_SYSTEM: &str = "You are a search query expansion assistant. Given a user's search query, generate 3-5 alternative phrasings or related search terms that would help find relevant information. Return ONLY the alternative queries, one per line, with no numbering or extra text.";

const HYDE_SYSTEM: &str = "You are a helpful assistant. Given a user's question, write a short passage (2-4 sentences) that directly answers the question. Write as if you are quoting from an authoritative source. Be specific and factual. Do not say 'I think' or hedge — write the answer as a statement of fact.";

async fn generate_expansions(
    llm: &Arc<dyn LlmProvider>,
    query: &str,
) -> Result<Vec<String>, EnhancerError> {
    let response = llm
        .generate(query, Some(EXPANSION_SYSTEM))
        .await?;

    let queries: Vec<String> = response
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .map(|l| {
            // Strip any leading numbering like "1.", "1)", "- "
            let stripped = l
                .trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == ')' || c == '-')
                .trim();
            stripped.to_string()
        })
        .filter(|l| !l.is_empty() && l.len() > 3)
        .take(5)
        .collect();

    Ok(queries)
}

async fn generate_hyde(
    llm: &Arc<dyn LlmProvider>,
    query: &str,
) -> Result<String, EnhancerError> {
    let response = llm
        .generate(query, Some(HYDE_SYSTEM))
        .await?;

    Ok(response.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_noop_enhancer() {
        let enhancer = NoopEnhancer;
        let result = enhancer.enhance("test query").await.unwrap();
        assert_eq!(result.original, "test query");
        assert!(result.expanded_queries.is_empty());
        assert!(result.hyde_document.is_none());
    }

    #[test]
    fn test_parse_expansions() {
        // Simulate LLM output parsing
        let output = "1. How does permanent storage work on Arweave?\n2. Arweave pricing model explained\n3. Cost of storing data on Arweave blockchain\n";
        let queries: Vec<String> = output
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .map(|l| {
                l.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == ')' || c == '-')
                    .trim()
                    .to_string()
            })
            .filter(|l| !l.is_empty() && l.len() > 3)
            .take(5)
            .collect();

        assert_eq!(queries.len(), 3);
        assert!(queries[0].starts_with("How does"));
    }
}
