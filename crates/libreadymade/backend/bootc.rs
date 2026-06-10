use crate::prelude::*;
use std::io::Write;

pub fn target_backed_containers_conf(target_root: &Path) -> Result<tempfile::NamedTempFile> {
    let image_copy_tmp_dir = toml_escape(&target_root.join("var/tmp").to_string_lossy());
    let mut config = tempfile::NamedTempFile::new()?;
    writeln!(
        config,
        "[engine]\nimage_copy_tmp_dir = \"{image_copy_tmp_dir}\""
    )?;
    Ok(config)
}

fn toml_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}
