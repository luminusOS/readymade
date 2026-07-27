use color_eyre::Result;
use serde::{Deserialize, Serialize};
use std::os::fd::AsRawFd;

use super::{Context, PostInstallModule};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct InitialSetup;

const FS_IMMUTABLE_FL: nix::libc::c_long = 0x0000_0010;
const FS_IOC_GETFLAGS: nix::libc::c_ulong = 0x8008_6601;
const FS_IOC_SETFLAGS: nix::libc::c_ulong = 0x4008_6602;

fn inode_flags(directory: &std::fs::File) -> Result<nix::libc::c_long> {
    let mut flags = 0;
    // SAFETY: `directory` owns a valid fd and the ioctl writes one c_long to
    // the supplied pointer. Linux exposes FS_IOC_GETFLAGS for directories.
    let result = unsafe {
        nix::libc::ioctl(
            directory.as_raw_fd(),
            FS_IOC_GETFLAGS,
            std::ptr::addr_of_mut!(flags),
        )
    };
    if result == -1 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(flags)
}

fn set_inode_flags(directory: &std::fs::File, flags: nix::libc::c_long) -> Result<()> {
    // SAFETY: `directory` owns a valid fd and the ioctl only reads one c_long
    // from the supplied pointer.
    let result = unsafe {
        nix::libc::ioctl(
            directory.as_raw_fd(),
            FS_IOC_SETFLAGS,
            std::ptr::addr_of!(flags),
        )
    };
    if result == -1 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

/// Create a first-boot marker in an OSTree deployment root.
///
/// OSTree protects the deployment directory with FS_IMMUTABLE_FL while keeping
/// `/etc` writable. Temporarily clear only that bit, create the marker, and
/// restore the exact original flags even when creation fails.
fn create_first_boot_marker(root: &std::path::Path) -> Result<()> {
    let marker = root.join(".unconfigured");
    let create_error = match std::fs::File::create(&marker) {
        Ok(_) => return Ok(()),
        Err(error) => error,
    };

    let directory = std::fs::File::open(root)?;
    let flags = inode_flags(&directory)?;
    let was_immutable = flags & FS_IMMUTABLE_FL != 0;

    if !was_immutable {
        return Err(create_error.into());
    }

    set_inode_flags(&directory, flags & !FS_IMMUTABLE_FL)?;

    let create_result = std::fs::File::create(marker);
    let restore_result = set_inode_flags(&directory, flags);
    restore_result?;
    create_result?;
    Ok(())
}

impl PostInstallModule for InitialSetup {
    fn name(&self) -> &'static str {
        "InitialSetup"
    }

    fn run(&self, context: &Context) -> Result<()> {
        // This triggers whatever the heck (e.g. Taidan) during next boot
        create_first_boot_marker(&context.root)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::create_first_boot_marker;

    #[test]
    fn creates_marker_in_a_regular_root() {
        let root = tempfile::tempdir().unwrap();
        create_first_boot_marker(root.path()).unwrap();
        assert!(root.path().join(".unconfigured").is_file());
    }
}
