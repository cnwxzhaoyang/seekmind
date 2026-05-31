use crate::docmind::models::{QaRetrievalView, QaSourceView};

#[derive(Debug, Clone)]
pub struct QaSourceBlock {
    pub source: QaSourceView,
    pub block: String,
}

#[derive(Debug, Clone)]
pub struct QaContext {
    pub sources: Vec<QaSourceBlock>,
    pub retrieval: QaRetrievalView,
}
