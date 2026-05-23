#![allow(dead_code)]

pub fn normalize_query(query: &str) -> Vec<String> {
    tokenize_search_text(query)
}

pub fn normalize_search_text(input: &str) -> String {
    tokenize_search_text(input).join(" ")
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

    tokens
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
