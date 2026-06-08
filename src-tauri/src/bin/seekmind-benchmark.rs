/**
 * @author MorningSun
 * @CreatedDate 2026/06/08
 * @Description SeekMind 阶段 8 性能基线命令行工具，输出索引与搜索基线报告。
 */
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use chrono::Utc;
use seekmind_lib::seekmind;
use serde::Serialize;

const DEFAULT_QUERY_LIMIT: usize = 20;
const DEFAULT_REPEAT: usize = 3;
const DEFAULT_HISTORY_LIMIT: i64 = 8;
const DEFAULT_QUERIES: [&str; 5] = ["设计", "系统", "WBS", "测试", "集成"];

#[derive(Debug, Clone)]
struct BenchmarkOptions {
    output_path: Option<PathBuf>,
    limit: usize,
    repeat: usize,
    history_limit: i64,
    queries_file: Option<PathBuf>,
    queries: Vec<String>,
}

#[derive(Debug, Serialize)]
struct BenchmarkReport {
    generated_at: String,
    sqlite_path: String,
    query_limit: usize,
    repeat: usize,
    max_rss_mb: f64,
    open_database_ms: u128,
    load_index_status_ms: u128,
    load_parser_runtime_ms: u128,
    load_semantic_status_ms: u128,
    parser_runtime: ParserRuntimeSummary,
    semantic_status: SemanticStatusSummary,
    index_status: IndexStatusSummary,
    queries: Vec<QueryBenchmarkResult>,
    latency_ms: LatencySummary,
}

#[derive(Debug, Serialize)]
struct ParserRuntimeSummary {
    active: String,
    python_available: bool,
    pdf_ocr_available: bool,
    office_available: bool,
    chinese_ocr_available: bool,
}

#[derive(Debug, Serialize)]
struct SemanticStatusSummary {
    enabled_model: String,
    model_available: bool,
    needs_rebuild: bool,
    sqlite_chunks: usize,
    embedded_chunks: usize,
}

#[derive(Debug, Serialize)]
struct IndexStatusSummary {
    indexed_docs: usize,
    indexed_chunks: usize,
    scanned_docs: usize,
    failed_files: usize,
    pdf_ocr_tasks: usize,
}

#[derive(Debug, Serialize)]
struct QueryBenchmarkResult {
    query: String,
    hit_count: usize,
    best_ms: f64,
    avg_ms: f64,
    worst_ms: f64,
    samples_ms: Vec<f64>,
}

#[derive(Debug, Serialize)]
struct LatencySummary {
    min_ms: f64,
    avg_ms: f64,
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    max_ms: f64,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("[SeekMind][Benchmark] failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let options = parse_args(env::args().skip(1).collect())?;
    eprintln!(
        "[SeekMind][Benchmark] start limit={} repeat={} history_limit={} output={}",
        options.limit,
        options.repeat,
        options.history_limit,
        options
            .output_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<stdout>".to_string())
    );

    let sqlite_path = seekmind::storage::db::sqlite_database_path();
    let open_started = Instant::now();
    let database =
        tauri::async_runtime::block_on(seekmind::storage::Database::open_or_init_read_only_index())?;
    let open_database_ms = open_started.elapsed().as_millis();
    eprintln!(
        "[SeekMind][Benchmark] database opened path={} elapsed_ms={open_database_ms}",
        sqlite_path.display()
    );

    let status_started = Instant::now();
    let index_status = tauri::async_runtime::block_on(database.get_index_status())
        .map_err(|error| error.to_string())?;
    let load_index_status_ms = status_started.elapsed().as_millis();
    eprintln!(
        "[SeekMind][Benchmark] index status docs={} chunks={} elapsed_ms={load_index_status_ms}",
        index_status.indexed_docs, index_status.indexed_chunks
    );

    let parser_started = Instant::now();
    let parser_runtime = tauri::async_runtime::block_on(seekmind::commands::get_parser_runtime())?;
    let load_parser_runtime_ms = parser_started.elapsed().as_millis();
    eprintln!(
        "[SeekMind][Benchmark] parser runtime active={} pdf_ocr={} office={} elapsed_ms={load_parser_runtime_ms}",
        parser_runtime.active, parser_runtime.pdf_ocr_available, parser_runtime.office_available
    );

    let semantic_started = Instant::now();
    let semantic_status =
        tauri::async_runtime::block_on(seekmind::semantic::store::get_embedding_model_status(
            &database,
        ))?;
    let load_semantic_status_ms = semantic_started.elapsed().as_millis();
    eprintln!(
        "[SeekMind][Benchmark] semantic model={} available={} elapsed_ms={load_semantic_status_ms}",
        semantic_status.model.name, semantic_status.model.available
    );

    let queries = resolve_queries(&database, &options)?;
    eprintln!(
        "[SeekMind][Benchmark] resolved queries count={} values={:?}",
        queries.len(),
        queries
    );

    let mut query_reports = Vec::with_capacity(queries.len());
    let mut all_samples = Vec::new();
    for query in queries {
        let mut samples_ms = Vec::with_capacity(options.repeat);
        let mut hit_count = 0;
        for round in 0..options.repeat {
            let search_started = Instant::now();
            let hits = tauri::async_runtime::block_on(database.search_documents(
                &query,
                options.limit,
            ))
            .map_err(|error| error.to_string())?;
            let elapsed_ms = duration_ms(search_started.elapsed());
            hit_count = hits.len();
            samples_ms.push(elapsed_ms);
            all_samples.push(elapsed_ms);
            eprintln!(
                "[SeekMind][Benchmark] search query=\"{}\" round={} hits={} elapsed_ms={:.2}",
                query,
                round + 1,
                hit_count,
                elapsed_ms
            );
        }

        query_reports.push(QueryBenchmarkResult {
            query,
            hit_count,
            best_ms: samples_ms
                .iter()
                .copied()
                .reduce(f64::min)
                .unwrap_or_default(),
            avg_ms: average(&samples_ms),
            worst_ms: samples_ms
                .iter()
                .copied()
                .reduce(f64::max)
                .unwrap_or_default(),
            samples_ms,
        });
    }

    let report = BenchmarkReport {
        generated_at: Utc::now().to_rfc3339(),
        sqlite_path: sqlite_path.display().to_string(),
        query_limit: options.limit,
        repeat: options.repeat,
        max_rss_mb: process_max_rss_mb(),
        open_database_ms,
        load_index_status_ms,
        load_parser_runtime_ms,
        load_semantic_status_ms,
        parser_runtime: ParserRuntimeSummary {
            active: parser_runtime.active,
            python_available: parser_runtime.available,
            pdf_ocr_available: parser_runtime.pdf_ocr_available,
            office_available: parser_runtime.office_available,
            chinese_ocr_available: parser_runtime.chinese_ocr_available,
        },
        semantic_status: SemanticStatusSummary {
            enabled_model: semantic_status.model.name,
            model_available: semantic_status.model.available,
            needs_rebuild: semantic_status.needs_rebuild,
            sqlite_chunks: semantic_status.sqlite_chunks,
            embedded_chunks: semantic_status.embedded_chunks,
        },
        index_status: IndexStatusSummary {
            indexed_docs: index_status.indexed_docs,
            indexed_chunks: index_status.indexed_chunks,
            scanned_docs: index_status.scanned_docs,
            failed_files: index_status.failed_files,
            pdf_ocr_tasks: index_status.pdf_ocr_tasks,
        },
        queries: query_reports,
        latency_ms: LatencySummary {
            min_ms: percentile(&all_samples, 0.0),
            avg_ms: average(&all_samples),
            p50_ms: percentile(&all_samples, 0.50),
            p95_ms: percentile(&all_samples, 0.95),
            p99_ms: percentile(&all_samples, 0.99),
            max_ms: percentile(&all_samples, 1.0),
        },
    };

    let report_json =
        serde_json::to_string_pretty(&report).map_err(|error| format!("serialize report: {error}"))?;
    if let Some(output_path) = options.output_path {
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("create output dir {}: {error}", parent.display()))?;
        }
        fs::write(&output_path, &report_json)
            .map_err(|error| format!("write report {}: {error}", output_path.display()))?;
        eprintln!(
            "[SeekMind][Benchmark] report written path={}",
            output_path.display()
        );
    }

    println!("{report_json}");
    Ok(())
}

fn parse_args(args: Vec<String>) -> Result<BenchmarkOptions, String> {
    let mut output_path = None;
    let mut limit = DEFAULT_QUERY_LIMIT;
    let mut repeat = DEFAULT_REPEAT;
    let mut history_limit = DEFAULT_HISTORY_LIMIT;
    let mut queries_file = None;
    let mut queries = Vec::new();

    let mut index = 0;
    while index < args.len() {
        let current = &args[index];
        match current.as_str() {
            "--output" => {
                index += 1;
                output_path = Some(PathBuf::from(next_arg(&args, index, "--output")?));
            }
            "--limit" => {
                index += 1;
                limit = parse_usize(next_arg(&args, index, "--limit")?, "--limit")?;
            }
            "--repeat" => {
                index += 1;
                repeat = parse_usize(next_arg(&args, index, "--repeat")?, "--repeat")?.max(1);
            }
            "--history-limit" => {
                index += 1;
                history_limit = next_arg(&args, index, "--history-limit")?
                    .parse::<i64>()
                    .map_err(|error| format!("invalid --history-limit: {error}"))?
                    .max(1);
            }
            "--queries-file" => {
                index += 1;
                queries_file = Some(PathBuf::from(next_arg(&args, index, "--queries-file")?));
            }
            "--query" => {
                index += 1;
                queries.push(next_arg(&args, index, "--query")?.to_string());
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            other => {
                return Err(format!("unknown arg: {other}. use --help for usage"));
            }
        }
        index += 1;
    }

    Ok(BenchmarkOptions {
        output_path,
        limit,
        repeat,
        history_limit,
        queries_file,
        queries,
    })
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
        "SeekMind benchmark\n\
         --output <path>         optional report file path\n\
         --limit <number>        search result limit, default 20\n\
         --repeat <number>       repeat count per query, default 3\n\
         --history-limit <num>   fallback search history count, default 8\n\
         --queries-file <path>   newline separated query file\n\
         --query <text>          append one query, can repeat"
    );
}

fn resolve_queries(
    database: &seekmind::storage::Database,
    options: &BenchmarkOptions,
) -> Result<Vec<String>, String> {
    if !options.queries.is_empty() {
        return Ok(normalize_queries(options.queries.clone()));
    }

    if let Some(path) = &options.queries_file {
        let queries = read_queries_file(path)?;
        if !queries.is_empty() {
            return Ok(queries);
        }
    }

    let history = tauri::async_runtime::block_on(database.list_search_history(options.history_limit))
        .map_err(|error| error.to_string())?;
    let history_queries = history
        .into_iter()
        .map(|item| item.query)
        .collect::<Vec<_>>();
    let normalized_history = normalize_queries(history_queries);
    if !normalized_history.is_empty() {
        return Ok(normalized_history);
    }

    Ok(DEFAULT_QUERIES.iter().map(|item| item.to_string()).collect())
}

fn read_queries_file(path: &Path) -> Result<Vec<String>, String> {
    let content =
        fs::read_to_string(path).map_err(|error| format!("read queries file {}: {error}", path.display()))?;
    Ok(normalize_queries(
        content.lines().map(|line| line.to_string()).collect(),
    ))
}

fn normalize_queries(queries: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    for query in queries {
        let trimmed = query.trim().to_string();
        if trimmed.is_empty() || normalized.contains(&trimmed) {
            continue;
        }
        normalized.push(trimmed);
    }
    normalized
}

fn duration_ms(duration: std::time::Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn average(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

fn percentile(values: &[f64], ratio: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let last_index = sorted.len().saturating_sub(1);
    let index = ((last_index as f64) * ratio.clamp(0.0, 1.0)).round() as usize;
    sorted[index.min(last_index)]
}

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
