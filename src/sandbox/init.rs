use super::*;

impl Sandbox {
    /// Sets up the cache directory for the sandbox
    pub(super) fn setup_cache_directory(id: u8) -> anyhow::Result<PathBuf> {
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
    pub(super) fn initialize_isolate_sandbox(id: u8) -> anyhow::Result<PathBuf> {
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
    pub(super) fn reinit(&self) -> anyhow::Result<()> {
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

        log::debug!("Sandbox {} reinitialized", self.id);
        Ok(())
    }
}
