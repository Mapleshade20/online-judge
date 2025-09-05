use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use chrono::Local;
use tokio::time::timeout;

use crate::config::{JudgeType, OneCaseConfig, OneLanguageConfig, OneProblemConfig};
use crate::routes::JobRecord;

use super::{CompilationResult, SandboxRunner, TestCaseResult};

/// A simple runner that executes code without sandboxing
///
/// SimpleRunner provides basic code compilation and execution without the security
/// restrictions of isolate. It only provides timeout functionality but no memory,
/// file system, or permission controls. This is intended for development/testing
/// environments where security isolation is not critical.
#[allow(dead_code)]
pub struct SimpleRunner {
    /// Unique identifier for this instance
    id: u8,
    /// Path to the working directory for this runner
    work_dir: PathBuf,
    /// Path to the cache directory for temporary files
    cache_dir: PathBuf,
}

impl SandboxRunner for SimpleRunner {
    fn build(id: u8) -> Result<Self> {
        let work_dir = Self::create_work_directory(id)?;
        let cache_dir = Self::setup_cache_directory(id)?;

        log::info!("SimpleRunner {id} initialized successfully");
        log::warn!(
            "SimpleRunner provides NO security isolation - use only in trusted environments"
        );

        Ok(Self {
            id,
            work_dir,
            cache_dir,
        })
    }

    fn run(
        &self,
        mut job: JobRecord,
        problem: OneProblemConfig,
        language: OneLanguageConfig,
    ) -> Result<JobRecord> {
        self.cleanup_work_dir()?;

        // Step 1: Compile the source code
        let compilation_result = self.compile_source_code(&mut job, &language)?;
        if !compilation_result.success {
            return Ok(job);
        }

        // Step 2: Run test cases
        self.run_test_cases(&mut job, &problem, compilation_result.cache_dir)?;

        Ok(job)
    }

    fn compile_source_code(
        &self,
        job: &mut JobRecord,
        language: &OneLanguageConfig,
    ) -> Result<CompilationResult> {
        // Write source code to work directory
        let source_path = self.work_dir.join(&language.file_name);
        fs::write(&source_path, format!("{}\n", &job.submission.source_code))?;

        // Set up compilation paths
        let timestamped_cache_dir = self.create_timestamped_cache_dir()?;
        let executable_name = if cfg!(windows) { "main.exe" } else { "main" };
        let executable_path = self.work_dir.join(executable_name);
        let compile_output_path = self.work_dir.join("compile_stdout.txt");

        // Generate compile command
        let compile_command = self.generate_compile_command(
            language,
            &source_path.to_string_lossy(),
            &executable_path.to_string_lossy(),
        )?;

        // Execute compilation with timeout
        let start_time = Instant::now();
        let compilation_result = tokio::runtime::Handle::current().block_on(async {
            timeout(
                Duration::from_secs(30), // Compile timeout
                self.execute_compile_command_async(&compile_command, &compile_output_path),
            )
            .await
        });
        let compile_time = start_time.elapsed();

        let mut result = TestCaseResult {
            time: compile_time.as_micros() as u32,
            memory: 0, // No memory tracking in simple mode
            error: None,
            info: String::new(),
            stdout_content: String::new(),
        };

        // Check compilation result
        match compilation_result {
            Ok(Ok(exit_status)) => {
                if !exit_status.success() {
                    result.error = Some("Compilation Error");
                }
            }
            Ok(Err(e)) => {
                result.error = Some("System Error");
                result.info = format!("Compilation process error: {e}");
            }
            Err(_) => {
                result.error = Some("Time Limit Exceeded");
                result.info = "Compilation timeout".to_string();
            }
        }

        // Read compilation output
        job.cases[0].info = fs::read_to_string(&compile_output_path).unwrap_or_default();
        job.cases[0].time = result.time;
        job.cases[0].memory = result.memory;

        let compilation_success = result.error.is_none() && executable_path.exists();

        if !compilation_success {
            job.cases[0].result = "Compilation Error".to_string();
            job.result = "Compilation Error".to_string();
            job.state = "Finished".to_string();
            return Ok(CompilationResult {
                success: false,
                cache_dir: timestamped_cache_dir,
            });
        }

        job.cases[0].result = "Compilation Success".to_string();

        // Move executable to cache directory
        let cached_executable = timestamped_cache_dir.join(executable_name);
        fs::rename(&executable_path, &cached_executable)?;

        Ok(CompilationResult {
            success: true,
            cache_dir: timestamped_cache_dir,
        })
    }

    fn run_test_cases(
        &self,
        job: &mut JobRecord,
        problem: &OneProblemConfig,
        cache_dir: PathBuf,
    ) -> Result<()> {
        let mut total_score = 0.0;
        let mut first_error: Option<&str> = None;

        let executable_name = if cfg!(windows) { "main.exe" } else { "main" };
        let executable_path = cache_dir.join(executable_name);

        for (idx, case_config) in problem.cases.iter().enumerate() {
            let case_idx = idx + 1; // Add 1 because case 0 is compilation
            job.cases[case_idx].result = "Running".to_string();

            let test_result = self.run_single_test_case(case_idx, case_config, &executable_path)?;

            job.cases[case_idx].time = test_result.time;
            job.cases[case_idx].memory = test_result.memory;

            if let Some(error) = test_result.error {
                job.cases[case_idx].result = error.to_string();
                job.cases[case_idx].info = test_result.info;
                first_error = first_error.or(Some(error));
            } else {
                // Check program output
                let is_correct = self.check_output_correctness(
                    &test_result.stdout_content,
                    case_config,
                    problem,
                )?;

                if is_correct {
                    job.cases[case_idx].result = "Accepted".to_string();
                    total_score += case_config.score;
                } else {
                    job.cases[case_idx].result = "Wrong Answer".to_string();
                    first_error = first_error.or(Some("Wrong Answer"));
                }
            }
        }

        job.score = total_score;
        job.result = first_error.map_or("Accepted".to_string(), |e| e.to_string());
        job.state = "Finished".to_string();

        Ok(())
    }
}

impl SimpleRunner {
    /// Creates a working directory for this runner instance
    fn create_work_directory(id: u8) -> Result<PathBuf> {
        let work_dir = std::env::temp_dir().join("oj-simple").join(id.to_string());
        fs::create_dir_all(&work_dir)?;
        Ok(work_dir)
    }

    /// Sets up the cache directory for the runner
    fn setup_cache_directory(id: u8) -> Result<PathBuf> {
        use directories::ProjectDirs;

        let proj_dirs = ProjectDirs::from("", "", "oj")
            .ok_or_else(|| anyhow!("Unable to find user directory"))?;

        let cache_base_dir = proj_dirs.cache_dir().join("simple");
        fs::create_dir_all(&cache_base_dir)?;

        let cache_dir = cache_base_dir.join(id.to_string());
        fs::create_dir_all(&cache_dir)?;

        Ok(cache_dir)
    }

    /// Cleans the working directory
    fn cleanup_work_dir(&self) -> Result<()> {
        if self.work_dir.exists() {
            fs::remove_dir_all(&self.work_dir)?;
            fs::create_dir_all(&self.work_dir)?;
        }
        Ok(())
    }

    /// Creates a timestamped cache directory for this run
    fn create_timestamped_cache_dir(&self) -> Result<PathBuf> {
        let timestamped_cache_dir = self
            .cache_dir
            .join(Local::now().format("%y%m%d-%H-%M-%S").to_string());
        fs::create_dir_all(&timestamped_cache_dir)?;
        Ok(timestamped_cache_dir)
    }

    /// Generates the compile command by applying template substitutions
    fn generate_compile_command(
        &self,
        language: &OneLanguageConfig,
        source_path: &str,
        executable_path: &str,
    ) -> Result<Vec<String>> {
        let mut mapping = HashMap::<&str, &str>::new();
        mapping.insert("%INPUT%", source_path);
        mapping.insert("%OUTPUT%", executable_path);

        let command: Vec<String> = language
            .command
            .iter()
            .map(|s| {
                let mut t = s.clone();
                for (k, v) in mapping.iter() {
                    t = t.replace(k, v);
                }
                t
            })
            .collect();

        Ok(command)
    }

    /// Executes the compilation command asynchronously
    async fn execute_compile_command_async(
        &self,
        command: &[String],
        output_path: &Path,
    ) -> Result<std::process::ExitStatus> {
        if command.is_empty() {
            bail!("Empty compile command");
        }

        let output_file = fs::File::create(output_path)?;

        let mut cmd = tokio::process::Command::new(&command[0]);
        cmd.args(&command[1..])
            .stdout(Stdio::from(output_file.try_clone()?))
            .stderr(Stdio::from(output_file))
            .current_dir(&self.work_dir);

        let mut child = cmd.spawn()?;
        let output = child.wait().await?;

        Ok(output)
    }

    /// Runs a single test case and returns the result
    fn run_single_test_case(
        &self,
        case_idx: usize,
        case_config: &OneCaseConfig,
        executable_path: &Path,
    ) -> Result<TestCaseResult> {
        let input_content = fs::read_to_string(&case_config.input_file)?;
        let output_path = self.work_dir.join(format!("{case_idx}.out"));

        let start_time = Instant::now();

        // Execute the program with timeout
        let execution_result = tokio::runtime::Handle::current().block_on(async {
            let timeout_duration = Duration::from_micros(case_config.time_limit.0 as u64);

            timeout(
                timeout_duration,
                self.execute_program_async(executable_path, &input_content, &output_path),
            )
            .await
        });

        let elapsed_time = start_time.elapsed();
        let mut result = TestCaseResult {
            time: elapsed_time.as_micros() as u32,
            memory: 0, // No memory tracking in simple mode
            error: None,
            info: String::new(),
            stdout_content: String::new(),
        };

        match execution_result {
            Ok(Ok(exit_status)) => {
                if !exit_status.success() {
                    result.error = Some("Runtime Error");
                    result.info = format!("Process exited with code: {:?}", exit_status.code());
                }
            }
            Ok(Err(e)) => {
                result.error = Some("System Error");
                result.info = format!("Execution error: {e}");
            }
            Err(_) => {
                result.error = Some("Time Limit Exceeded");
                result.info = "Program execution timeout".to_string();
            }
        }

        // Read program output if no error occurred
        if result.error.is_none() {
            result.stdout_content = fs::read_to_string(&output_path)
                .map_err(|e| {
                    log::error!("Failed to read output file: {e}");
                    result.error = Some("System Error");
                    result.info = "Failed to read output file".to_string();
                    e
                })
                .unwrap_or_default();
        }

        Ok(result)
    }

    /// Executes the program asynchronously with input/output redirection
    async fn execute_program_async(
        &self,
        executable_path: &Path,
        input_content: &str,
        output_path: &Path,
    ) -> Result<std::process::ExitStatus> {
        let output_file = fs::File::create(output_path)?;

        let mut cmd = tokio::process::Command::new(executable_path);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::from(output_file))
            .stderr(Stdio::null())
            .current_dir(&self.work_dir);

        let mut child = cmd.spawn()?;

        // Write input to stdin
        if let Some(stdin) = child.stdin.take() {
            let mut stdin = tokio::io::BufWriter::new(stdin);
            use tokio::io::AsyncWriteExt;
            stdin.write_all(input_content.as_bytes()).await?;
            stdin.flush().await?;
        }

        let output = child.wait().await?;
        Ok(output)
    }

    /// Checks if the program output matches the expected output
    fn check_output_correctness(
        &self,
        program_output: &str,
        case_config: &OneCaseConfig,
        problem: &OneProblemConfig,
    ) -> Result<bool> {
        let expected_output = fs::read_to_string(&case_config.answer_file).map_err(|e| {
            log::error!("Failed to read answer file: {e}");
            anyhow!("Failed to read answer file: {}", e)
        })?;

        let is_correct = match problem.judge_type {
            JudgeType::Standard => compare_output_standard(program_output, &expected_output),
            JudgeType::Strict => compare_output_strict(program_output, &expected_output),
            _ => {
                log::warn!("Unsupported judge type: {:?}", problem.judge_type);
                false
            }
        };

        Ok(is_correct)
    }
}

/// Compares program output with expected output using standard mode
///
/// Standard mode ignores trailing empty lines and trailing spaces on each line.
/// This is more lenient than strict comparison and is suitable for most
/// programming contests.
fn compare_output_standard(program_output: &str, expected_output: &str) -> bool {
    let normalize = |s: &str| -> String {
        s.lines()
            .map(|line| line.trim_end())
            .collect::<Vec<_>>()
            .join("\n")
            .trim_end()
            .to_string()
    };

    let normalized_program = normalize(program_output);
    let normalized_expected = normalize(expected_output);

    normalized_program == normalized_expected
}

/// Compares program output with expected output using strict mode
///
/// Strict mode performs exact character-by-character comparison.
/// This is used when the output format is critical and no variations
/// are allowed.
#[inline]
fn compare_output_strict(program_output: &str, expected_output: &str) -> bool {
    program_output == expected_output
}
