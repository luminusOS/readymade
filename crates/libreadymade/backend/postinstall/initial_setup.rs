use color_eyre::Result;
use serde::{Deserialize, Serialize};

use super::{Context, PostInstallModule};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct InitialSetup;

impl PostInstallModule for InitialSetup {
    fn name(&self) -> &'static str {
        "InitialSetup"
    }

    fn run(&self, context: &Context) -> Result<()> {
        // This triggers whatever the heck (e.g. Taidan) during next boot
        std::fs::File::create(context.root.join(".unconfigured"))?;
        Ok(())
    }
}
