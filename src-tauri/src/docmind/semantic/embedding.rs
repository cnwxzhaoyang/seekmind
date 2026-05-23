#![allow(dead_code)]

use sha2::{Digest, Sha256};

pub const DEFAULT_EMBEDDING_DIMENSION: usize = 384;

pub fn normalize_embedding_text(text: &str) -> String {
    let mut normalized = String::new();
    let mut last_was_space = false;

    for ch in text.chars() {
        if ch.is_whitespace() {
            if !last_was_space {
                normalized.push(' ');
                last_was_space = true;
            }
        } else {
            normalized.push(ch.to_ascii_lowercase());
            last_was_space = false;
        }
    }

    normalized.trim().to_string()
}

pub fn text_hash(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    hex_lower(&hasher.finalize())
}

pub fn embed_text(text: &str, dimension: usize) -> Vec<f32> {
    if dimension == 0 {
        return Vec::new();
    }

    let mut vector = vec![0.0_f32; dimension];
    let tokens = tokenize(text);
    if tokens.is_empty() {
        return vector;
    }

    let weight = 1.0_f32 / (tokens.len() as f32).sqrt().max(1.0);
    for token in tokens {
        let digest = Sha256::digest(token.as_bytes());
        let index = u64::from_le_bytes([
            digest[0], digest[1], digest[2], digest[3], digest[4], digest[5], digest[6], digest[7],
        ]) as usize
            % dimension;
        let sign = if digest[8] & 1 == 0 { 1.0 } else { -1.0 };
        vector[index] += sign * weight;
    }

    normalize_vector(&mut vector);
    vector
}

pub fn embed_texts(texts: &[String], dimension: usize) -> Vec<Vec<f32>> {
    texts
        .iter()
        .map(|text| embed_text(text, dimension))
        .collect()
}

pub fn cosine_similarity(left: &[f32], right: &[f32]) -> f32 {
    if left.is_empty() || right.is_empty() || left.len() != right.len() {
        return 0.0;
    }

    let mut dot = 0.0_f32;
    let mut left_norm = 0.0_f32;
    let mut right_norm = 0.0_f32;
    for (left_value, right_value) in left.iter().zip(right.iter()) {
        dot += left_value * right_value;
        left_norm += left_value * left_value;
        right_norm += right_value * right_value;
    }

    if left_norm == 0.0 || right_norm == 0.0 {
        return 0.0;
    }

    dot / (left_norm.sqrt() * right_norm.sqrt())
}

pub fn vector_norm(vector: &[f32]) -> f32 {
    vector.iter().map(|value| value * value).sum::<f32>().sqrt()
}

fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut cjk_chars = Vec::new();

    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            current.push(ch.to_ascii_lowercase());
            continue;
        }

        if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }

        if is_cjk(ch) {
            cjk_chars.push(ch);
            tokens.push(ch.to_string());
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    for window in cjk_chars.windows(2) {
        let bigram = window.iter().collect::<String>();
        if !bigram.trim().is_empty() {
            tokens.push(bigram);
        }
    }

    tokens
}

fn is_cjk(ch: char) -> bool {
    matches!(
        ch as u32,
        0x4E00..=0x9FFF
            | 0x3400..=0x4DBF
            | 0x20000..=0x2A6DF
            | 0x2A700..=0x2B73F
            | 0x2B740..=0x2B81F
            | 0x2B820..=0x2CEAF
            | 0xF900..=0xFAFF
    )
}

fn normalize_vector(vector: &mut [f32]) {
    let norm = vector_norm(vector);
    if norm == 0.0 {
        return;
    }

    for value in vector {
        *value /= norm;
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{:02x}", byte);
    }
    output
}
