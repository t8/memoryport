use tracing::debug;
use uc_embeddings::EmbeddingProvider;

/// Embedding-based retrieval gate (Gate 2).
/// Compares query embeddings against pre-computed exemplar centroids
/// to decide if retrieval is likely to be useful.
pub struct RetrievalGate {
    retrieve_centroid: Vec<f32>,
    skip_centroid: Vec<f32>,
    /// Bias toward retrieval. 0.0 = neutral, positive = favor retrieval.
    retrieve_bias: f32,
}

// Exemplar queries that typically need retrieval — mix of memory references AND
// general knowledge questions that stored context could answer
const RETRIEVE_EXEMPLARS: &[&str] = &[
    // Memory references
    "What did we discuss about authentication last week?",
    "Can you remind me what we decided about the database?",
    "What was that API endpoint we talked about?",
    "Summarize our conversation from yesterday",
    "What approach did we agree on for the cache layer?",
    "Tell me about the deployment strategy we planned",
    "How did we solve that memory leak issue?",
    "Can you recall the error handling approach we discussed?",
    // General knowledge questions that benefit from stored context
    "How does the pricing model work?",
    "What is Verto?",
    "Explain the architecture of this system",
    "What are the main components?",
    "How does the encryption work?",
    "What integrations does the system support?",
    "How does data flow through the pipeline?",
    "What were the requirements for the new feature?",
    "How do users authenticate?",
    "What testing strategy are we using?",
    "Tell me about the project roadmap",
    "What technology stack are we using?",
];

// Exemplar queries that typically don't need retrieval
const SKIP_EXEMPLARS: &[&str] = &[
    "Hello, how are you?",
    "Thanks for your help",
    "Fix the bug on line 42",
    "Run the test suite",
    "Add a docstring to this function",
    "What is 2 + 2?",
    "Refactor this into smaller functions",
    "Make this variable name more descriptive",
    "Remove the unused import",
    "Format this code properly",
    "Write a unit test for this function",
    "Convert this to async",
    "Add error handling here",
    "Rename this to something clearer",
    "Move this function to a separate file",
    "What does this regex do?",
    "Simplify this conditional logic",
    "Add logging to this endpoint",
    "Create a new file called config.rs",
    "Update the README with these changes",
];

impl RetrievalGate {
    /// Initialize the gate by embedding all exemplars and computing centroids.
    pub async fn init(embeddings: &dyn EmbeddingProvider) -> Result<Self, uc_embeddings::EmbeddingError> {
        debug!("initializing retrieval gate with {} + {} exemplars", RETRIEVE_EXEMPLARS.len(), SKIP_EXEMPLARS.len());

        let retrieve_vecs = embeddings.embed_batch(RETRIEVE_EXEMPLARS).await?;
        let skip_vecs = embeddings.embed_batch(SKIP_EXEMPLARS).await?;

        let retrieve_centroid = compute_centroid(&retrieve_vecs);
        let skip_centroid = compute_centroid(&skip_vecs);

        debug!("retrieval gate initialized");

        Ok(Self {
            retrieve_centroid,
            skip_centroid,
            // Bias toward retrieval — it's better to retrieve unnecessarily
            // (Gate 3 will filter low-quality results) than to miss relevant context.
            retrieve_bias: 0.05,
        })
    }

    /// Given a query embedding, decide if retrieval is likely to be useful.
    /// Returns true if the query is closer to "needs retrieval" exemplars
    /// (with a bias toward retrieval to avoid missing relevant context).
    pub fn should_retrieve(&self, query_embedding: &[f32]) -> bool {
        let retrieve_sim = cosine_similarity(query_embedding, &self.retrieve_centroid);
        let skip_sim = cosine_similarity(query_embedding, &self.skip_centroid);

        debug!(
            retrieve_sim = format!("{:.4}", retrieve_sim),
            skip_sim = format!("{:.4}", skip_sim),
            bias = format!("{:.4}", self.retrieve_bias),
            "gate 2 similarity scores"
        );

        (retrieve_sim + self.retrieve_bias) > skip_sim
    }
}

fn compute_centroid(vectors: &[Vec<f32>]) -> Vec<f32> {
    if vectors.is_empty() {
        return Vec::new();
    }
    let dims = vectors[0].len();
    let n = vectors.len() as f32;
    let mut centroid = vec![0.0_f32; dims];
    for v in vectors {
        for (i, val) in v.iter().enumerate() {
            centroid[i] += val;
        }
    }
    for val in &mut centroid {
        *val /= n;
    }
    // Normalize to unit length for cosine similarity
    let norm: f32 = centroid.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for val in &mut centroid {
            *val /= norm;
        }
    }
    centroid
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 0.001);
    }

    #[test]
    fn test_compute_centroid() {
        let vecs = vec![
            vec![1.0, 0.0],
            vec![0.0, 1.0],
        ];
        let c = compute_centroid(&vecs);
        // Average is [0.5, 0.5], normalized to unit length
        assert_eq!(c.len(), 2);
        let norm: f32 = c.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.01);
    }
}
