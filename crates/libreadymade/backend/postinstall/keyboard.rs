use color_eyre::{Result, eyre::bail};
use serde::{Deserialize, Serialize};

use super::{Context, PostInstallModule};

/// Persist the system-wide XKB source selected by the installer.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Keyboard {
    pub layout: String,
    pub variant: Option<String>,
}

impl PostInstallModule for Keyboard {
    fn name(&self) -> &'static str {
        "Keyboard"
    }

    fn run(&self, _context: &Context) -> Result<()> {
        if !valid_component(&self.layout)
            || self
                .variant
                .as_deref()
                .is_some_and(|variant| !valid_component(variant))
        {
            bail!("invalid XKB layout or variant");
        }

        let directory = "/etc/X11/xorg.conf.d";
        std::fs::create_dir_all(directory)?;
        std::fs::write(
            format!("{directory}/00-keyboard.conf"),
            xorg_config(&self.layout, self.variant.as_deref()),
        )?;
        Ok(())
    }
}

fn valid_component(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn xorg_config(layout: &str, variant: Option<&str>) -> String {
    let variant = variant.unwrap_or_default();
    format!(
        "Section \"InputClass\"\n\
         \tIdentifier \"system-keyboard\"\n\
         \tMatchIsKeyboard \"on\"\n\
         \tOption \"XkbLayout\" \"{layout}\"\n\
         \tOption \"XkbVariant\" \"{variant}\"\n\
         EndSection\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_layout_and_variant_in_localed_compatible_xorg_format() {
        assert_eq!(
            xorg_config("br", Some("nodeadkeys")),
            "Section \"InputClass\"\n\
             \tIdentifier \"system-keyboard\"\n\
             \tMatchIsKeyboard \"on\"\n\
             \tOption \"XkbLayout\" \"br\"\n\
             \tOption \"XkbVariant\" \"nodeadkeys\"\n\
             EndSection\n"
        );
    }

    #[test]
    fn rejects_values_that_could_escape_the_xorg_configuration() {
        assert!(valid_component("intl"));
        assert!(valid_component("mac_nodeadkeys"));
        assert!(!valid_component("br\"\nEndSection"));
        assert!(!valid_component(""));
    }
}
