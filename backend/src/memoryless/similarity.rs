// Standalone, deliberately UNWIRED utility (Block 11 spec point 6): the
// memoryless staged-upload dual-retrieval scan (PRD.md, Staged Upload
// Retrieval — merge a normal ChromaDB search with an in-memory cosine
// scan over this session's own staged vectors) belongs in Rust rather
// than ai_service: Rust already owns Redis and must merge these results
// with ChromaDB's own, and this is plain vector arithmetic, not model
// inference — no AI-gateway-boundary reason (Rule 9) to route it
// through FastAPI. Built ahead of its first real caller, same as every
// ai_client:: function was: no staged-upload embedding pipeline exists
// yet to feed this (that's Block 12+ / markdown/deferred.md territory).

/// Cosine similarity between two equal-length vectors, in [-1, 1].
/// Returns 0.0 for a zero-magnitude vector rather than dividing by
/// zero/producing NaN — a degenerate embedding should rank as
/// "unrelated," not silently corrupt every downstream sort.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len(), "cosine_similarity requires equal-length vectors");

    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm_a = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

/// Ranks candidates by cosine similarity to `query`, highest first.
/// Brute-force, no indexing — matches PRD.md's own reasoning ("a
/// handful of chunks at most... negligible cost at this scale").
pub fn rank_by_similarity<'a, T>(query: &[f32], candidates: &'a [(T, Vec<f32>)]) -> Vec<(&'a T, f32)> {
    let mut scored: Vec<(&T, f32)> = candidates
        .iter()
        .map(|(item, vector)| (item, cosine_similarity(query, vector)))
        .collect();
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    scored
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_vectors_have_similarity_one() {
        let v = vec![1.0, 2.0, 3.0];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn orthogonal_vectors_have_similarity_zero() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!(cosine_similarity(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn opposite_vectors_have_similarity_negative_one() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        assert!((cosine_similarity(&a, &b) + 1.0).abs() < 1e-6);
    }

    #[test]
    fn zero_vector_returns_zero_not_nan() {
        let a = vec![0.0, 0.0];
        let b = vec![1.0, 2.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn rank_by_similarity_orders_highest_first() {
        let query = vec![1.0, 0.0];
        let candidates = vec![
            ("far", vec![0.0, 1.0]),
            ("close", vec![1.0, 0.1]),
            ("exact", vec![1.0, 0.0]),
        ];
        let ranked = rank_by_similarity(&query, &candidates);
        assert_eq!(ranked[0].0, &"exact");
        assert_eq!(ranked[1].0, &"close");
        assert_eq!(ranked[2].0, &"far");
    }
}
