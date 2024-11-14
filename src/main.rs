use anyhow::{anyhow, Context, Result};
use walkdir::WalkDir;

fn main() -> Result<()> {
    let content_path = std::env::args()
        .next()
        .ok_or_else(|| anyhow!("file path to content was not provided"))?;

    for entry in WalkDir::new(content_path) {
        let file = entry.context("failed to read content entry")?;
    }

    Ok(())
}
