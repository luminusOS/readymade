use color_eyre::Result;
use serde::{Deserialize, Serialize};

use super::{Context, PostInstallModule};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Language {
    pub lang: String,
}

impl PostInstallModule for Language {
    fn name(&self) -> &'static str {
        "Language"
    }

    fn run(&self, _context: &Context) -> Result<()> {
        // `LOCALE.CONF(5)`: /etc/locale.conf
        std::fs::create_dir_all("/etc")?;
        std::fs::write("/etc/locale.conf", locale_conf(&self.lang))?;
        Ok(())
    }
}

fn locale_conf(language: &str) -> String {
    let language = language
        .strip_suffix(".UTF-8")
        .or_else(|| language.strip_suffix(".utf8"))
        .unwrap_or(language);
    let locale = format!("{language}.UTF-8");
    format!("LANG={locale}\nLANGUAGE={language}\nLC_MESSAGES={locale}\n")
}

#[cfg(test)]
mod tests {
    use super::locale_conf;

    #[test]
    fn writes_a_utf8_locale_and_gettext_language() {
        assert_eq!(
            locale_conf("pt_BR"),
            "LANG=pt_BR.UTF-8\nLANGUAGE=pt_BR\nLC_MESSAGES=pt_BR.UTF-8\n"
        );
    }

    #[test]
    fn does_not_duplicate_an_existing_utf8_suffix() {
        assert_eq!(
            locale_conf("en_US.UTF-8"),
            "LANG=en_US.UTF-8\nLANGUAGE=en_US\nLC_MESSAGES=en_US.UTF-8\n"
        );
    }
}
