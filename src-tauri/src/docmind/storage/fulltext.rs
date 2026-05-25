#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::Mutex;

use dirs::{cache_dir, data_dir};
use tantivy::query::QueryParser;
use tantivy::schema::document::TantivyDocument;
use tantivy::schema::Value;
use tantivy::schema::{Field, Schema, STORED, STRING, TEXT};
use tantivy::{doc, Index, IndexReader, IndexWriter, ReloadPolicy, Term};

use crate::docmind::search::{normalize_query, normalize_search_text};
use crate::docmind::storage::types::{ChunkRecord, ExtractedDocument};

#[derive(Debug, Clone)]
pub struct SearchHit {
    pub chunk_id: String,
    pub score: f32,
}

pub struct SearchIndex {
    index: Index,
    reader: IndexReader,
    writer: Mutex<IndexWriter>,
    fields: SearchFields,
}

impl SearchIndex {
    pub fn open_or_init() -> Result<Self, String> {
        let index_dir = fulltext_index_dir();
        eprintln!("[DocMind] Tantivy index dir: {}", index_dir.display());
        ensure_index_dir_ready(&index_dir)?;

        let schema = build_schema();
        let index = match Index::open_in_dir(&index_dir) {
            Ok(index) => {
                if index.schema().get_field("content_text").is_err() {
                    std::fs::remove_dir_all(&index_dir).map_err(|error| error.to_string())?;
                    std::fs::create_dir_all(&index_dir).map_err(|error| error.to_string())?;
                    Index::create_in_dir(&index_dir, schema.clone())
                        .map_err(|error| error.to_string())?
                } else {
                    index
                }
            }
            Err(_) => Index::create_in_dir(&index_dir, schema.clone())
                .map_err(|error| error.to_string())?,
        };

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|error| error.to_string())?;

        let writer = index
            .writer(50_000_000)
            .map_err(|error| error.to_string())?;

        let fields = SearchFields::from_schema(&index.schema());

        Ok(Self {
            index,
            reader,
            writer: Mutex::new(writer),
            fields,
        })
    }

    pub fn doc_count(&self) -> u64 {
        self.reader.searcher().num_docs()
    }

    pub fn clear_all(&self) -> Result<(), String> {
        let mut writer = self.writer.lock().map_err(|error| error.to_string())?;
        writer
            .delete_all_documents()
            .map_err(|error| error.to_string())?;
        writer.commit().map_err(|error| error.to_string())?;
        self.reader.reload().map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn delete_directory(&self, dir_path: &str) -> Result<(), String> {
        let mut writer = self.writer.lock().map_err(|error| error.to_string())?;
        writer.delete_term(Term::from_field_text(self.fields.dir_path, dir_path));
        writer.commit().map_err(|error| error.to_string())?;
        self.reader.reload().map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn delete_document(&self, path: &str) -> Result<(), String> {
        let mut writer = self.writer.lock().map_err(|error| error.to_string())?;
        writer.delete_term(Term::from_field_text(self.fields.path_exact, path));
        writer.commit().map_err(|error| error.to_string())?;
        self.reader.reload().map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn index_document(
        &self,
        document_id: &str,
        document: &ExtractedDocument,
        chunks: &[ChunkRecord],
    ) -> Result<(), String> {
        let mut writer = self.writer.lock().map_err(|error| error.to_string())?;

        for (index, chunk) in chunks.iter().enumerate() {
            let chunk_id = format!("{document_id}:{index}");
            let search_text = normalize_search_text(&format!(
                "{} {} {} {}",
                document.file_name, document.path, chunk.heading, chunk.snippet
            ));

            let tantivy_doc = doc!(
                self.fields.chunk_id => chunk_id,
                self.fields.document_id => document_id.to_string(),
                self.fields.dir_path => document.dir_path.clone(),
                self.fields.path_exact => document.path.clone(),
                self.fields.file_name => normalize_search_text(&document.file_name),
                self.fields.path_text => normalize_search_text(&document.path),
                self.fields.ext => normalize_search_text(&document.ext),
                self.fields.heading => normalize_search_text(&chunk.heading),
                self.fields.snippet => normalize_search_text(&search_text),
                self.fields.content_text => normalize_search_text(&document.content),
            );

            writer
                .add_document(tantivy_doc)
                .map_err(|error| error.to_string())?;
        }

        writer.commit().map_err(|error| error.to_string())?;
        self.reader.reload().map_err(|error| error.to_string())?;
        eprintln!(
            "[DocMind] Tantivy indexed document path={} chunks={} total_docs={}",
            document.path,
            chunks.len(),
            self.doc_count()
        );
        Ok(())
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>, String> {
        let normalized_terms = normalize_query(query);
        if normalized_terms.is_empty() {
            return Ok(Vec::new());
        }

        let search_text = normalized_terms.join(" ");
        let searcher = self.reader.searcher();

        let mut hits = self.run_query(&searcher, &search_text, limit, true)?;
        if hits.is_empty() {
            hits = self.run_query(&searcher, &search_text, limit, false)?;
        }

        Ok(hits)
    }

    fn run_query(
        &self,
        searcher: &tantivy::Searcher,
        query_text: &str,
        limit: usize,
        conjunction: bool,
    ) -> Result<Vec<SearchHit>, String> {
        let fields = vec![
            self.fields.file_name,
            self.fields.path_text,
            self.fields.ext,
            self.fields.heading,
            self.fields.snippet,
            self.fields.content_text,
        ];

        let mut parser = QueryParser::for_index(&self.index, fields);
        if conjunction {
            parser.set_conjunction_by_default();
        }

        let query = parser
            .parse_query(query_text)
            .map_err(|error| error.to_string())?;
        let top_docs = searcher
            .search(
                &query,
                &tantivy::collector::TopDocs::with_limit(limit.max(1)),
            )
            .map_err(|error| error.to_string())?;

        let mut hits = Vec::new();
        for (score, address) in top_docs {
            let document: TantivyDocument =
                searcher.doc(address).map_err(|error| error.to_string())?;
            let chunk_id = document
                .get_first(self.fields.chunk_id)
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string();

            if !chunk_id.is_empty() {
                hits.push(SearchHit { chunk_id, score });
            }
        }

        Ok(hits)
    }
}

#[derive(Debug, Clone, Copy)]
struct SearchFields {
    chunk_id: Field,
    document_id: Field,
    dir_path: Field,
    path_exact: Field,
    file_name: Field,
    path_text: Field,
    ext: Field,
    heading: Field,
    snippet: Field,
    content_text: Field,
}

impl SearchFields {
    fn from_schema(schema: &Schema) -> Self {
        Self {
            chunk_id: schema
                .get_field("chunk_id")
                .expect("missing chunk_id field"),
            document_id: schema
                .get_field("document_id")
                .expect("missing document_id field"),
            dir_path: schema
                .get_field("dir_path")
                .expect("missing dir_path field"),
            path_exact: schema
                .get_field("path_exact")
                .expect("missing path_exact field"),
            file_name: schema
                .get_field("file_name")
                .expect("missing file_name field"),
            path_text: schema
                .get_field("path_text")
                .expect("missing path_text field"),
            ext: schema.get_field("ext").expect("missing ext field"),
            heading: schema.get_field("heading").expect("missing heading field"),
            snippet: schema.get_field("snippet").expect("missing snippet field"),
            content_text: schema
                .get_field("content_text")
                .expect("missing content_text field"),
        }
    }
}

fn build_schema() -> Schema {
    let mut builder = Schema::builder();
    builder.add_text_field("chunk_id", STRING | STORED);
    builder.add_text_field("document_id", STRING | STORED);
    builder.add_text_field("dir_path", STRING | STORED);
    builder.add_text_field("path_exact", STRING | STORED);
    builder.add_text_field("file_name", TEXT | STORED);
    builder.add_text_field("path_text", TEXT | STORED);
    builder.add_text_field("ext", STRING | STORED);
    builder.add_text_field("heading", TEXT | STORED);
    builder.add_text_field("snippet", TEXT | STORED);
    builder.add_text_field("content_text", TEXT | STORED);
    builder.build()
}

pub fn fulltext_index_dir() -> PathBuf {
    if let Ok(path) = std::env::var("DOCMIND_TANTIVY_DIR") {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    default_fulltext_index_dir()
}

#[cfg(debug_assertions)]
fn default_fulltext_index_dir() -> PathBuf {
    let base = cache_dir()
        .or_else(data_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("com.zhaoyang.docmind.dev").join("tantivy")
}

#[cfg(not(debug_assertions))]
fn default_fulltext_index_dir() -> PathBuf {
    let base = cache_dir()
        .or_else(data_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("com.zhaoyang.docmind").join("tantivy")
}

fn legacy_fulltext_index_dir() -> PathBuf {
    let base = data_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("DocMind").join("tantivy")
}

fn ensure_index_dir_ready(index_dir: &PathBuf) -> Result<(), String> {
    if index_dir.exists() {
        return Ok(());
    }

    let legacy_dir = legacy_fulltext_index_dir();
    if legacy_dir.exists() {
        if let Some(parent) = index_dir.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        if std::fs::rename(&legacy_dir, index_dir).is_ok() {
            return Ok(());
        }
    }

    std::fs::create_dir_all(index_dir).map_err(|error| error.to_string())
}
