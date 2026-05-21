use super::models::{FailedFileView, IndexDirView, SearchResultView};

pub fn index_dirs() -> Vec<IndexDirView> {
    vec![
        IndexDirView {
            path: "/Users/zhaoyang/Documents".to_string(),
            enabled: true,
            docs: 842,
            chunks: 13_280,
            status: "indexed".to_string(),
        },
        IndexDirView {
            path: "/Users/zhaoyang/Downloads".to_string(),
            enabled: true,
            docs: 196,
            chunks: 2_844,
            status: "indexing".to_string(),
        },
    ]
}

pub fn search_results() -> Vec<SearchResultView> {
    vec![
        SearchResultView {
            id: "1".to_string(),
            file_name: "Maven离线仓库方案.md".to_string(),
            path: "/Users/zhaoyang/Documents/dev/maven-offline.md".to_string(),
            ext: "md".to_string(),
            heading: "三、离线依赖导出方案".to_string(),
            snippet: "通过 dependency:go-offline 拉取 parent POM、BOM、plugin 依赖，并生成 .offline-repo 目录，用于内网环境构建。"
                .to_string(),
            paragraph: Some(5),
            page: None,
            modified: "今天 09:42".to_string(),
            score: 0.92,
        },
        SearchResultView {
            id: "2".to_string(),
            file_name: "SpringBoot内网构建记录.docx".to_string(),
            path: "/Users/zhaoyang/Documents/work/springboot-offline.docx".to_string(),
            ext: "docx".to_string(),
            heading: "BOM 与插件依赖处理".to_string(),
            snippet: "对于 spring-boot-dependencies、spring-cloud-dependencies 等 import BOM，需要递归解析 dependencyManagement 并下载对应 POM。"
                .to_string(),
            paragraph: Some(12),
            page: None,
            modified: "昨天 18:20".to_string(),
            score: 0.86,
        },
        SearchResultView {
            id: "3".to_string(),
            file_name: "Jenkins离线部署.pdf".to_string(),
            path: "/Users/zhaoyang/Documents/ops/jenkins-offline.pdf".to_string(),
            ext: "pdf".to_string(),
            heading: "插件目录与启动参数".to_string(),
            snippet: "插件可以放在 JENKINS_HOME/plugins 目录下，离线环境需要同时准备插件本体以及 transitive dependencies。"
                .to_string(),
            paragraph: None,
            page: Some(4),
            modified: "2026-04-10".to_string(),
            score: 0.78,
        },
    ]
}

pub fn failed_files() -> Vec<FailedFileView> {
    vec![
        FailedFileView {
            file: "扫描版合同.pdf".to_string(),
            reason: "暂不支持 OCR，无法提取文本".to_string(),
        },
        FailedFileView {
            file: "old-report.doc".to_string(),
            reason: "暂不支持旧版 .doc 格式".to_string(),
        },
    ]
}
