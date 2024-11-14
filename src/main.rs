use anyhow::{anyhow, Context, Result};
use std::{fs::read_dir, path::PathBuf};

fn main() -> Result<()> {
    let content_path: PathBuf = std::env::args()
        .next()
        .ok_or_else(|| anyhow!("path to articles was not provided"))?
        .into();

    for article_dir in
        read_dir(content_path).context("failed to start traversal of all articles")?
    {
        let article_dir_path = article_dir
            .context("failed to access article directory")?
            .path();

        let article_path = read_dir(article_dir_path)
            .context("failed to start traversal of article directory")?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| path.is_file() && path.file_name().unwrap() == "index.md")
            .ok_or_else(|| anyhow!("failed to find article text file"))?;
    }

    Ok(())
}
