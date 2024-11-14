use anyhow::{anyhow, Context, Result};
use std::{fs::read_dir, path::PathBuf};

fn main() -> Result<()> {
    let content_path: PathBuf = std::env::args()
        .next()
        .ok_or_else(|| anyhow!("path to content was not provided"))?
        .into();

    for article_dir in
        read_dir(content_path).context("failed to start traversal of content at supplied path")?
    {
        let article_dir_path = article_dir
            .context("failed to access content entry")?
            .path();
    }

    Ok(())
}
