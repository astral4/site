use anyhow::{anyhow, Context, Result};
use std::{
    fs::{read_dir, read_to_string},
    path::PathBuf,
};
use tap::Pipe;

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

        let article_text = article_dir_path
            .join("index.md")
            .pipe(read_to_string)
            .context("failed to read article text file")?;
    }

    Ok(())
}
