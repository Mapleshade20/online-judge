use std::path::PathBuf;

use anyhow::Result;

use crate::config::{OneLanguageConfig, OneProblemConfig};
use crate::routes::JobRecord;

use super::CompilationResult;

/// Trait for different sandbox execution implementations
///
/// This trait abstracts the core functionality needed for compiling and running
/// user code in different environments - from full isolation with `isolate`
/// to simple process execution without sandboxing.
pub trait SandboxRunner: Send + Sync {
    /// Creates a new sandbox runner instance with the given ID
    fn build(id: u8) -> Result<Self>
    where
        Self: Sized;

    /// Main entry point for running a job
    ///
    /// This method coordinates compilation and test case execution for a submitted job.
    fn run(
        &self,
        job: JobRecord,
        problem: OneProblemConfig,
        language: OneLanguageConfig,
    ) -> Result<JobRecord>;

    /// Compiles the source code and returns compilation result
    fn compile_source_code(
        &self,
        job: &mut JobRecord,
        language: &OneLanguageConfig,
    ) -> Result<CompilationResult>;

    /// Runs all test cases for the compiled program
    fn run_test_cases(
        &self,
        job: &mut JobRecord,
        problem: &OneProblemConfig,
        cache_dir: PathBuf,
    ) -> Result<()>;
}
