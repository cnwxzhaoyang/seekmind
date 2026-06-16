/**
 * @author MorningSun
 * @CreatedDate 2026/06/05
 * @Description SeekMind 文档发现、解析与 OCR 队列转换逻辑。
 */
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use zip::ZipArchive;

use crate::seekmind::parser::types::ParserStreamEvent;
use crate::seekmind::parser::types::{ParsedBlock, PdfOcrTask};
use crate::seekmind::parser::{ParsedDocument, ParserClientError, PythonParserClient};
use crate::seekmind::process_utils::configure_hidden_child_process;
#[cfg(target_os = "windows")]
use crate::seekmind::process_utils::run_hidden_powershell_script;
use crate::seekmind::runtime_paths::office_runtime;
use crate::seekmind::vision_ocr::recognize_image_text;

use super::types::{
    ChunkRecord, DiscoveredFile, ExtractedDocument, IndexSettings, ParseOutcome, ParserSource,
};

const SUPPORTED_EXTENSIONS: &[&str] = &[
    "txt", "md", "markdown", "html", "htm", "doc", "docx", "pdf", "log", "toml", "json", "yaml",
    "yml", "xml", "csv", "rs", "js", "ts", "tsx", "jsx", "py", "ppt", "pptx", "xlsx", "epub",
    "rtf", "png", "jpg", "jpeg", "webp", "bmp", "gif", "tif", "tiff", "heic",
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
) -> Result<
    (
        ExtractedDocument,
        Vec<ChunkRecord>,
        Vec<ParsedBlock>,
        Vec<PdfOcrTask>,
        ParseOutcome,
    ),
    String,
> {
    parse_document_with_progress(file, |_| {})
}

pub fn parse_document_with_progress<F>(
    file: &DiscoveredFile,
    mut on_event: F,
) -> Result<
    (
        ExtractedDocument,
        Vec<ChunkRecord>,
        Vec<ParsedBlock>,
        Vec<PdfOcrTask>,
        ParseOutcome,
    ),
    String,
>
where
    F: FnMut(ParserStreamEvent),
{
    if crate::seekmind::parser::python_parser_enabled() {
        let client = PythonParserClient::from_env();
        if client.is_configured() {
            match client.parse_document_stream(&file.path, |event| {
                on_event(event);
            }) {
                Ok(parsed) => {
                    let (document, chunks, blocks, ocr_tasks) =
                        convert_python_document(file, parsed);
                    return Ok((
                        document,
                        chunks,
                        blocks,
                        ocr_tasks,
                        ParseOutcome {
                            source: ParserSource::Python,
                            warning: None,
                        },
                    ));
                }
                Err(error) => {
                    let warning = match error {
                        ParserClientError::ParserFailed(parser_error) => {
                            // 修复：图片等文件的回退提示只保留“默认/备用”链路语义，避免把内部实现名暴露给用户。
                            if parser_error.code == "unsupported_file_type" {
                                "默认解析器不支持当前文件类型，已切换到备用解析流程".to_string()
                            } else {
                                format!(
                                    "默认解析器处理失败，已切换到备用解析流程：{} ({})",
                                    parser_error.message, parser_error.code
                                )
                            }
                        }
                        other => format!("默认解析器处理失败，已切换到备用解析流程：{other}"),
                    };
                    if extension(&file.path) == "pdf" {
                        return Err(warning);
                    }

                    let fallback_warning = warning.clone();
                    let document = extract_document_at(&file.dir_path, &file.path)?;
                    let chunks = chunk_document(&document);
                    return Ok((
                        document,
                        chunks,
                        Vec::new(),
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
        "doc" | "ppt" | "rtf" => extract_office_text(path)?,
        "docx" => extract_docx_text(path)?,
        "pptx" => extract_pptx_text(path)?,
        "xlsx" => extract_xlsx_text(path)?,
        "epub" => extract_epub_text(path)?,
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "gif" | "tif" | "tiff" | "heic" => {
            extract_image_text(path)?
        }
        "pdf" => {
            return Err("暂不支持 PDF 解析，请启用默认解析链路或接入 PDF 文本提取".to_string());
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
        && (cfg!(target_os = "macos") || !is_image_extension(&ext))
        && !settings
            .exclude_exts
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(&ext))
}

fn is_image_extension(ext: &str) -> bool {
    matches!(
        ext,
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "gif" | "tif" | "tiff" | "heic"
    )
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

fn read_zip_file_to_string<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    entry_name: &str,
) -> Result<String, String> {
    let mut entry = archive
        .by_name(entry_name)
        .map_err(|error| error.to_string())?;
    let mut content = String::new();
    entry
        .read_to_string(&mut content)
        .map_err(|error| error.to_string())?;
    Ok(content)
}

fn extract_xml_attribute(xml: &str, attribute: &str) -> Option<String> {
    for pattern in [format!("{attribute}=\""), format!("{attribute}='")] {
        if let Some(start) = xml.find(&pattern) {
            let remainder = &xml[start + pattern.len()..];
            let end = remainder.find(['"', '\'']).unwrap_or(remainder.len());
            let value = remainder[..end].trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn extract_epub_manifest(opf_xml: &str, opf_path: &str) -> HashMap<String, String> {
    let mut manifest = HashMap::new();
    let base_dir = opf_path.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
    let mut cursor = opf_xml;

    while let Some(start) = cursor.find("<item") {
        cursor = &cursor[start + 5..];
        let Some(tag_end) = cursor.find('>') else {
            break;
        };
        let tag = &cursor[..tag_end];
        let id = extract_xml_attribute(tag, "id");
        let href = extract_xml_attribute(tag, "href");
        if let (Some(id), Some(href)) = (id, href) {
            let resolved = resolve_relative_zip_path(base_dir, &href);
            if !resolved.is_empty() {
                manifest.insert(id, resolved);
            }
        }
        cursor = &cursor[tag_end + 1..];
    }

    manifest
}

fn extract_epub_spine_paths(
    opf_xml: &str,
    opf_path: &str,
    manifest: &HashMap<String, String>,
) -> Vec<String> {
    let mut paths = Vec::new();
    let base_dir = opf_path.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
    let mut cursor = opf_xml;

    while let Some(start) = cursor.find("<itemref") {
        cursor = &cursor[start + 8..];
        let Some(tag_end) = cursor.find('>') else {
            break;
        };
        let tag = &cursor[..tag_end];
        if let Some(idref) = extract_xml_attribute(tag, "idref") {
            if let Some(href) = manifest.get(&idref) {
                paths.push(href.clone());
            } else if !base_dir.is_empty() {
                let candidate = resolve_relative_zip_path(base_dir, &idref);
                if !candidate.is_empty() {
                    paths.push(candidate);
                }
            }
        }
        cursor = &cursor[tag_end + 1..];
    }

    paths
}

fn resolve_relative_zip_path(base_dir: &str, href: &str) -> String {
    if href.contains("://") || href.starts_with('/') {
        return href.to_string();
    }

    let mut parts = if base_dir.is_empty() {
        Vec::new()
    } else {
        base_dir
            .split('/')
            .filter(|segment| !segment.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>()
    };

    for segment in href.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                let _ = parts.pop();
            }
            other => parts.push(other.to_string()),
        }
    }

    parts.join("/")
}

fn is_epub_text_candidate(path: &str) -> bool {
    let lowered = path.to_lowercase();
    lowered.ends_with(".xhtml")
        || lowered.ends_with(".html")
        || lowered.ends_with(".htm")
        || lowered.ends_with(".xml")
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

fn extract_pptx_text(path: &Path) -> Result<String, String> {
    let file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|error| error.to_string())?;
    let mut slide_names = archive
        .file_names()
        .map(|name| name.to_string())
        .filter(|name| name.starts_with("ppt/slides/slide") && name.ends_with(".xml"))
        .collect::<Vec<_>>();
    slide_names.sort();

    let mut slides = Vec::new();
    for slide_name in slide_names {
        if let Ok(slide_xml) = read_zip_file_to_string(&mut archive, &slide_name) {
            let text = extract_xml_text_nodes_by_tag(&slide_xml, "a:t");
            if !text.trim().is_empty() {
                slides.push(text);
            }
        }
    }

    let normalized = normalize_whitespace(&slides.join("\n\n"));
    if normalized.is_empty() {
        #[cfg(target_os = "windows")]
        if office_runtime().kind == "windows-office-com" {
            return extract_windows_powerpoint_text(path);
        }

        if let Some(converted_txt) = convert_office_document(path, "txt") {
            let text = fs::read_to_string(&converted_txt).map_err(|error| error.to_string())?;
            if !text.trim().is_empty() {
                return Ok(normalize_whitespace(&text));
            }
        }

        Err(format!(
            "PPTX produced empty document text for {}",
            path.to_string_lossy()
        ))
    } else {
        Ok(normalized)
    }
}

fn extract_xlsx_text(path: &Path) -> Result<String, String> {
    let file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|error| error.to_string())?;
    let mut blocks = Vec::new();

    if let Ok(shared_strings_xml) = read_zip_file_to_string(&mut archive, "xl/sharedStrings.xml") {
        let shared_text = extract_xml_text_nodes_by_tag(&shared_strings_xml, "t");
        if !shared_text.trim().is_empty() {
            blocks.push(shared_text);
        }
    }

    let mut worksheet_names = archive
        .file_names()
        .map(|name| name.to_string())
        .filter(|name| name.starts_with("xl/worksheets/sheet") && name.ends_with(".xml"))
        .collect::<Vec<_>>();
    worksheet_names.sort();

    for worksheet_name in worksheet_names {
        if let Ok(worksheet_xml) = read_zip_file_to_string(&mut archive, &worksheet_name) {
            let worksheet_text = extract_xml_text_nodes_by_tag(&worksheet_xml, "t");
            if !worksheet_text.trim().is_empty() {
                blocks.push(worksheet_text);
            }
        }
    }

    let normalized = normalize_whitespace(&blocks.join("\n\n"));
    if normalized.is_empty() {
        #[cfg(target_os = "windows")]
        if office_runtime().kind == "windows-office-com" {
            return extract_windows_excel_text(path);
        }

        if let Some(converted_txt) = convert_office_document(path, "txt") {
            let text = fs::read_to_string(&converted_txt).map_err(|error| error.to_string())?;
            if !text.trim().is_empty() {
                return Ok(normalize_whitespace(&text));
            }
        }

        Err(format!(
            "XLSX produced empty document text for {}",
            path.to_string_lossy()
        ))
    } else {
        Ok(normalized)
    }
}

fn extract_epub_text(path: &Path) -> Result<String, String> {
    let file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|error| error.to_string())?;
    let container_xml = read_zip_file_to_string(&mut archive, "META-INF/container.xml")?;
    let rootfile = extract_xml_attribute(&container_xml, "full-path")
        .ok_or_else(|| format!("EPUB 缺少 rootfile: {}", path.to_string_lossy()))?;
    let opf_xml = read_zip_file_to_string(&mut archive, &rootfile)?;

    let manifest = extract_epub_manifest(&opf_xml, &rootfile);
    let spine_paths = extract_epub_spine_paths(&opf_xml, &rootfile, &manifest);
    let mut seen_paths = HashSet::new();
    let mut text_blocks = Vec::new();

    for content_path in spine_paths {
        if !seen_paths.insert(content_path.clone()) {
            continue;
        }

        if let Ok(content) = read_zip_file_to_string(&mut archive, &content_path) {
            let text = normalize_whitespace(&strip_html_tags(&content));
            if !text.trim().is_empty() {
                text_blocks.push(text);
            }
        }
    }

    // EPUB 本质上是章节化压缩包，若 spine 无法完全解析，则回退到按文件名顺序抽取 HTML/XHTML，保证内容仍可搜索。
    if text_blocks.is_empty() {
        let mut candidates = archive
            .file_names()
            .map(|name| name.to_string())
            .filter(|name| is_epub_text_candidate(name))
            .collect::<Vec<_>>();
        candidates.sort();

        for candidate in candidates {
            if !seen_paths.insert(candidate.clone()) {
                continue;
            }

            if let Ok(content) = read_zip_file_to_string(&mut archive, &candidate) {
                let text = normalize_whitespace(&strip_html_tags(&content));
                if !text.trim().is_empty() {
                    text_blocks.push(text);
                }
            }
        }
    }

    let normalized = normalize_whitespace(&text_blocks.join("\n\n"));
    if normalized.is_empty() {
        Err(format!(
            "EPUB produced empty document text for {}",
            path.to_string_lossy()
        ))
    } else {
        Ok(normalized)
    }
}

fn extract_image_text(path: &Path) -> Result<String, String> {
    let languages = crate::seekmind::vision_ocr::default_vision_ocr_languages();
    if languages.is_empty() {
        return Err("未检测到可用的 OCR 语言，图片无法识别".to_string());
    }

    let text = recognize_image_text(path, &languages)?;
    let normalized = normalize_whitespace(&text);
    if normalized.is_empty() {
        // 修复：截图类图片可能本身不含可识别文本，OCR 结果为空不应被当作解析失败，否则会把正常图片错误计入失败数。
        eprintln!(
            "[SeekMind] image ocr empty path={} languages={}",
            path.to_string_lossy(),
            languages.join(",")
        );
        Ok(String::new())
    } else {
        eprintln!(
            "[SeekMind] image ocr ok path={} chars={}",
            path.to_string_lossy(),
            normalized.chars().count()
        );
        Ok(normalized)
    }
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
        return Err(format!(
            "textutil returned an error for {}: {}",
            path.to_string_lossy(),
            stderr
        ));
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

fn office_conversion_output_dir(path: &Path) -> PathBuf {
    let digest = Sha256::digest(path.to_string_lossy().as_bytes());
    let key = digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let dir = std::env::temp_dir()
        .join("seekmind-office-convert")
        .join(key);
    let _ = fs::create_dir_all(&dir);
    dir
}

fn convert_office_document(path: &Path, target_ext: &str) -> Option<PathBuf> {
    let runtime = office_runtime();
    if runtime.kind != "libreoffice" {
        return None;
    }

    let converter = runtime.bin;
    let output_dir = office_conversion_output_dir(path);
    let stem = path.file_stem()?.to_string_lossy().to_string();
    let target_ext = target_ext.trim_start_matches('.').to_lowercase();
    let expected_output = output_dir.join(format!("{stem}.{target_ext}"));

    if expected_output.exists() && expected_output.metadata().ok()?.len() > 0 {
        return Some(expected_output);
    }

    let mut command = Command::new(&converter);
    configure_hidden_child_process(&mut command);
    let status = command
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
        .find(|candidate| {
            candidate.extension().and_then(|value| value.to_str()) == Some(target_ext.as_str())
        })
}

#[cfg(target_os = "windows")]
fn escape_powershell_literal(input: &str) -> String {
    input.replace('\'', "''")
}

#[cfg(target_os = "windows")]
fn run_powershell_office_script(script: &str) -> Result<String, String> {
    let (_, output) = run_hidden_powershell_script(script)?;

    if !output.status.success() {
        return Err(normalize_whitespace(&String::from_utf8_lossy(
            &output.stderr,
        )));
    }

    String::from_utf8(output.stdout).map_err(|error| error.to_string())
}

#[cfg(target_os = "windows")]
fn extract_windows_word_text(path: &Path) -> Result<String, String> {
    let escaped_path = escape_powershell_literal(&path.to_string_lossy());
    let script = format!(
        r#"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$source = '{escaped_path}'
$word = $null
$document = $null
try {{
  $word = New-Object -ComObject Word.Application
  $word.Visible = $false
  $word.DisplayAlerts = 0
  $document = $word.Documents.Open($source, $false, $true)
  Write-Output $document.Content.Text
}} finally {{
  if ($document -ne $null) {{ $document.Close($false) }}
  if ($word -ne $null) {{ $word.Quit() }}
}}
"#
    );

    let text = run_powershell_office_script(&script)?;
    let normalized = normalize_whitespace(&text);
    if normalized.is_empty() {
        Err(format!(
            "Word COM produced empty document text for {}",
            path.to_string_lossy()
        ))
    } else {
        Ok(normalized)
    }
}

#[cfg(target_os = "windows")]
fn extract_windows_powerpoint_text(path: &Path) -> Result<String, String> {
    let escaped_path = escape_powershell_literal(&path.to_string_lossy());
    let script = format!(
        r#"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$source = '{escaped_path}'
$powerpoint = $null
$presentation = $null
$lines = New-Object System.Collections.Generic.List[string]
try {{
  $powerpoint = New-Object -ComObject PowerPoint.Application
  $presentation = $powerpoint.Presentations.Open($source, $false, $true, $false)
  foreach ($slide in $presentation.Slides) {{
    foreach ($shape in $slide.Shapes) {{
      try {{
        if ($shape.HasTextFrame -and $shape.TextFrame.HasText) {{
          $text = $shape.TextFrame.TextRange.Text
          if ($text -and $text.Trim().Length -gt 0) {{
            [void]$lines.Add($text)
          }}
        }}
      }} catch {{}}
      try {{
        if ($shape.HasTable) {{
          for ($row = 1; $row -le $shape.Table.Rows.Count; $row++) {{
            for ($column = 1; $column -le $shape.Table.Columns.Count; $column++) {{
              $cellText = $shape.Table.Cell($row, $column).Shape.TextFrame.TextRange.Text
              if ($cellText -and $cellText.Trim().Length -gt 0) {{
                [void]$lines.Add($cellText)
              }}
            }}
          }}
        }}
      }} catch {{}}
    }}
  }}
  Write-Output ($lines -join [Environment]::NewLine)
}} finally {{
  if ($presentation -ne $null) {{ $presentation.Close() }}
  if ($powerpoint -ne $null) {{ $powerpoint.Quit() }}
}}
"#
    );

    let text = run_powershell_office_script(&script)?;
    let normalized = normalize_whitespace(&text);
    if normalized.is_empty() {
        Err(format!(
            "PowerPoint COM produced empty document text for {}",
            path.to_string_lossy()
        ))
    } else {
        Ok(normalized)
    }
}

#[cfg(target_os = "windows")]
fn extract_windows_excel_text(path: &Path) -> Result<String, String> {
    let escaped_path = escape_powershell_literal(&path.to_string_lossy());
    let script = format!(
        r#"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$source = '{escaped_path}'
$excel = $null
$workbook = $null
$lines = New-Object System.Collections.Generic.List[string]
try {{
  $excel = New-Object -ComObject Excel.Application
  $excel.Visible = $false
  $excel.DisplayAlerts = $false
  $workbook = $excel.Workbooks.Open($source, 0, $true)
  foreach ($sheet in $workbook.Worksheets) {{
    $range = $sheet.UsedRange
    if ($null -eq $range) {{
      continue
    }}
    foreach ($cell in $range.Cells) {{
      $text = $cell.Text
      if ($text -and $text.ToString().Trim().Length -gt 0) {{
        [void]$lines.Add($text.ToString())
      }}
    }}
  }}
  Write-Output ($lines -join [Environment]::NewLine)
}} finally {{
  if ($workbook -ne $null) {{ $workbook.Close($false) }}
  if ($excel -ne $null) {{ $excel.Quit() }}
}}
"#
    );

    let text = run_powershell_office_script(&script)?;
    let normalized = normalize_whitespace(&text);
    if normalized.is_empty() {
        Err(format!(
            "Excel COM produced empty document text for {}",
            path.to_string_lossy()
        ))
    } else {
        Ok(normalized)
    }
}

fn extract_office_text(path: &Path) -> Result<String, String> {
    let runtime = office_runtime();
    let ext = extension(path);

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

    #[cfg(target_os = "windows")]
    {
        if runtime.kind == "windows-office-com" {
            eprintln!(
                "[SeekMind] office fallback runtime=windows-office-com ext={} path={}",
                ext,
                path.to_string_lossy()
            );
            return match ext.as_str() {
                "doc" | "rtf" => extract_windows_word_text(path),
                "ppt" => extract_windows_powerpoint_text(path),
                "xlsx" => extract_windows_excel_text(path),
                other => Err(format!(
                    "Windows Office COM does not support {} for {}",
                    other,
                    path.to_string_lossy()
                )),
            };
        }
    }

    #[cfg(target_os = "macos")]
    {
        if runtime.kind == "macos-textutil" && matches!(ext.as_str(), "doc" | "rtf") {
            eprintln!(
                "[SeekMind] office fallback runtime=macos-textutil ext={} path={}",
                ext,
                path.to_string_lossy()
            );
            return extract_doc_text_with_textutil(path);
        }
    }

    Err(format!(
        "failed to extract office text for {}",
        path.to_string_lossy()
    ))
}

fn extract_xml_text_nodes(xml: &str) -> String {
    extract_xml_text_nodes_by_tag(xml, "w:t")
}

fn extract_xml_text_nodes_by_tag(xml: &str, tag_name: &str) -> String {
    let mut text = String::new();
    let mut cursor = xml;
    let open_pattern = format!("<{tag_name}");
    let close_pattern = format!("</{tag_name}>");

    while let Some(start) = cursor.find(&open_pattern) {
        cursor = &cursor[start + open_pattern.len()..];
        if let Some(tag_end) = cursor.find('>') {
            cursor = &cursor[tag_end + 1..];
            if let Some(end) = cursor.find(&close_pattern) {
                let piece = clean_docx_text(&decode_entities(&cursor[..end]));
                if !piece.is_empty() {
                    text.push_str(&piece);
                    text.push('\n');
                }
                cursor = &cursor[end + close_pattern.len()..];
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
) -> (
    ExtractedDocument,
    Vec<ChunkRecord>,
    Vec<ParsedBlock>,
    Vec<PdfOcrTask>,
) {
    let ParsedDocument {
        title: _,
        file_type: _,
        content,
        chunks: parsed_chunks,
        blocks: parsed_blocks,
        ocr_tasks,
    } = parsed;
    let content = normalize_whitespace(&content);
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

    let ocr_tasks = ocr_tasks.unwrap_or_default();

    let chunks = parsed_chunks
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

    let blocks = parsed_blocks.unwrap_or_default();

    eprintln!(
        "[SeekMind] convert_document path={} chunks={} blocks={} ocr_tasks={}",
        file.path.to_string_lossy(),
        chunks.len(),
        blocks.len(),
        ocr_tasks.len()
    );

    (document, chunks, blocks, ocr_tasks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

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

    #[test]
    fn extracts_epub_chapters_in_spine_order() {
        let temp_dir =
            std::env::temp_dir().join(format!("seekmind-epub-test-{}", uuid::Uuid::new_v4()));
        let epub_path = temp_dir.with_extension("epub");

        fs::create_dir_all(&temp_dir).expect("failed to create temp dir");

        {
            let file = fs::File::create(&epub_path).expect("failed to create epub file");
            let mut writer = zip::ZipWriter::new(file);
            let options = zip::write::FileOptions::default();

            writer
                .start_file("META-INF/container.xml", options)
                .expect("failed to write container");
            writer
                .write_all(
                    br#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#,
                )
                .expect("failed to write container xml");

            writer
                .start_file("OEBPS/content.opf", options)
                .expect("failed to write opf");
            writer
                .write_all(
                    br#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="bookid">
  <manifest>
    <item id="chap1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
    <item id="chap2" href="chapter2.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="chap1"/>
    <itemref idref="chap2"/>
  </spine>
</package>"#,
                )
                .expect("failed to write opf xml");

            writer
                .start_file("OEBPS/chapter1.xhtml", options)
                .expect("failed to write chapter1");
            writer
                .write_all(
                    "<html><body><h1>第一章</h1><p>EPUB 第一段内容。</p></body></html>".as_bytes(),
                )
                .expect("failed to write chapter1 html");

            writer
                .start_file("OEBPS/chapter2.xhtml", options)
                .expect("failed to write chapter2");
            writer
                .write_all(
                    "<html><body><h1>第二章</h1><p>EPUB 第二段内容。</p></body></html>".as_bytes(),
                )
                .expect("failed to write chapter2 html");

            writer.finish().expect("failed to finish epub");
        }

        let document = extract_document_at("", &epub_path).expect("epub should be extractable");
        assert_eq!(document.ext, "epub");
        assert!(document.content.contains("EPUB 第一段内容"));
        assert!(document.content.contains("EPUB 第二段内容"));

        let _ = fs::remove_file(&epub_path);
        let _ = fs::remove_dir_all(&temp_dir);
    }
}
