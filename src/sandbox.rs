mod isolate_runner;
mod runner;
mod simple_runner;

// Re-export the trait and common types
use isolate_runner::IsolateRunner;
pub use runner::SandboxRunner;
use simple_runner::SimpleRunner;

use std::path::PathBuf;

use anyhow::Result;

/// Result of compilation process
#[derive(Debug)]
pub struct CompilationResult {
    pub success: bool,
    pub cache_dir: PathBuf,
}

/// Result of a single test case execution
#[derive(Debug)]
pub struct TestCaseResult {
    pub time: u32,
    pub memory: u32,
    pub error: Option<&'static str>,
    pub info: String,
    pub stdout_content: String,
}

/// Creates a sandbox runner based on environment configuration
///
/// If NO_ISOLATE environment variable is set to "1", creates a SimpleRunner
/// that provides basic timeout functionality without security isolation.
/// Otherwise, creates an IsolateRunner with full sandboxing capabilities.
pub fn create_sandbox_runner(id: u8) -> Result<Box<dyn SandboxRunner>> {
    let no_isolate = std::env::var("NO_ISOLATE").unwrap_or_default() == "1";

    if no_isolate {
        log::info!("Creating SimpleRunner {id} (NO_ISOLATE mode)");
        let runner = SimpleRunner::build(id)?;
        Ok(Box::new(runner))
    } else {
        log::info!("Creating IsolateRunner {id} (full isolation mode)");
        let runner = IsolateRunner::build(id)?;
        Ok(Box::new(runner))
    }
}

/// Check if we're in no-isolate mode
pub fn is_no_isolate_mode() -> bool {
    std::env::var("NO_ISOLATE").unwrap_or_default() == "1"
}
