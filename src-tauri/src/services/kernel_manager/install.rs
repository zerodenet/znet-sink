use super::transaction::BundleTransaction;
use super::*;

// CLI version probes briefly execute the installed binary. Serialize them
// with replacement/rollback so Windows never sees an app-owned probe holding
// the executable open during publication. Runtime lifecycle has its own owner.
static INSTALLED_FILES: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub(super) fn lock_installed_files() -> AppResult<std::sync::MutexGuard<'static, ()>> {
    INSTALLED_FILES
        .lock()
        .map_err(|_| AppError::internal("kernel installation lock poisoned"))
}

/// Downloaded and validated assets. Dropping this before installation only
/// removes the private staging directory; the running kernel is untouched.
pub struct PreparedKernelInstall {
    pub result: KernelInstallResult,
    pub(super) dir: PathBuf,
    pub(super) staged_bundle_dir: PathBuf,
    pub(super) executable_name: String,
    pub(super) bundle_files: Vec<String>,
    pub(super) previous_managed_files: BTreeSet<String>,
    pub(super) _workspace: KernelInstallWorkspace,
}

impl PreparedKernelInstall {
    pub fn backup(&self) -> AppResult<BundleTransaction> {
        let mut files = self.bundle_files.clone();
        files.push(RUNTIME_MANIFEST_FILE.to_owned());
        BundleTransaction::prepare(&self.dir, &data_dir()?.join("kernel-rollback"), &files)
    }

    /// The caller owns the stop/install/start transaction and only commits
    /// its backup after the new process and its capture settings are ready.
    pub fn install(&self) -> AppResult<()> {
        let _files = lock_installed_files()?;
        for name in &self.bundle_files {
            let source = self.staged_bundle_dir.join(name);
            let target = self.dir.join(name);
            if name != &self.executable_name
                && target.exists()
                && files_are_identical(&source, &target)?
            {
                continue;
            }
            #[cfg(unix)]
            if name == &self.executable_name {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&source, fs::Permissions::from_mode(0o755)).map_err(
                    |error| {
                        AppError::internal(format!("failed to set kernel permissions: {error}"))
                    },
                )?;
            }
            transaction::replace_file(&source, &target)?;
        }
        let mut files = self.previous_managed_files.clone();
        files.extend(self.bundle_files.iter().cloned());
        write_runtime_manifest(&self.dir, &files)
    }
}
