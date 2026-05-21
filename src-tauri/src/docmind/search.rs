use std::cmp::Ordering;

use super::models::SearchResultView;

pub fn normalize_query(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .filter(|part| !part.is_empty())
        .map(|part| part.to_lowercase())
        .collect()
}

pub fn score_result(query_terms: &[String], result: &SearchResultView) -> f32 {
    if query_terms.is_empty() {
        return result.score;
    }

    let haystack = format!(
        "{} {} {} {}",
        result.file_name, result.path, result.heading, result.snippet
    )
    .to_lowercase();

    let mut score = 0.0;
    for term in query_terms {
        if haystack.contains(term) {
            score += 1.0;
        }
    }

    result.score + score
}

pub fn sort_results(mut results: Vec<SearchResultView>) -> Vec<SearchResultView> {
    results.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(Ordering::Equal)
    });
    results
}

pub fn filter_results(query_terms: &[String], results: &[SearchResultView]) -> Vec<SearchResultView> {
    results
        .iter()
        .cloned()
        .map(|mut result| {
            result.score = score_result(query_terms, &result);
            result
        })
        .filter(|item| {
            let haystack = format!(
                "{} {} {} {}",
                item.file_name, item.path, item.heading, item.snippet
            )
            .to_lowercase();
            query_terms.iter().all(|term| haystack.contains(term))
        })
        .collect()
}

pub fn relax_results(query_terms: &[String], results: &[SearchResultView]) -> Vec<SearchResultView> {
    results
        .iter()
        .cloned()
        .map(|mut result| {
            result.score = score_result(query_terms, &result);
            result
        })
        .filter(|item| {
            let haystack = format!(
                "{} {} {} {}",
                item.file_name, item.path, item.heading, item.snippet
            )
            .to_lowercase();
            query_terms.iter().any(|term| haystack.contains(term))
        })
        .collect()
}
