/**
 * @author MorningSun
 * @CreatedDate 2026/06/21
 * @Description 语义模型按需下载、代理复用与本地模型包管理。
 */
use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use chrono::Utc;
use flate2::read::GzDecoder;
use reqwest::blocking::Client;
use reqwest::Proxy;
use serde_json::json;
use sha2::{Digest, Sha256};
use tar::Archive;
use tauri::{AppHandle, Emitter};

use crate::seekmind::models::{
    SemanticDownloadFileView, SemanticDownloadModelView, SemanticModelDownloadProgressView,
};
use crate::seekmind::runtime_paths::{
    fastembed_model_cache_dir, writable_fastembed_cache_dir, FASTEMBED_MODEL_CACHE_DIRNAME,
};
use crate::seekmind::storage::types::NetworkProxySettings;
use crate::seekmind::storage::Database;

const DEFAULT_SEMANTIC_MODEL_ID: &str = "bge-small-zh-v1.5-fastembed-cache";
const DEFAULT_SEMANTIC_MODEL_URL: &str =
    "https://github.com/cnwxzhaoyang/seekmind/releases/download/semantic-models-v1/fastembed-cache.tar.gz";
const DEFAULT_SEMANTIC_MODEL_SHA256: &str =
    "f9d6c509bbefacfdcc40b0b4aa1bbb7e83d3af6c662b6e842d6a3b2e386e2cf3";
const DEFAULT_SEMANTIC_MODEL_SIZE_BYTES: u64 = 55_371_779;

#[derive(Debug, Clone)]
struct SemanticDownloadModelManifest {
    id: &'static str,
    name: &'static str,
    provider: &'static str,
    version: &'static str,
    runtime: &'static str,
    dimension: usize,
    languages: &'static [&'static str],
    size_bytes: u64,
    recommended: bool,
    description: &'static str,
    archive_dirname: &'static str,
    file_name: &'static str,
    url: &'static str,
    sha256: &'static str,
}

fn semantic_model_manifest() -> Vec<SemanticDownloadModelManifest> {
    vec![SemanticDownloadModelManifest {
        id: DEFAULT_SEMANTIC_MODEL_ID,
        name: "BGE Small 中文",
        provider: "Qdrant / BAAI",
        version: "1.5",
        runtime: "fastembed",
        dimension: 512,
        languages: &["zh", "en"],
        size_bytes: DEFAULT_SEMANTIC_MODEL_SIZE_BYTES,
        recommended: true,
        description: "体积较小，适合普通电脑，中文知识库检索优先推荐。",
        archive_dirname: FASTEMBED_MODEL_CACHE_DIRNAME,
        file_name: "fastembed-cache.tar.gz",
        url: DEFAULT_SEMANTIC_MODEL_URL,
        sha256: DEFAULT_SEMANTIC_MODEL_SHA256,
    }]
}

fn manifest_to_view(item: &SemanticDownloadModelManifest) -> SemanticDownloadModelView {
    let local_dir = fastembed_model_cache_dir(&writable_fastembed_cache_dir());
    let downloaded = local_dir.exists();
    let download_ready = !item.url.trim().is_empty() && item.sha256.len() == 64;

    SemanticDownloadModelView {
        id: item.id.to_string(),
        name: item.name.to_string(),
        provider: item.provider.to_string(),
        version: item.version.to_string(),
        runtime: item.runtime.to_string(),
        dimension: item.dimension,
        languages: item.languages.iter().map(|value| value.to_string()).collect(),
        size_bytes: item.size_bytes,
        recommended: item.recommended,
        description: item.description.to_string(),
        download_ready,
        downloaded,
        status: if downloaded {
            "downloaded".to_string()
        } else if download_ready {
            "not_downloaded".to_string()
        } else {
            "source_not_configured".to_string()
        },
        local_dir: local_dir.display().to_string(),
        files: vec![SemanticDownloadFileView {
            name: item.file_name.to_string(),
            url: item.url.to_string(),
            sha256: item.sha256.to_string(),
            size_bytes: item.size_bytes,
        }],
    }
}

pub fn list_semantic_download_models() -> Vec<SemanticDownloadModelView> {
    semantic_model_manifest()
        .iter()
        .map(manifest_to_view)
        .collect()
}

fn emit_download_progress(
    app: &AppHandle,
    model_id: &str,
    state: &str,
    code: &str,
    downloaded_bytes: u64,
    total_bytes: u64,
    extra_params: serde_json::Value,
) {
    let percent = if total_bytes == 0 {
        0
    } else {
        ((downloaded_bytes.saturating_mul(100) / total_bytes).min(100)) as u8
    };
    let payload = SemanticModelDownloadProgressView {
        model_id: model_id.to_string(),
        state: state.to_string(),
        code: code.to_string(),
        params: json!({
            "model_id": model_id,
            "downloaded_bytes": downloaded_bytes,
            "total_bytes": total_bytes,
            "extra": extra_params,
        }),
        downloaded_bytes,
        total_bytes,
        percent,
        message: code.to_string(),
        updated_at: Utc::now().to_rfc3339(),
    };
    let _ = app.emit("seekmind:semantic:model-download-progress", payload);
}

fn semantic_downloads_dir() -> PathBuf {
    writable_fastembed_cache_dir().join("downloads")
}

fn build_http_client(proxy_settings: NetworkProxySettings) -> Result<Client, String> {
    let mut builder = Client::builder();

    if proxy_settings.enabled && !proxy_settings.proxy_url.trim().is_empty() {
        eprintln!(
            "[SeekMind] semantic model download uses configured proxy url={}",
            proxy_settings.proxy_url
        );
        let proxy = Proxy::all(proxy_settings.proxy_url.trim())
            .map_err(|error| format!("proxy_unreachable: {error}"))?;
        builder = builder.proxy(proxy);
    }

    builder.build().map_err(|error| error.to_string())
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn ensure_safe_tar_path(path: &Path) -> Result<(), String> {
    for component in path.components() {
        if matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_)) {
            return Err(format!("unsafe archive path: {}", path.display()));
        }
    }
    Ok(())
}

fn extract_fastembed_archive(archive_path: &Path, target_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(target_dir).map_err(|error| error.to_string())?;
    let file = fs::File::open(archive_path).map_err(|error| error.to_string())?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);

    for entry in archive.entries().map_err(|error| error.to_string())? {
        let mut entry = entry.map_err(|error| error.to_string())?;
        let entry_path = entry.path().map_err(|error| error.to_string())?.into_owned();
        ensure_safe_tar_path(&entry_path)?;
        let target_path = target_dir.join(&entry_path);
        entry.unpack(&target_path).map_err(|error| error.to_string())?;
    }

    Ok(())
}

pub async fn download_semantic_model(
    app: AppHandle,
    database: Database,
    model_id: String,
) -> Result<Vec<SemanticDownloadModelView>, String> {
    let manifest = semantic_model_manifest()
        .into_iter()
        .find(|item| item.id == model_id.trim())
        .ok_or_else(|| format!("model_incompatible: {}", model_id.trim()))?;

    if manifest.url.trim().is_empty() || manifest.sha256.len() != 64 {
        return Err("semantic_model_source_not_configured".to_string());
    }

    let app_for_task = app.clone();
    let model_id_for_task = manifest.id.to_string();
    let total_bytes_for_error = manifest.size_bytes;
    let proxy_settings = database
        .get_network_proxy_settings()
        .await
        .map_err(|error| error.to_string())?;
    let task_result = tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let client = build_http_client(proxy_settings)?;
        let downloads_dir = semantic_downloads_dir();
        fs::create_dir_all(&downloads_dir).map_err(|error| error.to_string())?;
        let archive_path = downloads_dir.join(format!("{}.tar.gz", manifest.id));
        let temp_archive_path = downloads_dir.join(format!("{}.tar.gz.part", manifest.id));

        emit_download_progress(
            &app_for_task,
            manifest.id,
            "running",
            "semantic_model.download.running",
            0,
            manifest.size_bytes,
            json!({ "phase": "request" }),
        );

        eprintln!(
            "[SeekMind] semantic model download start model={} url={}",
            manifest.id, manifest.url
        );
        let mut response = client
            .get(manifest.url)
            .send()
            .map_err(|error| format!("download_failed: {error}"))?;
        if !response.status().is_success() {
            return Err(format!("download_failed: http {}", response.status()));
        }

        let total_bytes = response.content_length().unwrap_or(manifest.size_bytes);
        let mut output = fs::File::create(&temp_archive_path).map_err(|error| error.to_string())?;
        let mut downloaded = 0u64;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let read = response
                .read(&mut buffer)
                .map_err(|error| format!("download_failed: {error}"))?;
            if read == 0 {
                break;
            }
            output
                .write_all(&buffer[..read])
                .map_err(|error| error.to_string())?;
            downloaded = downloaded.saturating_add(read as u64);
            emit_download_progress(
                &app_for_task,
                manifest.id,
                "running",
                "semantic_model.download.running",
                downloaded,
                total_bytes,
                json!({ "phase": "download" }),
            );
        }
        output.flush().map_err(|error| error.to_string())?;

        let actual_sha256 = sha256_file(&temp_archive_path)?;
        if actual_sha256 != manifest.sha256 {
            let _ = fs::remove_file(&temp_archive_path);
            return Err(format!(
                "checksum_failed: expected {} got {}",
                manifest.sha256, actual_sha256
            ));
        }

        fs::rename(&temp_archive_path, &archive_path).map_err(|error| error.to_string())?;
        extract_fastembed_archive(&archive_path, &writable_fastembed_cache_dir())?;

        let model_dir = fastembed_model_cache_dir(&writable_fastembed_cache_dir());
        if !model_dir.exists() {
            return Err(format!(
                "model_incompatible: archive missing {}",
                manifest.archive_dirname
            ));
        }

        emit_download_progress(
            &app_for_task,
            manifest.id,
            "completed",
            "semantic_model.download.completed",
            total_bytes,
            total_bytes,
            json!({ "phase": "completed" }),
        );
        eprintln!(
            "[SeekMind] semantic model download completed model={} dir={}",
            manifest.id,
            model_dir.display()
        );
        Ok(())
    })
    .await
    .map_err(|error| error.to_string())
    .and_then(|result| result);

    if let Err(error) = task_result {
        // 修复：下载失败也通过状态码事件通知前端，避免长任务面板停留在 running。
        emit_download_progress(
            &app,
            &model_id_for_task,
            "failed",
            "semantic_model.download.failed",
            0,
            total_bytes_for_error,
            json!({ "error": error.clone() }),
        );
        return Err(error);
    }

    eprintln!(
        "[SeekMind] semantic model download refresh model={}",
        model_id_for_task
    );
    Ok(list_semantic_download_models())
}

pub async fn delete_semantic_model(model_id: String) -> Result<Vec<SemanticDownloadModelView>, String> {
    let manifest = semantic_model_manifest()
        .into_iter()
        .find(|item| item.id == model_id.trim())
        .ok_or_else(|| format!("model_incompatible: {}", model_id.trim()))?;

    let model_dir = fastembed_model_cache_dir(&writable_fastembed_cache_dir());
    if model_dir.ends_with(manifest.archive_dirname) && model_dir.exists() {
        fs::remove_dir_all(&model_dir).map_err(|error| error.to_string())?;
        eprintln!(
            "[SeekMind] semantic model deleted model={} dir={}",
            manifest.id,
            model_dir.display()
        );
    }

    Ok(list_semantic_download_models())
}
