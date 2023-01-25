use dialoguer::{theme::ColorfulTheme, Confirm};
use eyre::{Result, WrapErr};
use std::fs;
use std::path::Path;

/// Write string to file if new (or prompt to overwrite).
///
/// # Arguments
///
/// * `file` - Path to file
/// * `contents` - String to write.
/// * `force` - If true, overwrite existing file without asking.
pub fn write_string_or_prompt(file: impl AsRef<Path>, contents: &str, force: bool) -> Result<()> {
    let file = file.as_ref();
    if file.is_file() && !force {
        let proceed = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!("File {} exists. Overwrite?", file.display()))
            .interact()?;
        if !proceed {
            return Ok(());
        }
    }
    fs::write(file, contents).wrap_err(format!("Unable to write to file: {}", file.display()))
}
