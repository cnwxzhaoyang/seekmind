/**
 * @author MorningSun
 * @CreatedDate 2026/06/08
 * @Description SeekMind 阶段 8 大文档库压测脚本，生成样本目录并输出全量索引压测报告。
 */
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use chrono::Utc;
use seekmind_lib::seekmind;
use serde::Serialize;

const DEFAULT_SIZES: [usize; 3] = [1_000, 5_000, 10_000];
const DEFAULT_FILES_PER_DIR: usize = 200;
const DEFAULT_FILE_WORDS: usize = 260;

#[derive(Debug, Clone)]
struct StressOptions {
    output_path: Option<PathBuf>,
    workspace: PathBuf,
    sizes: Vec<usize>,
    files_per_dir: usize,
    file_words: usize,
    query_limit: usize,
    cleanup: bool,
}

#[derive(Debug, Serialize)]
struct StressReport {
    generated_at: String,
    workspace: String,
    query_limit: usize,
    files_per_dir: usize,
    file_words: usize,
    runs: Vec<StressRunResult>,
}

#[derive(Debug, Serialize)]
struct StressRunResult {
    size: usize,
    corpus_root: String,
    created_files: usize,
    directories: usize,
    rebuild_ms: u128,
    search_ms: u128,
    max_rss_mb: f64,
    indexed_docs: usize,
    indexed_chunks: usize,
    failed_files: usize,
    pdf_ocr_tasks: usize,
    query_hits: Vec<QueryHitSummary>,
}

#[derive(Debug, Serialize)]
struct QueryHitSummary {
    query: String,
    hit_count: usize,
    elapsed_ms: f64,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("[SeekMind][Stress] failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let options = parse_args(env::args().skip(1).collect())?;
    fs::create_dir_all(&options.workspace)
        .map_err(|error| format!("create workspace {}: {error}", options.workspace.display()))?;

    let report = execute(&options)?;
    let report_json = serde_json::to_string_pretty(&report)
        .map_err(|error| format!("serialize report: {error}"))?;
    if let Some(output_path) = &options.output_path {
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("create report dir {}: {error}", parent.display()))?;
        }
        fs::write(output_path, &report_json)
            .map_err(|error| format!("write report {}: {error}", output_path.display()))?;
        eprintln!(
            "[SeekMind][Stress] report written path={}",
            output_path.display()
        );
    }
    println!("{report_json}");
    Ok(())
}

fn execute(options: &StressOptions) -> Result<StressReport, String> {
    let mut runs = Vec::new();
    for size in &options.sizes {
        let size = *size;
        let corpus_root = options.workspace.join(format!("corpus-{size}"));
        if corpus_root.exists() {
            fs::remove_dir_all(&corpus_root)
                .map_err(|error| format!("clear corpus {}: {error}", corpus_root.display()))?;
        }
        fs::create_dir_all(&corpus_root)
            .map_err(|error| format!("create corpus {}: {error}", corpus_root.display()))?;

        let created_files = generate_corpus(
            &corpus_root,
            size,
            options.files_per_dir,
            options.file_words,
        )?;
        let directories = count_directories(&corpus_root)?;

        let run_root = options.workspace.join(format!("run-{size}"));
        let data_root = run_root.join("data");
        let cache_root = run_root.join("cache");
        if run_root.exists() {
            fs::remove_dir_all(&run_root)
                .map_err(|error| format!("clear run root {}: {error}", run_root.display()))?;
        }
        fs::create_dir_all(&data_root)
            .map_err(|error| format!("create data root {}: {error}", data_root.display()))?;
        fs::create_dir_all(&cache_root)
            .map_err(|error| format!("create cache root {}: {error}", cache_root.display()))?;

        env::set_var("SEEKMIND_DATA_ROOT", &data_root);
        env::set_var("SEEKMIND_CACHE_ROOT", &cache_root);

        let open_started = Instant::now();
        let database = tauri::async_runtime::block_on(seekmind::storage::Database::open_or_init())
            .map_err(|error| format!("open benchmark database: {error}"))?;
        let _open_ms = open_started.elapsed().as_millis();

        let corpus_root_string = corpus_root
            .to_string_lossy()
            .trim_end_matches('/')
            .to_string();
        tauri::async_runtime::block_on(database.add_index_dir(&corpus_root_string))
            .map_err(|error| error.to_string())?;

        let rebuild_started = Instant::now();
        let status = tauri::async_runtime::block_on(seekmind::storage::indexer::rebuild_all(
            &database,
            &format!("stress-{size}"),
            std::sync::Arc::new(|_| {}),
        ))
        .map_err(|error| error.to_string())?;
        let rebuild_ms = rebuild_started.elapsed().as_millis();

        let search_started = Instant::now();
        let query_hits = run_search_probes(&database, size, options.query_limit)?;
        let search_ms = search_started.elapsed().as_millis();

        let max_rss_mb = process_max_rss_mb();
        eprintln!(
            "[SeekMind][Stress] size={} files={} dirs={} rebuild_ms={} search_ms={} docs={} chunks={}",
            size,
            created_files,
            directories,
            rebuild_ms,
            search_ms,
            status.indexed_docs,
            status.indexed_chunks
        );

        runs.push(StressRunResult {
            size,
            corpus_root: corpus_root.display().to_string(),
            created_files,
            directories,
            rebuild_ms,
            search_ms,
            max_rss_mb,
            indexed_docs: status.indexed_docs,
            indexed_chunks: status.indexed_chunks,
            failed_files: status.failed_files,
            pdf_ocr_tasks: status.pdf_ocr_tasks,
            query_hits,
        });
    }

    if options.cleanup {
        if let Err(error) = fs::remove_dir_all(&options.workspace) {
            eprintln!(
                "[SeekMind][Stress] cleanup failed workspace={} error={error}",
                options.workspace.display()
            );
        }
    }

    Ok(StressReport {
        generated_at: Utc::now().to_rfc3339(),
        workspace: options.workspace.display().to_string(),
        query_limit: options.query_limit,
        files_per_dir: options.files_per_dir,
        file_words: options.file_words,
        runs,
    })
}

fn parse_args(args: Vec<String>) -> Result<StressOptions, String> {
    let mut output_path = None;
    let mut workspace = std::env::temp_dir().join("seekmind-stress");
    let mut sizes = Vec::new();
    let mut files_per_dir = DEFAULT_FILES_PER_DIR;
    let mut file_words = DEFAULT_FILE_WORDS;
    let mut query_limit = 20usize;
    let mut cleanup = false;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--output" => {
                index += 1;
                output_path = Some(PathBuf::from(next_arg(&args, index, "--output")?));
            }
            "--workspace" => {
                index += 1;
                workspace = PathBuf::from(next_arg(&args, index, "--workspace")?);
            }
            "--sizes" => {
                index += 1;
                sizes = parse_sizes(next_arg(&args, index, "--sizes")?)?;
            }
            "--files-per-dir" => {
                index += 1;
                files_per_dir = parse_usize(
                    next_arg(&args, index, "--files-per-dir")?,
                    "--files-per-dir",
                )?;
            }
            "--file-words" => {
                index += 1;
                file_words = parse_usize(next_arg(&args, index, "--file-words")?, "--file-words")?;
            }
            "--query-limit" => {
                index += 1;
                query_limit =
                    parse_usize(next_arg(&args, index, "--query-limit")?, "--query-limit")?;
            }
            "--cleanup" => {
                cleanup = true;
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            other => return Err(format!("unknown arg: {other}")),
        }
        index += 1;
    }

    if sizes.is_empty() {
        sizes = DEFAULT_SIZES.to_vec();
    }

    Ok(StressOptions {
        output_path,
        workspace,
        sizes,
        files_per_dir,
        file_words,
        query_limit,
        cleanup,
    })
}

fn parse_sizes(raw: &str) -> Result<Vec<usize>, String> {
    let mut sizes = Vec::new();
    for item in raw.split(',') {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        sizes.push(
            trimmed
                .parse::<usize>()
                .map_err(|error| format!("invalid size {trimmed}: {error}"))?,
        );
    }
    Ok(sizes)
}

fn next_arg<'a>(args: &'a [String], index: usize, flag: &str) -> Result<&'a str, String> {
    args.get(index)
        .map(|value| value.as_str())
        .ok_or_else(|| format!("missing value for {flag}"))
}

fn parse_usize(raw: &str, flag: &str) -> Result<usize, String> {
    raw.parse::<usize>()
        .map_err(|error| format!("invalid {flag}: {error}"))
}

fn print_help() {
    println!(
        "SeekMind stress\n\
         --output <path>         optional report file path\n\
         --workspace <path>      working directory for generated corpora and temp DB\n\
         --sizes <a,b,c>         corpus sizes, default 1000,5000,10000\n\
         --files-per-dir <n>     files generated per subdir, default 200\n\
         --file-words <n>       words per generated file, default 260\n\
         --query-limit <n>       search result limit for probes, default 20\n\
         --cleanup               remove workspace after run"
    );
}

fn generate_corpus(
    corpus_root: &Path,
    size: usize,
    files_per_dir: usize,
    file_words: usize,
) -> Result<usize, String> {
    let subdir_count = ((size as f64) / (files_per_dir as f64)).ceil() as usize;
    let mut created = 0usize;
    for dir_index in 0..subdir_count.max(1) {
        let dir_path = corpus_root.join(format!("section-{:03}", dir_index + 1));
        fs::create_dir_all(&dir_path)
            .map_err(|error| format!("create corpus dir {}: {error}", dir_path.display()))?;
        let start = dir_index * files_per_dir;
        let end = (start + files_per_dir).min(size);
        for file_index in start..end {
            let ext = match file_index % 3 {
                0 => "md",
                1 => "txt",
                _ => "html",
            };
            let file_path = dir_path.join(format!("doc-{file_index:05}.{ext}"));
            let content = build_document_content(size, file_index, file_words);
            fs::write(&file_path, content)
                .map_err(|error| format!("write corpus file {}: {error}", file_path.display()))?;
            created += 1;
        }
    }
    Ok(created)
}

fn build_document_content(size: usize, file_index: usize, file_words: usize) -> String {
    let topic = match file_index % 5 {
        0 => "索引",
        1 => "搜索",
        2 => "WBS",
        3 => "测试",
        _ => "集成",
    };
    let mut body = String::new();
    body.push_str(&format!(
        "SeekMind stress corpus size={size} file_index={file_index} topic={topic}\n\n"
    ));
    for paragraph_index in 0..4 {
        body.push_str(&format!("## 段落 {paragraph_index}\n"));
        for word_index in 0..file_words {
            let token = match (word_index + file_index + paragraph_index) % 7 {
                0 => "文档",
                1 => "目录",
                2 => "切片",
                3 => "搜索",
                4 => "索引",
                5 => topic,
                _ => "性能",
            };
            body.push_str(token);
            if word_index % 18 == 17 {
                body.push('\n');
            } else {
                body.push(' ');
            }
        }
        body.push_str("\n\n");
    }
    body
}

fn count_directories(root: &Path) -> Result<usize, String> {
    let mut count = 0usize;
    for entry in fs::read_dir(root).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry.path().is_dir() {
            count += 1;
        }
    }
    Ok(count)
}

fn run_search_probes(
    database: &seekmind::storage::Database,
    size: usize,
    query_limit: usize,
) -> Result<Vec<QueryHitSummary>, String> {
    let queries = ["索引", "搜索", "WBS", "测试", "集成"];
    let mut summaries = Vec::with_capacity(queries.len());
    for query in queries {
        let started = Instant::now();
        let hits = tauri::async_runtime::block_on(database.search_documents(query, query_limit))
            .map_err(|error| error.to_string())?;
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        eprintln!(
            "[SeekMind][Stress] search probe size={} query={} hits={} elapsed_ms={elapsed:.2}",
            size,
            query,
            hits.len()
        );
        summaries.push(QueryHitSummary {
            query: query.to_string(),
            hit_count: hits.len(),
            elapsed_ms: elapsed,
        });
    }
    Ok(summaries)
}

#[cfg(unix)]
fn process_max_rss_mb() -> f64 {
    #[allow(unsafe_code)]
    unsafe {
        let mut usage = std::mem::zeroed::<libc::rusage>();
        if libc::getrusage(libc::RUSAGE_SELF, &mut usage) != 0 {
            return 0.0;
        }
        #[cfg(target_os = "macos")]
        {
            usage.ru_maxrss as f64 / 1024.0 / 1024.0
        }
        #[cfg(not(target_os = "macos"))]
        {
            usage.ru_maxrss as f64 / 1024.0
        }
    }
}

#[cfg(not(unix))]
fn process_max_rss_mb() -> f64 {
    // 修复：Windows 构建链没有 libc::getrusage，压测工具先降级返回 0，避免平台专属 API 阻塞编译。
    0.0
}
