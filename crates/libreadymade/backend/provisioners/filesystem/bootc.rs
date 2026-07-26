use crate::{
    backend::{mounts::generate_cryptdata, provisioners::filesystem::FileSystemProvisionerModule},
    prelude::*,
};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Bootc {
    pub imgref: String,
    pub target_imgref: Option<String>,
    pub enforce_sigpolicy: bool,
    pub kargs: Vec<String>,
    pub args: Vec<String>,
}

impl Bootc {
    /// Call bootc to copy the contents of the container into the target.
    ///
    /// The caller must verify that `self.copy_mode.is_bootc()`.
    #[allow(clippy::unwrap_in_result, clippy::needless_pass_by_value)]
    pub fn bootc_copy(&self, target_root: &Path, cryptdata: Option<CryptData>) -> Result<()> {
        let imgref = &self.imgref;
        let target_imgref = &self.target_imgref;
        let enforce_sigpolicy = &self.enforce_sigpolicy;
        let kargs = &self.kargs;
        let args = &self.args;
        tracing::info!(imgref=?self.imgref, "running bootc install to-filesystem");

        let cmd = Command::new("bootc")
            .args(["install", "to-filesystem", "--source-imgref", imgref])
            .args(cryptdata.iter().flat_map(|data| {
                data.cmdline_opts
                    .iter()
                    .flat_map(|opt| ["--karg", opt.as_str()])
            }))
            .arg(target_root)
            .args(
                target_imgref
                    .iter()
                    .flat_map(|a| ["--target-imgref", a.as_str()]),
            )
            .args(kargs.iter().flat_map(|e| ["--karg", e.as_str()]))
            .args(enforce_sigpolicy.then_some("--enforce-container-sigpolicy"))
            .args(args)
            .status()
            .context("fail to execute bootc")?;
        if !cmd.success() {
            bail!("`bootc install to-filesystem` failed: {:?}", cmd.code());
        }

        Ok(())
    }

    /// Complete an installation started with `bootc install to-filesystem
    /// --skip-finalize`.
    ///
    /// The official finalizer commits changes made by post-install modules
    /// inside the target checkout before pruning its temporary files. Deleting
    /// the checkout by hand loses changes such as `/etc/locale.conf`.
    fn bootc_finalize(mountpoint: &Path) -> Result<()> {
        let status = Self::bootc_finalize_command(mountpoint)
            .status()
            .context("failed to execute `bootc install finalize`")?;
        if !status.success() {
            bail!("`bootc install finalize` failed: {:?}", status.code());
        }
        Ok(())
    }

    fn bootc_finalize_command(mountpoint: &Path) -> Command {
        let mut command = Command::new("bootc");
        command.args(["install", "finalize"]).arg(mountpoint);
        command
    }
}

impl FileSystemProvisionerModule for Bootc {
    fn run(&self, playbook: &crate::playbook::Playbook, mounts: &Mounts) -> Result<()> {
        // bootc overlays /tmp with a private tmpfs while preparing an external
        // source image. A target mounted below /tmp would become hidden, and
        // the subsequent unmount would fail with EINVAL. /run remains visible
        // for the complete bootc invocation.
        let tmproot = tempfile::Builder::new()
            .prefix("readymade-bootc-")
            .tempdir_in("/run")?;
        let bootc_rootfs_mountpoint = tmproot.path();
        mounts.mount_all(
            bootc_rootfs_mountpoint,
            playbook
                .encryption
                .as_ref()
                .map(|e| e.encryption_key.as_str()),
        )?;

        self.bootc_copy(bootc_rootfs_mountpoint, generate_cryptdata(mounts)?)?;

        mounts
            .umount_all(bootc_rootfs_mountpoint)
            .wrap_err("unmounting target filesystems after bootc")?;
        Ok(())
    }

    fn cleanup(&self, playbook: &crate::playbook::Playbook, mounts: &Mounts) -> Result<()> {
        let tmproot = tempfile::Builder::new()
            .prefix("readymade-bootc-cleanup-")
            .tempdir_in("/run")?;
        let bootc_rootfs_mountpoint = tmproot.path();
        mounts
            .mount_all(
                bootc_rootfs_mountpoint,
                playbook
                    .encryption
                    .as_ref()
                    .map(|e| e.encryption_key.as_str()),
            )
            .wrap_err("mounting target filesystems for bootc cleanup")?;
        Self::bootc_finalize(bootc_rootfs_mountpoint)?;
        // Finalize may already have remounted or detached the target. The
        // Mount abstraction treats EINVAL/ENOENT as an idempotent success.
        mounts
            .umount_all(bootc_rootfs_mountpoint)
            .wrap_err("unmounting target filesystems after bootc finalize")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Bootc;
    use std::ffi::OsStr;
    use std::path::Path;

    #[test]
    fn finalizes_the_customized_target_with_bootc() {
        let command = Bootc::bootc_finalize_command(Path::new("/mnt/target"));
        assert_eq!(command.get_program(), OsStr::new("bootc"));
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            ["install", "finalize", "/mnt/target"].map(OsStr::new)
        );
    }
}
