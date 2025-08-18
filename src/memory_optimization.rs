use std::collections::HashMap;
use std::sync::Arc;

/// String constants to avoid repeated allocations
pub struct StringConstants {
    pub waiting: Arc<str>,
    pub queueing: Arc<str>,
    pub running: Arc<str>,
    pub finished: Arc<str>,
    pub canceled: Arc<str>,
    pub skipped: Arc<str>,
    pub accepted: Arc<str>,
    pub wrong_answer: Arc<str>,
    pub runtime_error: Arc<str>,
    pub time_limit_exceeded: Arc<str>,
    pub compilation_error: Arc<str>,
    pub system_error: Arc<str>,
}

impl StringConstants {
    pub fn new() -> Self {
        Self {
            waiting: "Waiting".into(),
            queueing: "Queueing".into(),
            running: "Running".into(),
            finished: "Finished".into(),
            canceled: "Canceled".into(),
            skipped: "Skipped".into(),
            accepted: "Accepted".into(),
            wrong_answer: "Wrong Answer".into(),
            runtime_error: "Runtime Error".into(),
            time_limit_exceeded: "Time Limit Exceeded".into(),
            compilation_error: "Compilation Error".into(),
            system_error: "System Error".into(),
        }
    }
}

impl Default for StringConstants {
    fn default() -> Self {
        Self::new()
    }
}

/// Global string constants instance
static CONSTANTS: std::sync::OnceLock<StringConstants> = std::sync::OnceLock::new();

/// String cache for commonly used strings to reduce allocations
static STRING_CACHE: std::sync::OnceLock<parking_lot::RwLock<HashMap<&'static str, Arc<str>>>> =
    std::sync::OnceLock::new();

pub fn get_constants() -> &'static StringConstants {
    CONSTANTS.get_or_init(StringConstants::new)
}

fn get_string_cache() -> &'static parking_lot::RwLock<HashMap<&'static str, Arc<str>>> {
    STRING_CACHE.get_or_init(|| {
        let mut cache = HashMap::new();
        let constants = get_constants();

        // Pre-populate cache with common strings
        cache.insert("Waiting", constants.waiting.clone());
        cache.insert("Queueing", constants.queueing.clone());
        cache.insert("Running", constants.running.clone());
        cache.insert("Finished", constants.finished.clone());
        cache.insert("Canceled", constants.canceled.clone());
        cache.insert("Skipped", constants.skipped.clone());
        cache.insert("Accepted", constants.accepted.clone());
        cache.insert("Wrong Answer", constants.wrong_answer.clone());
        cache.insert("Runtime Error", constants.runtime_error.clone());
        cache.insert("Time Limit Exceeded", constants.time_limit_exceeded.clone());
        cache.insert("Compilation Error", constants.compilation_error.clone());
        cache.insert("System Error", constants.system_error.clone());

        parking_lot::RwLock::new(cache)
    })
}

/// Optimized string creation that reuses common constants
pub fn get_or_create_string(s: &str) -> String {
    // First try to get from cache
    {
        let cache = get_string_cache().read();
        if let Some(cached) = cache.get(s) {
            return cached.to_string();
        }
    }

    // If not in cache and it's a commonly used string, add it to cache
    if is_common_string(s) {
        let mut cache = get_string_cache().write();
        let arc_str: Arc<str> = s.into();
        cache.insert(Box::leak(s.to_string().into_boxed_str()), arc_str.clone());
        arc_str.to_string()
    } else {
        s.to_string()
    }
}

/// Check if a string is commonly used and should be cached
fn is_common_string(s: &str) -> bool {
    matches!(
        s,
        "Waiting"
            | "Queueing"
            | "Running"
            | "Finished"
            | "Canceled"
            | "Skipped"
            | "Accepted"
            | "Wrong Answer"
            | "Runtime Error"
            | "Time Limit Exceeded"
            | "Compilation Error"
            | "System Error"
            | "ERR_NOT_FOUND"
            | "ERR_INTERNAL"
            | "ERR_EXTERNAL"
            | "ERR_INVALID_ARGUMENT"
    )
}

/// Pre-allocate vectors with known capacity to avoid reallocations
pub fn create_case_results_with_capacity(capacity: usize) -> Vec<crate::routes::CaseResult> {
    Vec::with_capacity(capacity)
}

/// Optimize timestamp creation to reuse format
pub fn create_timestamp() -> String {
    use chrono::{SecondsFormat, Utc};
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Pre-allocate string with capacity to avoid reallocations
pub fn create_string_with_capacity(capacity: usize) -> String {
    String::with_capacity(capacity)
}

/// Reusable buffer pool for temporary operations
pub struct BufferPool {
    strings: parking_lot::Mutex<Vec<String>>,
    vectors: parking_lot::Mutex<Vec<Vec<u8>>>,
}

impl BufferPool {
    pub fn new() -> Self {
        Self {
            strings: parking_lot::Mutex::new(Vec::new()),
            vectors: parking_lot::Mutex::new(Vec::new()),
        }
    }

    pub fn get_string(&self) -> String {
        let mut pool = self.strings.lock();
        pool.pop().unwrap_or_else(|| String::with_capacity(1024))
    }

    pub fn return_string(&self, mut s: String) {
        s.clear();
        if s.capacity() <= 4096 {
            // Don't keep very large strings
            let mut pool = self.strings.lock();
            if pool.len() < 16 {
                // Limit pool size
                pool.push(s);
            }
        }
    }

    pub fn get_vector(&self) -> Vec<u8> {
        let mut pool = self.vectors.lock();
        pool.pop().unwrap_or_else(|| Vec::with_capacity(1024))
    }

    pub fn return_vector(&self, mut v: Vec<u8>) {
        v.clear();
        if v.capacity() <= 4096 {
            // Don't keep very large vectors
            let mut pool = self.vectors.lock();
            if pool.len() < 16 {
                // Limit pool size
                pool.push(v);
            }
        }
    }
}

static BUFFER_POOL: std::sync::OnceLock<BufferPool> = std::sync::OnceLock::new();

pub fn get_buffer_pool() -> &'static BufferPool {
    BUFFER_POOL.get_or_init(BufferPool::new)
}
