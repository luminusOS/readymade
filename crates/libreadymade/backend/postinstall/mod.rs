use crate::prelude::*;

use cleanup_boot::CleanupBoot;
use color_eyre::Result;
use cryptsetup::CryptSetup;
use dracut::Dracut;
use efi_stub::EfiStub;
use enum_dispatch::enum_dispatch;
use fstab::Fstab;
use grub2::GRUB2;
use initial_setup::InitialSetup;
use keyboard::Keyboard;
use language::Language;
use prepare_fedora::PrepareFedora;
use reinstall_kernel::ReinstallKernel;
use script::Script;
use selinux::SELinux;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub mod cleanup_boot;
pub mod cryptsetup;
pub mod dracut;
pub mod efi_stub;
pub mod fstab;
pub mod grub2;
pub mod initial_setup;
pub mod keyboard;
pub mod language;
pub mod prepare_fedora;
pub mod reinstall_kernel;
pub mod script;
pub mod selinux;

#[derive(serde::Serialize)]
pub struct Context {
    pub destination_disk: PathBuf,
    pub uefi: bool,
    /// Root of the system that will actually boot.
    ///
    /// For a traditional installation this is `/`. A bootc filesystem is an
    /// OSTree sysroot, so `/` is only the storage root and the bootable system
    /// lives below `/ostree/deploy/.../deploy/<checksum>.<serial>`.
    pub root: PathBuf,
    // pub esp_partition: Option<String>,
    // Installs should always have an xbootldr partition
    // pub xbootldr_partition: String,
    // pub crypt_data: Option<CryptData>,
    pub mounts: Mounts,
}

/// Resolve the root that post-install modules must modify.
///
/// `bootc install to-filesystem` creates an OSTree sysroot. Chrooting into the
/// filesystem root therefore does not chroot into the deployment that will
/// boot. Follow OSTree's active boot link and make modules write into that
/// deployment instead. Silently falling back to the sysroot when OSTree is
/// present would produce an apparently successful install with lost settings.
pub(crate) fn resolve_target_root(sysroot: &Path) -> Result<PathBuf> {
    let ostree = sysroot.join("ostree");
    if !ostree.exists() {
        return Ok(sysroot.to_path_buf());
    }

    let loader_link = std::fs::read_link(sysroot.join("boot/loader"))
        .wrap_err("reading active OSTree loader link")?;
    let loader_name = loader_link
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| eyre!("active OSTree loader link is not valid UTF-8"))?;
    let boot_version = loader_name
        .strip_prefix("loader.")
        .filter(|version| matches!(*version, "0" | "1"))
        .ok_or_else(|| eyre!("unexpected active OSTree loader target: {loader_name}"))?;
    let boot_link = ostree.join(format!("boot.{boot_version}"));

    let canonical_sysroot =
        std::fs::canonicalize(sysroot).wrap_err("resolving installation sysroot")?;
    let boot_root =
        std::fs::canonicalize(&boot_link).wrap_err("resolving active OSTree boot link")?;
    let mut deployments = Vec::new();

    for os_entry in std::fs::read_dir(&boot_root).wrap_err("reading OSTree OS entries")? {
        let os_entry = os_entry?;
        if !os_entry.file_type()?.is_dir() {
            continue;
        }
        for checksum_entry in
            std::fs::read_dir(os_entry.path()).wrap_err("reading OSTree boot checksums")?
        {
            let checksum_entry = checksum_entry?;
            if !checksum_entry.file_type()?.is_dir() {
                continue;
            }
            for slot_entry in
                std::fs::read_dir(checksum_entry.path()).wrap_err("reading OSTree boot slots")?
            {
                let slot_entry = slot_entry?;
                let deployment = std::fs::canonicalize(slot_entry.path())
                    .wrap_err("resolving OSTree deployment link")?;
                if deployment.starts_with(&canonical_sysroot)
                    && deployment.join(".ostree.cfs").is_file()
                    && !deployments.contains(&deployment)
                {
                    deployments.push(deployment);
                }
            }
        }
    }

    match deployments.as_slice() {
        [deployment] => Ok(deployment.clone()),
        [] => bail!(
            "OSTree sysroot detected at {}, but no active boot deployment was found",
            sysroot.display()
        ),
        _ => bail!(
            "OSTree sysroot at {} has {} active boot deployments; refusing to configure an ambiguous target",
            sysroot.display(),
            deployments.len()
        ),
    }
}

#[enum_dispatch(Module)]
pub trait PostInstallModule {
    #[must_use]
    fn name(&self) -> &'static str;
    fn run(&self, context: &Context) -> Result<()>;
}

#[enum_dispatch]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "module")]
pub enum Module {
    SELinux,
    Dracut,
    ReinstallKernel,
    GRUB2,
    CleanupBoot,
    PrepareFedora,
    EfiStub,
    InitialSetup,
    Keyboard,
    Language,
    CryptSetup,
    Script,
    Fstab,
}

#[cfg(test)]
mod tests {
    use super::{
        Context, PostInstallModule, initial_setup::InitialSetup, keyboard::Keyboard,
        language::Language, resolve_target_root,
    };
    use crate::backend::mounts::Mounts;
    use std::os::unix::fs::symlink;
    use std::path::PathBuf;

    #[test]
    fn traditional_install_uses_the_chroot_root() {
        let sysroot = tempfile::tempdir().unwrap();
        assert_eq!(resolve_target_root(sysroot.path()).unwrap(), sysroot.path());
    }

    #[test]
    fn bootc_install_uses_the_active_ostree_deployment() {
        let sysroot = tempfile::tempdir().unwrap();
        let ostree = sysroot.path().join("ostree");
        let boot_root = ostree.join("boot.1.1/default/boot-checksum");
        let deployment = ostree.join("deploy/default/deploy/deployment-checksum.0");
        std::fs::create_dir_all(&boot_root).unwrap();
        std::fs::create_dir_all(&deployment).unwrap();
        std::fs::create_dir_all(sysroot.path().join("boot")).unwrap();
        std::fs::write(deployment.join(".ostree.cfs"), "").unwrap();
        symlink("loader.1", sysroot.path().join("boot/loader")).unwrap();
        symlink("boot.1.1", ostree.join("boot.1")).unwrap();
        symlink(
            "../../../deploy/default/deploy/deployment-checksum.0",
            boot_root.join("0"),
        )
        .unwrap();

        assert_eq!(
            resolve_target_root(sysroot.path()).unwrap(),
            deployment.canonicalize().unwrap()
        );
    }

    #[test]
    fn bootc_postinstall_writes_into_the_system_that_will_boot() {
        let sysroot = tempfile::tempdir().unwrap();
        let ostree = sysroot.path().join("ostree");
        let boot_root = ostree.join("boot.1.1/default/boot-checksum");
        let deployment = ostree.join("deploy/default/deploy/deployment-checksum.0");
        std::fs::create_dir_all(&boot_root).unwrap();
        std::fs::create_dir_all(&deployment).unwrap();
        std::fs::create_dir_all(sysroot.path().join("boot")).unwrap();
        std::fs::write(deployment.join(".ostree.cfs"), "").unwrap();
        symlink("loader.1", sysroot.path().join("boot/loader")).unwrap();
        symlink("boot.1.1", ostree.join("boot.1")).unwrap();
        symlink(
            "../../../deploy/default/deploy/deployment-checksum.0",
            boot_root.join("0"),
        )
        .unwrap();

        let context = Context {
            destination_disk: PathBuf::from("/dev/test"),
            uefi: true,
            root: resolve_target_root(sysroot.path()).unwrap(),
            mounts: Mounts(Vec::new()),
        };
        Language {
            lang: "pt_BR".into(),
        }
        .run(&context)
        .unwrap();
        Keyboard {
            layout: "br".into(),
            variant: Some("nodeadkeys".into()),
        }
        .run(&context)
        .unwrap();
        InitialSetup.run(&context).unwrap();

        assert_eq!(
            std::fs::read_to_string(deployment.join("etc/locale.conf")).unwrap(),
            "LANG=pt_BR.UTF-8\nLANGUAGE=pt_BR\nLC_MESSAGES=pt_BR.UTF-8\n"
        );
        assert!(
            deployment
                .join("etc/X11/xorg.conf.d/00-keyboard.conf")
                .is_file()
        );
        assert!(deployment.join(".unconfigured").is_file());
        assert!(!sysroot.path().join("etc/locale.conf").exists());
        assert!(!sysroot.path().join(".unconfigured").exists());
    }
}
