use anyhow::{anyhow, Context, Result};
use pulldown_cmark::{Event, Options, Parser, TextMergeStream};
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

    let markdown_parser_options: Options = Options::ENABLE_MATH
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS;

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

        let article_parser = TextMergeStream::new(Parser::new_with_broken_link_callback(
            &article_text,
            markdown_parser_options,
            Some(|_| None), // TODO: resolve "broken" links such as inter-article links
        ))
        .map(|event| match event {
            Event::DisplayMath(raw) | Event::InlineMath(raw) => {
                todo!()
            }
            _ => event,
        });
    }

    Ok(())
}
