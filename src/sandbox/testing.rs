use super::*;

impl Sandbox {
    /// Runs all test cases for the compiled program
    pub(super) fn run_test_cases(
        &self,
        job: &mut JobRecord,
        problem: &OneProblemConfig,
        cache_dir: PathBuf,
    ) -> anyhow::Result<()> {
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

    /// Runs a single test case and returns the result
    fn run_single_test_case(
        &self,
        case_idx: usize,
        case_config: &OneCaseConfig,
        cache_dir: &PathBuf,
    ) -> anyhow::Result<TestCaseResult> {
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
                    log::error!("Failed to read output file: {}", e);
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
    fn setup_test_case_paths(
        &self,
        case_idx: usize,
        cache_dir: &PathBuf,
    ) -> anyhow::Result<TestCasePaths> {
        let stdin_name = format!("{}.in", case_idx);
        let stdout_name = format!("{}.out", case_idx);
        let meta_name = format!("{}.meta", case_idx);

        Ok(TestCasePaths {
            stdin: self.box_dir.join(stdin_name),
            stdout: self.box_dir.join(stdout_name),
            meta: cache_dir.join(meta_name),
        })
    }

    /// Executes a test case in the sandbox
    fn execute_test_case(
        &self,
        case_config: &OneCaseConfig,
        paths: &TestCasePaths,
    ) -> anyhow::Result<()> {
        let wall_time_limit = Second::from(case_config.time_limit);
        let memory_limit = case_config.memory_limit;

        let sandbox_id = self.id.to_string();
        let wall_time_arg = format!("{:.4}", wall_time_limit.0 + 0.5);
        let memory_arg = format!("--cg-mem={}", memory_limit.0);
        let stack_arg = format!("--stack={}", memory_limit.0 / 2);
        let processes_arg = format!("--processes={}", RUNTIME_PROCESSES);
        let open_files_arg = format!("--open-files={}", RUNTIME_OPEN_FILES);
        let fsize_arg = format!("--fsize={}", RUNTIME_FILE_SIZE);
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
    pub(super) fn process_meta_content(&self, meta_content: &str, result: &mut TestCaseResult) {
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
    ) -> anyhow::Result<bool> {
        let expected_output = fs::read_to_string(&case_config.answer_file).map_err(|e| {
            log::error!("Failed to read answer file: {}", e);
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
