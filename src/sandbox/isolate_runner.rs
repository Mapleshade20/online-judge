use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use anyhow::{Result, anyhow, bail};
use chrono::Local;

use crate::config::{
    JudgeType, MicroSecond, OneCaseConfig, OneLanguageConfig, OneProblemConfig, Second,
};
use crate::routes::JobRecord;

use super::{CompilationResult, SandboxRunner, TestCaseResult};

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

/// A sandbox environment for compiling and executing code safely using isolate
///
/// The IsolateRunner provides an isolated environment where user-submitted code can be
/// compiled and executed with resource limits and security restrictions using Linux isolate.
pub struct IsolateRunner {
    /// Unique identifier for this sandbox instance
    id: u8,
    /// Path to the sandbox's working directory (inside isolate)
    box_dir: PathBuf,
    /// Path to the cache directory for temporary files
    cache_dir: PathBuf,
}

impl SandboxRunner for IsolateRunner {
    fn build(id: u8) -> Result<Self> {
        let cache_dir = Self::setup_cache_directory(id)?;
        let box_dir = Self::initialize_isolate_sandbox(id)?;

        log::info!("IsolateRunner {id} initialized successfully");
        Ok(Self {
            id,
            box_dir,
            cache_dir,
        })
    }

    fn run(
        &self,
        mut job: JobRecord,
        problem: OneProblemConfig,
        language: OneLanguageConfig,
    ) -> Result<JobRecord> {
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

    fn compile_source_code(
        &self,
        job: &mut JobRecord,
        language: &OneLanguageConfig,
    ) -> Result<CompilationResult> {
        // Write source code to sandbox
        let source_name = &language.file_name;
        fs::write(
            self.box_dir.join(source_name),
            format!("{}\n", &job.submission.source_code),
        )?;

        // Set up compilation paths
        let timestamped_cache_dir = self.create_timestamped_cache_dir()?;
        let executable_name = "main"; // NOTE: subject to change for `cargo`
        let compile_paths = CompilationPaths {
            executable: self.box_dir.join(executable_name),
            stdout: self.box_dir.join("compile_stdout.txt"),
            meta: timestamped_cache_dir.join("compile.meta"),
        };

        // Generate and run compile command
        let compile_command = self.generate_compile_command(language, source_name, executable_name);
        self.execute_compile_command(&compile_command, &compile_paths)?;

        // Process compilation results
        let compilation_success = self.process_compilation_results(
            job,
            &compile_paths,
            &timestamped_cache_dir,
            executable_name,
        )?;

        Ok(CompilationResult {
            success: compilation_success,
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

        for (idx, case_config) in problem.cases.iter().enumerate() {
            let case_idx = idx + 1; // Add 1 because case 0 is compilation
            job.cases[case_idx].result = "Running".to_string();

            let test_result = self.run_single_test_case(case_idx, case_config, &cache_dir)?;

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

impl IsolateRunner {
    /// Sets up the cache directory for the sandbox
    fn setup_cache_directory(id: u8) -> Result<PathBuf> {
        use directories::ProjectDirs;

        let proj_dirs = ProjectDirs::from("", "", "oj")
            .ok_or_else(|| anyhow!("Unable to find user directory"))?;

        let cache_base_dir = proj_dirs.cache_dir();
        fs::create_dir_all(cache_base_dir)?;
        fs::set_permissions(
            cache_base_dir,
            fs::Permissions::from_mode(CACHE_DIR_PERMISSIONS),
        )?;

        let cache_dir = cache_base_dir.join(id.to_string());
        fs::create_dir_all(&cache_dir)?;

        Ok(cache_dir)
    }

    /// Initializes the isolate sandbox and returns the box directory
    fn initialize_isolate_sandbox(id: u8) -> Result<PathBuf> {
        let output = Command::new("isolate")
            .arg("-b")
            .arg(id.to_string())
            .arg("--cg")
            .arg("--init")
            .output()
            .map_err(|e| anyhow!("Failed to spawn isolate --init: {}", e))?;

        if !output.status.success() {
            bail!("isolate --init exited with non-zero status");
        }

        let root_dir_absolute = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if root_dir_absolute.is_empty() {
            bail!(
                "isolate --init produced empty stdout; stderr={}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        Ok(PathBuf::from(root_dir_absolute).join("box"))
    }

    /// Reinitializes the sandbox by cleaning and setting it up again
    fn reinit(&self) -> Result<()> {
        let output = Command::new("isolate")
            .arg("-b")
            .arg(self.id.to_string())
            .arg("--cg")
            .arg("--init")
            .output()
            .map_err(|e| anyhow!("Failed to spawn isolate --init: {}", e))?;

        if !output.status.success() {
            bail!("isolate --init exited with non-zero status");
        }

        log::debug!("IsolateRunner {} reinitialized", self.id);
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
        source_name: &str,
        executable_name: &str,
    ) -> String {
        let mut mapping = HashMap::<&str, &str>::new();
        mapping.insert("%INPUT%", source_name);
        mapping.insert("%OUTPUT%", executable_name);
        apply_template_and_join(&language.command, &mapping)
    }

    /// Executes the compilation command in the sandbox
    fn execute_compile_command(
        &self,
        compile_command: &str,
        paths: &CompilationPaths,
    ) -> Result<()> {
        let sandbox_id = self.id.to_string();
        let processes_arg = format!("--processes={COMPILE_PROCESSES}");
        let open_files_arg = format!("--open-files={COMPILE_OPEN_FILES}");
        let fsize_arg = format!("--fsize={COMPILE_FILE_SIZE}");
        let wall_time_arg = format!("--wall-time={COMPILE_TIME_LIMIT}");
        let memory_arg = format!("--cg-mem={COMPILE_MEMORY_LIMIT}");
        let meta_path = paths.meta.to_string_lossy();
        let dir_args = if Path::new("/etc/alternatives").exists() {
            vec!["--dir=/opt/oj", "--dir=/etc/alternatives"]
        } else {
            vec!["--dir=/opt/oj"]
        };

        let _ = Command::new("isolate")
            .args(dir_args)
            .args([
                "-b", &sandbox_id,
                "--cg", "--run",
                &processes_arg,
                &open_files_arg,
                &fsize_arg,
                &wall_time_arg,
                &memory_arg,
                "-E", "RUSTUP_HOME=/opt/oj/rust/rustup",
                "-E", "CARGO_HOME=/opt/oj/rust/cargo", 
                "-E", "PATH=/opt/oj/rust/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
                "-M", &meta_path,
                "--silent",
                "--stderr-to-stdout",
                "-o", "compile_stdout.txt",
                "--",
                "/bin/sh", "-c", compile_command
            ])
            .output()?;

        Ok(())
    }

    /// Processes compilation results and updates job status
    fn process_compilation_results(
        &self,
        job: &mut JobRecord,
        paths: &CompilationPaths,
        cache_dir: &Path,
        executable_name: &str,
    ) -> Result<bool> {
        let mut result = TestCaseResult {
            time: 0,
            memory: 0,
            error: None,
            info: String::new(),
            stdout_content: String::new(),
        };

        // Read meta file for compilation information
        let meta_content = fs::read_to_string(&paths.meta)?;
        self.process_meta_content(&meta_content, &mut result);

        // Record compilation output
        job.cases[0].info = fs::read_to_string(&paths.stdout).unwrap_or_default();
        job.cases[0].time = result.time;
        job.cases[0].memory = result.memory;

        if meta_content.contains("status") || !paths.executable.exists() {
            job.cases[0].result = "Compilation Error".to_string();
            job.result = "Compilation Error".to_string();
            job.state = "Finished".to_string();
            return Ok(false);
        }

        job.cases[0].result = "Compilation Success".to_string();

        // Move executable to cache and prepare for test cases
        fs::rename(&paths.executable, cache_dir.join(executable_name))?;
        self.reinit()?; // Clean sandbox for test cases
        fs::rename(cache_dir.join(executable_name), &paths.executable)?;

        Ok(true)
    }

    /// Runs a single test case and returns the result
    fn run_single_test_case(
        &self,
        case_idx: usize,
        case_config: &OneCaseConfig,
        cache_dir: &Path,
    ) -> Result<TestCaseResult> {
        let paths = self.setup_test_case_paths(case_idx, cache_dir)?;

        // Prepare input file
        fs::copy(&case_config.input_file, &paths.stdin)?;

        // Execute the program
        let start_time = Instant::now();
        self.execute_test_case(case_config, &paths)?;
        let elapsed_time = start_time.elapsed();

        // Set result template
        let mut result = TestCaseResult {
            time: 0,
            memory: 0,
            error: None,
            info: String::new(),
            stdout_content: String::new(),
        };

        // Read meta file for execution information
        if let Ok(meta_content) = fs::read_to_string(&paths.meta) {
            self.process_meta_content(&meta_content, &mut result);
        } else {
            result.error = Some("System Error");
            result.info = "Failed to read meta file".to_string();
        }

        // Use external wall timer to modify the result
        if elapsed_time.as_micros() as u32 > case_config.time_limit.0 {
            result.time = elapsed_time.as_micros() as u32;
            result.error = Some("Time Limit Exceeded");
        }

        // Read program output if no error occurred
        if result.error.is_none() {
            result.stdout_content = fs::read_to_string(&paths.stdout)
                .map_err(|e| {
                    log::error!("Failed to read output file: {e}");
                    result.error = Some("System Error");
                    result.info = "Failed to read output file".to_string();
                    e
                })
                .unwrap_or_default();
        }

        // Move the stdout file to the cache directory  TODO: optional
        fs::rename(
            &paths.stdout,
            cache_dir.join(paths.stdout.file_name().unwrap()),
        )?;

        Ok(result)
    }

    /// Sets up file paths for a test case
    fn setup_test_case_paths(&self, case_idx: usize, cache_dir: &Path) -> Result<TestCasePaths> {
        let stdin_name = format!("{case_idx}.in");
        let stdout_name = format!("{case_idx}.out");
        let meta_name = format!("{case_idx}.meta");

        Ok(TestCasePaths {
            stdin: self.box_dir.join(stdin_name),
            stdout: self.box_dir.join(stdout_name),
            meta: cache_dir.join(meta_name),
        })
    }

    /// Executes a test case in the sandbox
    fn execute_test_case(&self, case_config: &OneCaseConfig, paths: &TestCasePaths) -> Result<()> {
        let wall_time_limit = Second::from(case_config.time_limit);
        let memory_limit = case_config.memory_limit;

        let sandbox_id = self.id.to_string();
        let wall_time_arg = format!("{:.4}", wall_time_limit.0 + 0.5);
        let memory_arg = format!("--cg-mem={}", memory_limit.0);
        let stack_arg = format!("--stack={}", memory_limit.0 / 2);
        let processes_arg = format!("--processes={RUNTIME_PROCESSES}");
        let open_files_arg = format!("--open-files={RUNTIME_OPEN_FILES}");
        let fsize_arg = format!("--fsize={RUNTIME_FILE_SIZE}");
        let meta_path = paths.meta.to_string_lossy();
        let stdin_name = paths.stdin.file_name().unwrap().to_string_lossy();
        let stdout_name = paths.stdout.file_name().unwrap().to_string_lossy();

        let _ = Command::new("isolate")
            .args([
                "-b",
                &sandbox_id,
                "--cg",
                "--run",
                "-w",
                &wall_time_arg,
                &memory_arg,
                &stack_arg,
                &processes_arg,
                &open_files_arg,
                &fsize_arg,
                "-E",
                "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
                "-M",
                &meta_path,
                "-i",
                &stdin_name,
                "-o",
                &stdout_name,
                "--stderr-to-stdout",
                "--silent",
                "--",
                "./main",
            ])
            .output()?;

        Ok(())
    }

    /// Processes the meta file content and updates the test result
    fn process_meta_content(&self, meta_content: &str, result: &mut TestCaseResult) {
        for line in meta_content.lines() {
            if let Some((key, value)) = line.split_once(':') {
                match key {
                    "killed" => {
                        // killed:1
                        result.error = Some("Time Limit Exceeded"); // no exitcode
                    }
                    "cg-oom-killed" => {
                        // cg-oom-killed:1
                        result.error = Some("Memory Limit Exceeded");
                    }
                    "exitcode" => {
                        if value != "0" && result.error.is_none() {
                            result.error = Some("Runtime Error");
                        }
                    }
                    "cg-mem" => {
                        if let Ok(memory) = value.parse::<u32>() {
                            result.memory = memory;
                        }
                    }
                    "message" => {
                        result.info = value.to_string();
                    }
                    "time-wall" => {
                        if let Ok(secs) = value.parse::<f64>() {
                            result.time = MicroSecond::from(Second(secs)).0;
                        }
                    }
                    _ => {}
                }
            }
        }
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

impl Drop for IsolateRunner {
    fn drop(&mut self) {
        let out = Command::new("isolate")
            .arg("-b")
            .arg(self.id.to_string())
            .arg("--cg")
            .arg("--cleanup")
            .output();

        if out.is_ok_and(|c| c.status.success()) {
            log::info!("IsolateRunner {} cleaned up", self.id);
        } else {
            log::error!("IsolateRunner {} failed to clean up", self.id);
        }
    }
}

/// Applies template substitutions to command arguments and joins them
///
/// This function takes a command template (array of strings) and a mapping
/// of placeholders to actual values, then replaces all occurrences and
/// joins the result into a single command string.
fn apply_template_and_join(cmd_template: &[String], mapping: &HashMap<&str, &str>) -> String {
    let replaced: Vec<String> = cmd_template
        .iter()
        .map(|s| {
            let mut t = s.clone();
            for (k, v) in mapping.iter() {
                t = t.replace(k, v);
            }
            t
        })
        .collect();

    replaced.join(" ")
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
