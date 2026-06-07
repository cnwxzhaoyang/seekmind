use futures_util::future::AbortHandle;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

static QA_JOB_CANCELS: OnceLock<Mutex<HashMap<String, AbortHandle>>> = OnceLock::new();

fn registry() -> &'static Mutex<HashMap<String, AbortHandle>> {
    QA_JOB_CANCELS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn register(job_id: String, abort_handle: AbortHandle) {
    if let Ok(mut registry) = registry().lock() {
        registry.insert(job_id, abort_handle);
    }
}

pub fn cancel(job_id: &str) -> bool {
    let handle = match registry().lock() {
        Ok(mut registry) => registry.remove(job_id),
        Err(_) => None,
    };

    if let Some(handle) = handle {
        handle.abort();
        true
    } else {
        false
    }
}

pub fn clear(job_id: &str) {
    if let Ok(mut registry) = registry().lock() {
        registry.remove(job_id);
    }
}
