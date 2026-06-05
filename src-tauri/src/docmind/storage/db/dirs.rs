/**
 * @author MorningSun
 * @CreatedDate 2026/06/05
 * @Description DocMind 目录统计与目录树视图逻辑。
 */

use super::util::{is_path_within_dir, is_virtual_directory, normalize_directory_path};
use super::rows::{DocumentPathRow, IndexDirRow};
use super::{Database, DirectoryAggregate};

impl Database {
    pub async fn list_index_dirs(&self) -> Result<Vec<crate::docmind::models::IndexDirView>, sqlx::Error> {
        let explicit_rows = sqlx::query_as::<_, IndexDirRow>(
            r#"
            SELECT path, enabled, docs, chunks, status
            FROM index_dirs
            ORDER BY path
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let document_rows = sqlx::query_as::<_, DocumentPathRow>(
            r#"
            SELECT
                d.path,
                d.dir_path,
                COUNT(c.id) AS chunks
            FROM documents d
            LEFT JOIN chunks c ON c.document_id = d.id
            GROUP BY d.id, d.path, d.dir_path
            ORDER BY d.path
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut nodes = std::collections::HashMap::<String, DirectoryAggregate>::new();
        let explicit_paths = explicit_rows
            .iter()
            .map(|row| normalize_directory_path(&row.path))
            .collect::<Vec<_>>();

        for row in explicit_rows {
            let path = normalize_directory_path(&row.path);
            nodes.insert(
                path.clone(),
                DirectoryAggregate {
                    path,
                    enabled: row.enabled != 0,
                    docs: 0,
                    chunks: 0,
                    status: row.status,
                    is_explicit: true,
                },
            );
        }

        let mut sorted_explicit_paths = explicit_paths.clone();
        sorted_explicit_paths
            .sort_by(|left, right| left.len().cmp(&right.len()).then(left.cmp(right)));

        for doc in document_rows {
            let doc_path = normalize_directory_path(&doc.path);
            if doc_path.is_empty() {
                continue;
            }

            let doc_parent = normalize_directory_path(
                &std::path::Path::new(&doc_path)
                    .parent()
                    .and_then(|value| value.to_str())
                    .unwrap_or("")
                    .to_string(),
            );
            if doc_parent.is_empty() {
                continue;
            }

            let matching_roots = sorted_explicit_paths
                .iter()
                .filter(|root| {
                    if is_virtual_directory(root) {
                        doc.dir_path == root.as_str()
                    } else {
                        is_path_within_dir(&doc_path, root)
                    }
                })
                .cloned()
                .collect::<Vec<_>>();

            if matching_roots.is_empty() {
                continue;
            }

            for root in matching_roots {
                if is_virtual_directory(&root) {
                    if doc.dir_path == root {
                        let entry =
                            nodes
                                .entry(root.clone())
                                .or_insert_with(|| DirectoryAggregate {
                                    path: root.clone(),
                                    enabled: true,
                                    docs: 0,
                                    chunks: 0,
                                    status: "indexed".to_string(),
                                    is_explicit: true,
                                });
                        entry.docs += 1;
                        entry.chunks += doc.chunks as usize;
                    }
                    continue;
                }

                let mut current = doc_parent.clone();
                loop {
                    if !is_path_within_dir(&current, &root) && current != root {
                        break;
                    }

                    let entry =
                        nodes
                            .entry(current.clone())
                            .or_insert_with(|| DirectoryAggregate {
                                path: current.clone(),
                                enabled: false,
                                docs: 0,
                                chunks: 0,
                                status: "empty".to_string(),
                                is_explicit: false,
                            });
                    entry.docs += 1;
                    entry.chunks += doc.chunks as usize;

                    if current == root {
                        break;
                    }

                    let parent = normalize_directory_path(
                        &std::path::Path::new(&current)
                            .parent()
                            .and_then(|value| value.to_str())
                            .unwrap_or("")
                            .to_string(),
                    );
                    if parent.is_empty() || parent == current {
                        break;
                    }
                    current = parent;
                }
            }
        }

        let mut node_paths = nodes.keys().cloned().collect::<Vec<_>>();
        node_paths.sort_by(|left, right| left.len().cmp(&right.len()).then(left.cmp(right)));

        let mut enabled_snapshot = nodes
            .iter()
            .map(|(path, node)| (path.clone(), node.enabled))
            .collect::<std::collections::HashMap<_, _>>();

        for path in &node_paths {
            if let Some(node) = nodes.get_mut(path) {
                if !node.is_explicit {
                    let parent = normalize_directory_path(
                        &std::path::Path::new(path)
                            .parent()
                            .and_then(|value| value.to_str())
                            .unwrap_or("")
                            .to_string(),
                    );
                    node.enabled = enabled_snapshot.get(&parent).copied().unwrap_or(false);
                }

                if node.status.trim().is_empty() {
                    node.status = if node.docs > 0 {
                        "indexed".to_string()
                    } else {
                        "empty".to_string()
                    };
                }

                enabled_snapshot.insert(path.clone(), node.enabled);
            }
        }

        let mut rows = nodes
            .into_values()
            .map(|node| crate::docmind::models::IndexDirView {
                path: node.path,
                enabled: node.enabled,
                docs: node.docs,
                chunks: node.chunks,
                status: node.status,
                is_explicit: node.is_explicit,
            })
            .collect::<Vec<_>>();
        rows.sort_by(|left, right| {
            left.path
                .len()
                .cmp(&right.path.len())
                .then(left.path.cmp(&right.path))
        });
        Ok(rows)
    }
}
