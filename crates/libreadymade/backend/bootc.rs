use crate::prelude::*;

pub fn target_backed_tmpdir(target_root: &Path) -> Result<PathBuf> {
    let tmpdir = target_root.join("var/tmp");
    std::fs::create_dir_all(&tmpdir)
        .wrap_err_with(|| format!("cannot create bootc temporary directory {tmpdir:?}"))?;
    Ok(tmpdir)
}
