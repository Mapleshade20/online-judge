use super::*;

impl Sandbox {
    /// Compiles the source code and returns compilation result
    pub(super) fn compile_source_code(
        &self,
        job: &mut JobRecord,
        language: &OneLanguageConfig,
    ) -> anyhow::Result<CompilationResult> {
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

    /// Creates a timestamped cache directory for this run
    fn create_timestamped_cache_dir(&self) -> anyhow::Result<PathBuf> {
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
    ) -> anyhow::Result<()> {
        let sandbox_id = self.id.to_string();
        let processes_arg = format!("--processes={}", COMPILE_PROCESSES);
        let open_files_arg = format!("--open-files={}", COMPILE_OPEN_FILES);
        let fsize_arg = format!("--fsize={}", COMPILE_FILE_SIZE);
        let wall_time_arg = format!("--wall-time={}", COMPILE_TIME_LIMIT);
        let memory_arg = format!("--cg-mem={}", COMPILE_MEMORY_LIMIT);
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
        cache_dir: &PathBuf,
        executable_name: &str,
    ) -> anyhow::Result<bool> {
        // Record compilation output
        job.cases[0].info = fs::read_to_string(&paths.stdout).unwrap_or_default();

        // Check compilation status
        let meta_content = fs::read_to_string(&paths.meta)?;
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
