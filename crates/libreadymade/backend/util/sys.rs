/// Check if the current running system is UEFI or not.
///
/// Simply checks for the existence of the `/sys/firmware/efi` directory.
///
/// False negatives are possible if the system is booted in BIOS mode and the UEFI variables are not exposed.
#[must_use]
pub fn check_uefi() -> bool {
    std::fs::metadata("/sys/firmware/efi").is_ok()
}

/// Ask the kernel and udev to settle partition nodes after repartitioning.
///
/// `systemd-repart` may return before newly-created `/dev/...` partition nodes
/// are immediately usable by following mount operations. Treat refresh failures
/// as warnings here: the repart operation already succeeded, and some
/// environments may not provide every helper command.
pub fn settle_blockdev_partitions(blockdev: &std::path::Path) {
    let Some(blockdev) = blockdev.to_str() else {
        tracing::warn!(?blockdev, "cannot settle non-UTF-8 block device path");
        return;
    };

    run_refresh_command("blockdev", &["--rereadpt", blockdev]);
    run_refresh_command("partx", &["--update", blockdev]);
    run_refresh_command("udevadm", &["settle", "--timeout=10"]);
    std::thread::sleep(std::time::Duration::from_millis(500));
}

fn run_refresh_command(command: &str, args: &[&str]) {
    match std::process::Command::new(command).args(args).status() {
        Ok(status) if status.success() => {}
        Ok(status) => tracing::warn!(?status, command, ?args, "partition refresh command failed"),
        Err(err) => tracing::warn!(?err, command, ?args, "cannot run partition refresh command"),
    }
}
