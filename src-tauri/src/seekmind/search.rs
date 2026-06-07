#![allow(dead_code)]

pub fn normalize_query(query: &str) -> Vec<String> {
    tokenize_search_text(query)
}

pub fn normalize_search_text(input: &str) -> String {
    tokenize_search_text(input).join(" ")
}

pub fn rewrite_query_terms(input: &str) -> Vec<String> {
    let mut terms = tokenize_search_text(input);
    let lower = input.to_lowercase();
    let mut expanded = Vec::new();

    if lower.contains("离线仓库")
        || lower.contains("offline")
        || lower.contains("repo")
        || lower.contains("repository")
    {
        expanded.push("offline".to_string());
        expanded.push("repo".to_string());
        expanded.push("repository".to_string());
        expanded.push("local repository".to_string());
    }
    if lower.contains("语义搜索")
        || lower.contains("semantic")
        || lower.contains("embedding")
        || lower.contains("向量")
    {
        expanded.push("semantic search".to_string());
        expanded.push("embedding".to_string());
        expanded.push("vector".to_string());
    }
    if lower.contains("切片") || lower.contains("chunk") || lower.contains("chunks") {
        expanded.push("chunk".to_string());
        expanded.push("paragraph".to_string());
        expanded.push("snippet".to_string());
    }
    if lower.contains("markdown")
        || lower == "md"
        || lower.contains(" md ")
        || lower.starts_with("md ")
        || lower.ends_with(" md")
    {
        expanded.push("markdown".to_string());
        expanded.push("md".to_string());
    }
    if lower.contains("docx") || lower.contains("word") {
        expanded.push("docx".to_string());
        expanded.push("word".to_string());
    }
    if lower.contains("html") {
        expanded.push("html".to_string());
    }

    terms.append(&mut expanded);
    dedupe_terms(terms)
}

pub fn rewrite_search_text(input: &str) -> String {
    rewrite_query_terms(input).join(" ")
}

fn tokenize_search_text(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut ascii = String::new();
    let mut chinese = String::new();

    let flush_ascii = |ascii: &mut String, tokens: &mut Vec<String>| {
        if !ascii.is_empty() {
            tokens.push(ascii.to_lowercase());
            ascii.clear();
        }
    };

    let flush_chinese = |chinese: &mut String, tokens: &mut Vec<String>| {
        if chinese.is_empty() {
            return;
        }

        let chars: Vec<char> = chinese.chars().collect();
        if chars.len() == 1 {
            tokens.push(chinese.clone());
        } else {
            for window in chars.windows(2) {
                tokens.push(window.iter().collect());
            }
        }

        chinese.clear();
    };

    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            flush_chinese(&mut chinese, &mut tokens);
            ascii.push(ch);
            continue;
        }

        if is_han_character(ch) {
            flush_ascii(&mut ascii, &mut tokens);
            chinese.push(ch);
            continue;
        }

        flush_ascii(&mut ascii, &mut tokens);
        flush_chinese(&mut chinese, &mut tokens);
    }

    flush_ascii(&mut ascii, &mut tokens);
    flush_chinese(&mut chinese, &mut tokens);

    dedupe_terms(tokens)
}

fn dedupe_terms(terms: Vec<String>) -> Vec<String> {
    let mut deduped = Vec::new();
    for term in terms {
        if term.trim().is_empty() {
            continue;
        }
        if !deduped.iter().any(|existing| existing == &term) {
            deduped.push(term);
        }
    }
    deduped
}

fn is_han_character(ch: char) -> bool {
    matches!(ch as u32,
        0x4E00..=0x9FFF |
        0x3400..=0x4DBF |
        0x20000..=0x2A6DF |
        0x2A700..=0x2B73F |
        0x2B740..=0x2B81F |
        0x2B820..=0x2CEAF |
        0xF900..=0xFAFF)
}
