use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use zip::ZipArchive;

use crate::docmind::parser::types::ParserStreamEvent;
use crate::docmind::parser::types::ParsedBlock;
use crate::docmind::parser::{ParsedDocument, ParserClientError, PythonParserClient};

use super::types::{
    ChunkRecord, DiscoveredFile, ExtractedDocument, IndexSettings, ParseOutcome, ParserSource,
};

const SUPPORTED_EXTENSIONS: &[&str] = &[
    "txt", "md", "markdown", "html", "htm", "doc", "docx", "pdf", "log", "toml", "json", "yaml",
    "yml", "xml", "csv", "rs", "js", "ts", "tsx", "jsx", "py", "ppt", "pptx",
];

const SKIPPED_DIRECTORIES: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "Library",
    "Caches",
    "Application Support",
];

#[allow(dead_code)]
pub fn discover_supported_files(dir_paths: &[String]) -> Vec<DiscoveredFile> {
    discover_supported_files_with_settings(dir_paths, &default_index_settings())
}

pub fn discover_supported_files_with_settings(
    dir_paths: &[String],
    settings: &IndexSettings,
) -> Vec<DiscoveredFile> {
    let mut discovered = Vec::new();

    for dir_path in dir_paths {
        let root = Path::new(dir_path);
        if !root.exists() || !root.is_dir() {
            continue;
        }

        walk_directory(root, dir_path, settings, &mut discovered);
    }

    discovered
}

pub fn snapshot_supported_file(
    dir_path: &str,
    path: &Path,
    settings: &IndexSettings,
) -> Result<DiscoveredFile, String> {
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    let file_size = metadata.len() as i64;
    let max_file_size_bytes = (settings.max_file_size_mb.saturating_mul(1024 * 1024)) as i64;
    if max_file_size_bytes > 0 && file_size > max_file_size_bytes {
        return Err(format!(
            "文件过大: {} ({} bytes > {} MB)",
            path.to_string_lossy(),
            file_size,
            settings.max_file_size_mb
        ));
    }
    let modified_at = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default();

    Ok(DiscoveredFile {
        dir_path: dir_path.to_string(),
        path: path.to_path_buf(),
        file_size,
        modified_at,
        content_hash: hash_file_bytes(path)?,
    })
}

pub fn is_supported_document_path(path: &Path, settings: &IndexSettings) -> bool {
    path.is_file() && is_supported_file(path, settings)
}

#[allow(dead_code)]
pub fn extract_document(file: &DiscoveredFile) -> Result<ExtractedDocument, String> {
    extract_document_at(&file.dir_path, &file.path)
}

pub fn parse_document(
    file: &DiscoveredFile,
) -> Result<(ExtractedDocument, Vec<ChunkRecord>, Vec<ParsedBlock>, ParseOutcome), String> {
    parse_document_with_progress(file, |_| {})
}

pub fn parse_document_with_progress<F>(
    file: &DiscoveredFile,
    mut on_event: F,
) -> Result<(ExtractedDocument, Vec<ChunkRecord>, Vec<ParsedBlock>, ParseOutcome), String>
where
    F: FnMut(ParserStreamEvent),
{
    if crate::docmind::parser::python_parser_enabled() {
        let client = PythonParserClient::from_env();
        if client.is_configured() {
            match client.parse_document_stream(&file.path, |event| {
                on_event(event);
            }) {
                Ok(parsed) => {
                    let (document, chunks, blocks) = convert_python_document(file, parsed);
                    return Ok((
                        document,
                        chunks,
                        blocks,
                        ParseOutcome {
                            source: ParserSource::Python,
                            warning: None,
                        },
                    ));
                }
                Err(error) => {
                    let warning = match error {
                        ParserClientError::ParserFailed(parser_error) => format!(
                            "Python 解析失败：{} ({})",
                            parser_error.message, parser_error.code
                        ),
                        other => format!("Python 解析失败：{other}"),
                    };
                    if extension(&file.path) == "pdf" {
                        return Err(warning);
                    }

                    let fallback_warning = warning.replace("Python 解析失败：", "Python 解析失败，已回退 Rust：");
                    let document = extract_document_at(&file.dir_path, &file.path)?;
                    let chunks = chunk_document(&document);
                    return Ok((
                        document,
                        chunks,
                        Vec::new(),
                        ParseOutcome {
                            source: ParserSource::Rust,
                            warning: Some(fallback_warning),
                        },
                    ));
                }
            }
        }
    }

    let document = extract_document_at(&file.dir_path, &file.path)?;
    let chunks = chunk_document(&document);
    Ok((
        document,
        chunks,
        Vec::new(),
        ParseOutcome {
            source: ParserSource::Rust,
            warning: None,
        },
    ))
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
        "txt" | "md" | "markdown" | "log" | "toml" | "json" | "yaml" | "yml" | "xml" | "csv"
        | "rs" | "js" | "ts" | "tsx" | "jsx" | "py" => read_text_file(path)?,
        "html" | "htm" => strip_html_tags(&read_text_file(path)?),
        "doc" | "ppt" | "pptx" => extract_office_text(path)?,
        "docx" => extract_docx_text(path)?,
        "pdf" => {
            return Err("暂不支持 PDF 解析，请启用 Python 解析器或接入 PDF 文本提取".to_string());
        }
        _ => return Err(format!("不支持的文件类型: {ext}")),
    };

    Ok(ExtractedDocument {
        dir_path: dir_path.to_string(),
        path: path.to_string_lossy().to_string(),
        file_name,
        ext,
        file_size: metadata.len() as i64,
        modified_at: metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or_default(),
        content_hash: hash_file_bytes(path)?,
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
            block_indexes: Vec::new(),
        });
    }

    if chunks.is_empty() && !document.content.trim().is_empty() {
        chunks.push(ChunkRecord {
            heading: document.file_name.clone(),
            snippet: truncate_snippet(&normalize_whitespace(&document.content), 240),
            paragraph: Some(1),
            page: None,
            score: 1.0,
            block_indexes: Vec::new(),
        });
    }

    chunks
}

fn walk_directory(
    root: &Path,
    dir_path: &str,
    settings: &IndexSettings,
    discovered: &mut Vec<DiscoveredFile>,
) {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();

        if path.is_dir() {
            if should_skip_directory(&name, settings) {
                continue;
            }
            walk_directory(&path, dir_path, settings, discovered);
            continue;
        }

        if path.is_file() && is_supported_file(&path, settings) {
            if let Ok(file) = snapshot_supported_file(dir_path, &path, settings) {
                discovered.push(file);
            }
        }
    }
}

fn should_skip_directory(name: &str, settings: &IndexSettings) -> bool {
    SKIPPED_DIRECTORIES
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(name))
        || settings
            .exclude_dirs
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(name))
        || name.starts_with('.')
}

fn is_supported_file(path: &Path, settings: &IndexSettings) -> bool {
    let ext = extension(path);
    SUPPORTED_EXTENSIONS
        .iter()
        .any(|candidate| *candidate == ext)
        && !settings
            .exclude_exts
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(&ext))
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

fn hash_file_bytes(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

#[allow(dead_code)]
fn default_index_settings() -> IndexSettings {
    IndexSettings {
        exclude_dirs: vec![
            "node_modules".to_string(),
            ".git".to_string(),
            "target".to_string(),
            "Library".to_string(),
            "Caches".to_string(),
            "Application Support".to_string(),
        ],
        exclude_exts: Vec::new(),
        max_file_size_mb: 50,
        semantic_search_enabled: true,
        semantic_weight: 0.25,
        semantic_threshold: 0.2,
        title_weight: 1.0,
        filename_weight: 1.0,
        preference_weight: 1.0,
        prefer_favorites_enabled: true,
        prefer_recent_enabled: true,
        prefer_history_enabled: true,
    }
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

fn extract_doc_text_with_textutil(path: &Path) -> Result<String, String> {
    let output = Command::new("/usr/bin/textutil")
        .arg("-stdout")
        .arg("-convert")
        .arg("txt")
        .arg(path)
        .output()
        .map_err(|error| error.to_string())?;

    if !output.status.success() {
        return Err(format!(
            "textutil failed for {}: {}",
            path.to_string_lossy(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let text = String::from_utf8(output.stdout).map_err(|error| error.to_string())?;
    let normalized = normalize_whitespace(&text);
    let stderr = normalize_whitespace(&String::from_utf8_lossy(&output.stderr));
    if looks_like_textutil_error_output(&stderr) {
        return Err(format!("textutil returned an error for {}: {}", path.to_string_lossy(), stderr));
    }
    if looks_like_textutil_error_output(&normalized) {
        return Err(format!(
            "textutil returned an error for {}: {}",
            path.to_string_lossy(),
            normalized
        ));
    }
    if normalized.is_empty() {
        return Err(format!(
            "textutil produced empty document text for {}{}",
            path.to_string_lossy(),
            if stderr.is_empty() {
                String::new()
            } else {
                format!(": {stderr}")
            }
        ));
    }

    Ok(normalized)
}

fn looks_like_textutil_error_output(text: &str) -> bool {
    let lowered = text.to_lowercase();
    lowered.starts_with("error reading ")
        || lowered.contains("the file isn’t in the correct format")
        || lowered.contains("the file isn't in the correct format")
}

fn office_converter_path() -> Option<String> {
    let mut candidates = vec![std::env::var("DOCMIND_OFFICE_BIN").ok()];
    if cfg!(target_os = "windows") {
        candidates.extend([
            Some("soffice.exe".to_string()),
            Some("libreoffice.exe".to_string()),
            std::env::var("PROGRAMFILES")
                .ok()
                .map(|value| format!("{value}\\LibreOffice\\program\\soffice.exe")),
            std::env::var("PROGRAMFILES(X86)")
                .ok()
                .map(|value| format!("{value}\\LibreOffice\\program\\soffice.exe")),
        ]);
    } else if cfg!(target_os = "macos") {
        candidates.extend([
            Some("soffice".to_string()),
            Some("libreoffice".to_string()),
            Some("/Applications/LibreOffice.app/Contents/MacOS/soffice".to_string()),
            Some("/Applications/LibreOffice.app/Contents/MacOS/libreoffice".to_string()),
        ]);
    } else {
        candidates.extend([Some("soffice".to_string()), Some("libreoffice".to_string())]);
    }

    candidates
        .into_iter()
        .flatten()
        .find(|candidate| office_converter_works(candidate))
}

fn office_converter_works(candidate: &str) -> bool {
    if candidate.trim().is_empty() {
        return false;
    }

    Command::new(candidate)
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn office_conversion_output_dir(path: &Path) -> PathBuf {
    let digest = Sha256::digest(path.to_string_lossy().as_bytes());
    let key = digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let dir = std::env::temp_dir().join("docmind-office-convert").join(key);
    let _ = fs::create_dir_all(&dir);
    dir
}

fn convert_office_document(path: &Path, target_ext: &str) -> Option<PathBuf> {
    let converter = office_converter_path()?;
    let output_dir = office_conversion_output_dir(path);
    let stem = path.file_stem()?.to_string_lossy().to_string();
    let target_ext = target_ext.trim_start_matches('.').to_lowercase();
    let expected_output = output_dir.join(format!("{stem}.{target_ext}"));

    if expected_output.exists() && expected_output.metadata().ok()?.len() > 0 {
        return Some(expected_output);
    }

    let status = Command::new(converter)
        .arg("--headless")
        .arg("--nologo")
        .arg("--nofirststartwizard")
        .arg("--convert-to")
        .arg(&target_ext)
        .arg("--outdir")
        .arg(&output_dir)
        .arg(path)
        .status()
        .ok()?;

    if !status.success() {
        return None;
    }

    if expected_output.exists() && expected_output.metadata().ok()?.len() > 0 {
        return Some(expected_output);
    }

    fs::read_dir(&output_dir)
        .ok()?
        .filter_map(|entry| entry.ok().map(|item| item.path()))
        .find(|candidate| candidate.extension().and_then(|value| value.to_str()) == Some(target_ext.as_str()))
}

fn extract_office_text(path: &Path) -> Result<String, String> {
    if let Some(converted_txt) = convert_office_document(path, "txt") {
        let text = fs::read_to_string(&converted_txt).map_err(|error| error.to_string())?;
        if !text.trim().is_empty() {
            return Ok(normalize_whitespace(&text));
        }
    }

    if let Some(converted_html) = convert_office_document(path, "html") {
        let text = fs::read_to_string(&converted_html).map_err(|error| error.to_string())?;
        if !text.trim().is_empty() {
            return Ok(normalize_whitespace(&strip_html_tags(&text)));
        }
    }

    #[cfg(target_os = "macos")]
    {
        return extract_doc_text_with_textutil(path);
    }

    #[cfg(not(target_os = "macos"))]
    {
        Err(format!(
            "failed to extract office text for {}",
            path.to_string_lossy()
        ))
    }
}

fn extract_xml_text_nodes(xml: &str) -> String {
    let mut text = String::new();
    let mut cursor = xml;

    while let Some(start) = cursor.find("<w:t") {
        cursor = &cursor[start + 4..];
        if let Some(tag_end) = cursor.find('>') {
            cursor = &cursor[tag_end + 1..];
            if let Some(end) = cursor.find("</w:t>") {
                let piece = clean_docx_text(&decode_entities(&cursor[..end]));
                if !piece.is_empty() {
                    text.push_str(&piece);
                    text.push('\n');
                }
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

fn clean_docx_text(input: &str) -> String {
    let cleaned = normalize_whitespace(&decode_entities(input));
    if cleaned.is_empty() || looks_like_docx_xml_noise(&cleaned) {
        return String::new();
    }
    cleaned
}

fn looks_like_docx_xml_noise(text: &str) -> bool {
    let lowered = text.to_lowercase();
    if lowered.contains("<w:") || lowered.contains("</w:") || lowered.contains("xmlns:") {
        return true;
    }
    if lowered.starts_with('<')
        && lowered.contains('>')
        && (lowered.contains("w:") || lowered.contains("xml"))
    {
        return true;
    }
    false
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

pub(crate) fn convert_python_document(
    file: &DiscoveredFile,
    parsed: ParsedDocument,
) -> (ExtractedDocument, Vec<ChunkRecord>, Vec<ParsedBlock>) {
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
        file_size: file.file_size,
        modified_at: file.modified_at,
        content_hash: file.content_hash.clone(),
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
            score: chunk.score.clamp(0.25, 1.0),
            block_indexes: chunk.block_indexes.unwrap_or_default(),
        })
        .collect::<Vec<_>>();

    let chunks = if chunks.is_empty() && !document.content.trim().is_empty() {
        vec![ChunkRecord {
            heading: file_name,
            snippet: truncate_snippet(&document.content, 800),
            paragraph: Some(1),
            page: None,
            score: 1.0,
            block_indexes: Vec::new(),
        }]
    } else {
        chunks
    };

    let blocks = match parsed.blocks {
        Some(blocks) => blocks,
        None => Vec::new(),
    };

    (document, chunks, blocks)
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
