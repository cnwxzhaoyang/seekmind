use std::fs;
use std::io::Read;
use std::path::Path;

use chrono::{DateTime, Utc};
use zip::ZipArchive;

use crate::docmind::parser::{python_parse_or_fallback, ParsedDocument};

use super::types::{ChunkRecord, DiscoveredFile, ExtractedDocument};

const SUPPORTED_EXTENSIONS: &[&str] = &[
    "txt", "md", "markdown", "html", "htm", "docx", "log", "toml", "json", "yaml", "yml", "xml",
    "csv", "rs", "js", "ts", "tsx", "jsx", "py",
];

const SKIPPED_DIRECTORIES: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "Library",
    "Caches",
    "Application Support",
];

pub fn discover_supported_files(dir_paths: &[String]) -> Vec<DiscoveredFile> {
    let mut discovered = Vec::new();

    for dir_path in dir_paths {
        let root = Path::new(dir_path);
        if !root.exists() || !root.is_dir() {
            continue;
        }

        walk_directory(root, dir_path, &mut discovered);
    }

    discovered
}

#[allow(dead_code)]
pub fn extract_document(file: &DiscoveredFile) -> Result<ExtractedDocument, String> {
    extract_document_at(&file.dir_path, &file.path)
}

pub fn parse_document(file: &DiscoveredFile) -> Result<(ExtractedDocument, Vec<ChunkRecord>), String> {
    if let Some(parsed) = python_parse_or_fallback(&file.path) {
        return Ok(convert_python_document(file, parsed));
    }

    let document = extract_document_at(&file.dir_path, &file.path)?;
    let chunks = chunk_document(&document);
    Ok((document, chunks))
}

pub fn extract_document_at(dir_path: &str, path: &Path) -> Result<ExtractedDocument, String> {
    let ext = extension(path);
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("unknown")
        .to_string();

    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    let modified = metadata
        .modified()
        .map(DateTime::<Utc>::from)
        .map(|value| value.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|_| "未知".to_string());

    let content = match ext.as_str() {
        "txt" | "md" | "markdown" | "log" | "toml" | "json" | "yaml" | "yml" | "xml"
        | "csv" | "rs" | "js" | "ts" | "tsx" | "jsx" | "py" => read_text_file(path)?,
        "html" | "htm" => strip_html_tags(&read_text_file(path)?),
        "docx" => extract_docx_text(path)?,
        "pdf" => {
            return Err("暂不支持 PDF 解析，请后续接入 PDF 文本提取或 OCR".to_string());
        }
        _ => return Err(format!("不支持的文件类型: {ext}")),
    };

    Ok(ExtractedDocument {
        dir_path: dir_path.to_string(),
        path: path.to_string_lossy().to_string(),
        file_name,
        ext,
        modified,
        content: normalize_whitespace(&content),
    })
}

pub fn chunk_document(document: &ExtractedDocument) -> Vec<ChunkRecord> {
    let mut chunks = Vec::new();
    let paragraphs = split_paragraphs(&document.content);

    for (index, paragraph) in paragraphs.into_iter().enumerate() {
        let paragraph = paragraph.trim();
        if paragraph.is_empty() {
            continue;
        }

        let heading = derive_heading(paragraph, &document.file_name, index == 0);
        let snippet = truncate_snippet(&normalize_whitespace(paragraph), 240);

        chunks.push(ChunkRecord {
            heading,
            snippet,
            paragraph: Some((index + 1) as i64),
            page: None,
            score: 1.0,
        });
    }

    if chunks.is_empty() && !document.content.trim().is_empty() {
        chunks.push(ChunkRecord {
            heading: document.file_name.clone(),
            snippet: truncate_snippet(&normalize_whitespace(&document.content), 240),
            paragraph: Some(1),
            page: None,
            score: 1.0,
        });
    }

    chunks
}

fn walk_directory(root: &Path, dir_path: &str, discovered: &mut Vec<DiscoveredFile>) {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();

        if path.is_dir() {
            if should_skip_directory(&name) {
                continue;
            }
            walk_directory(&path, dir_path, discovered);
            continue;
        }

        if path.is_file() && is_supported_file(&path) {
            discovered.push(DiscoveredFile {
                dir_path: dir_path.to_string(),
                path,
            });
        }
    }
}

fn should_skip_directory(name: &str) -> bool {
    SKIPPED_DIRECTORIES
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(name))
        || name.starts_with('.')
}

fn is_supported_file(path: &Path) -> bool {
    let ext = extension(path);
    SUPPORTED_EXTENSIONS.iter().any(|candidate| *candidate == ext)
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_lowercase()
}

fn read_text_file(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| error.to_string())
}

fn extract_docx_text(path: &Path) -> Result<String, String> {
    let file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|error| error.to_string())?;
    let mut document_xml = String::new();

    archive
        .by_name("word/document.xml")
        .map_err(|error| error.to_string())?
        .read_to_string(&mut document_xml)
        .map_err(|error| error.to_string())?;

    Ok(extract_xml_text_nodes(&document_xml))
}

fn extract_xml_text_nodes(xml: &str) -> String {
    let mut text = String::new();
    let mut cursor = xml;

    while let Some(start) = cursor.find("<w:t") {
        cursor = &cursor[start + 4..];
        if let Some(tag_end) = cursor.find('>') {
            cursor = &cursor[tag_end + 1..];
            if let Some(end) = cursor.find("</w:t>") {
                text.push_str(&decode_entities(&cursor[..end]));
                text.push('\n');
                cursor = &cursor[end + 6..];
            } else {
                break;
            }
        } else {
            break;
        }
    }

    text
}

fn strip_html_tags(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut inside_tag = false;

    for character in input.chars() {
        match character {
            '<' => inside_tag = true,
            '>' => inside_tag = false,
            _ if !inside_tag => result.push(character),
            _ => {}
        }
    }

    decode_entities(&result)
}

fn decode_entities(input: &str) -> String {
    input
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

fn normalize_whitespace(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut last_was_space = false;

    for character in input.chars() {
        if character.is_whitespace() {
            if !last_was_space {
                output.push(' ');
            }
            last_was_space = true;
        } else {
            output.push(character);
            last_was_space = false;
        }
    }

    output.trim().to_string()
}

fn split_paragraphs(content: &str) -> Vec<String> {
    let normalized = content.replace("\r\n", "\n");
    let mut paragraphs = Vec::new();
    let mut current = Vec::new();

    for line in normalized.lines() {
        if line.trim().is_empty() {
            if !current.is_empty() {
                paragraphs.push(current.join("\n"));
                current.clear();
            }
        } else {
            current.push(line.to_string());
        }
    }

    if !current.is_empty() {
        paragraphs.push(current.join("\n"));
    }

    if paragraphs.is_empty() && !normalized.trim().is_empty() {
        paragraphs.push(normalized);
    }

    paragraphs
}

fn derive_heading(paragraph: &str, file_name: &str, is_first: bool) -> String {
    let first_line = paragraph.lines().next().unwrap_or("").trim();
    if let Some(stripped) = first_line.strip_prefix('#') {
        let heading = stripped.trim_start_matches('#').trim();
        if !heading.is_empty() {
            return heading.to_string();
        }
    }

    if is_first {
        return file_name.to_string();
    }

    file_name.to_string()
}

fn truncate_snippet(input: &str, limit: usize) -> String {
    let mut snippet = String::new();
    for character in input.chars() {
        if snippet.chars().count() >= limit {
            break;
        }
        snippet.push(character);
    }
    if input.chars().count() > limit {
        snippet.push('…');
    }
    snippet
}

fn convert_python_document(file: &DiscoveredFile, parsed: ParsedDocument) -> (ExtractedDocument, Vec<ChunkRecord>) {
    let content = normalize_whitespace(&parsed.content);
    let file_name = file
        .path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("unknown")
        .to_string();
    let ext = extension(&file.path);
    let modified = fs::metadata(&file.path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .map(DateTime::<Utc>::from)
        .map(|value| value.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "未知".to_string());

    let document = ExtractedDocument {
        dir_path: file.dir_path.clone(),
        path: file.path.to_string_lossy().to_string(),
        file_name: file_name.clone(),
        ext,
        modified,
        content,
    };

    let chunks = parsed
        .chunks
        .into_iter()
        .map(|chunk| ChunkRecord {
            heading: chunk.heading.unwrap_or_else(|| file_name.clone()),
            snippet: truncate_snippet(&normalize_whitespace(&chunk.text), 800),
            paragraph: Some(chunk.order as i64),
            page: chunk.page_no.map(|value| value as i64),
            score: 1.0,
        })
        .collect::<Vec<_>>();

    let chunks = if chunks.is_empty() && !document.content.trim().is_empty() {
        vec![ChunkRecord {
            heading: file_name,
            snippet: truncate_snippet(&document.content, 800),
            paragraph: Some(1),
            page: None,
            score: 1.0,
        }]
    } else {
        chunks
    };

    (document, chunks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_and_extracts_markdown_files_from_agent_directory() {
        let dir_path = "/Users/zhaoyang/Documents/MarkdownHome/zhaoyang-markdown/AI/面向agent编程";
        let files = discover_supported_files(&[dir_path.to_string()]);

        assert_eq!(files.len(), 4);
        assert!(files
            .iter()
            .all(|file| file.path.extension().and_then(|value| value.to_str()) == Some("md")));

        let first = &files[0];
        let document = extract_document(first).expect("expected markdown file to be extractable");
        assert_eq!(document.dir_path, dir_path);
        assert!(!document.content.trim().is_empty());

        let chunks = chunk_document(&document);
        assert!(!chunks.is_empty());
    }
}
