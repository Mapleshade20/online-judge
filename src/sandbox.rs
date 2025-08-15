mod compile;
mod init;
mod testing;

use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use anyhow::{anyhow, bail};
use chrono::Local;

use crate::config::{
    JudgeType, MicroSecond, OneCaseConfig, OneLanguageConfig, OneProblemConfig, Second,
};
use crate::routes::JobRecord;

// Sandbox configuration constants
const COMPILE_TIME_LIMIT: f64 = 30.0; // seconds
const COMPILE_MEMORY_LIMIT: u32 = 262144; // KB
const COMPILE_PROCESSES: u32 = 10;
const COMPILE_OPEN_FILES: u32 = 512;
const COMPILE_FILE_SIZE: u32 = 65536; // KB

const RUNTIME_PROCESSES: u32 = 4;
const RUNTIME_OPEN_FILES: u32 = 30;
const RUNTIME_FILE_SIZE: u32 = 16384; // KB

// Sandbox cache directory permissions
const CACHE_DIR_PERMISSIONS: u32 = 0o700;

/// Result of compilation process
#[derive(Debug)]
struct CompilationResult {
    success: bool,
    cache_dir: PathBuf,
}

/// Paths used during compilation
#[derive(Debug)]
struct CompilationPaths {
    executable: PathBuf,
    stdout: PathBuf,
    meta: PathBuf,
}

/// Paths used during test case execution
#[derive(Debug)]
struct TestCasePaths {
    stdin: PathBuf,
    stdout: PathBuf,
    meta: PathBuf,
}

/// Result of a single test case execution
#[derive(Debug)]
struct TestCaseResult {
    time: u32,
    memory: u32,
    error: Option<&'static str>,
    info: String,
    stdout_content: String,
}

/// A sandbox environment for compiling and executing code safely using isolate
///
/// The Sandbox provides an isolated environment where user-submitted code can be
/// compiled and executed with resource limits and security restrictions.
pub struct Sandbox {
    /// Unique identifier for this sandbox instance
    id: u8,
    /// Path to the sandbox's working directory (inside isolate)
    box_dir: PathBuf,
    /// Path to the cache directory for temporary files
    cache_dir: PathBuf,
}

impl Sandbox {
    /// Creates a new sandbox instance with the given ID
    pub fn build(id: u8) -> anyhow::Result<Self> {
        let cache_dir = Self::setup_cache_directory(id)?;
        let box_dir = Self::initialize_isolate_sandbox(id)?;

        log::info!("Sandbox {} initialized successfully", id);
        Ok(Self {
            id,
            box_dir,
            cache_dir,
        })
    }

    /// Main entry point for running a job in the sandbox
    pub fn run(
        &self,
        mut job: JobRecord,
        problem: OneProblemConfig,
        language: OneLanguageConfig,
    ) -> anyhow::Result<JobRecord> {
        self.reinit()?;

        // Step 1: Compile the source code
        let compilation_result = self.compile_source_code(&mut job, &language)?;
        if !compilation_result.success {
            return Ok(job);
        }

        // Step 2: Run test cases
        self.run_test_cases(&mut job, &problem, compilation_result.cache_dir)?;

        Ok(job)
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let out = Command::new("isolate")
            .arg("-b")
            .arg(self.id.to_string())
            .arg("--cg")
            .arg("--cleanup")
            .output();

        if out.is_ok_and(|c| c.status.success()) {
            log::info!("Sandbox {} cleaned up", self.id);
        } else {
            log::error!("Sandbox {} failed to clean up", self.id);
        }
    }
}
