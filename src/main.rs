use anyhow::{anyhow, Context, Result};
use pulldown_cmark::{Event, Options, Parser, TextMergeStream};
use ssg::{LatexConverter, RenderMode};
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

    let markdown_parser_options = Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_MATH;

    let latex_converter =
        LatexConverter::new().context("failed to initialize LaTeX-to-HTML conversion engine")?;

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
            Event::InlineMath(src) => latex_converter
                .latex_to_html(&src, RenderMode::Inline)
                .map(Into::into)
                .map(Event::InlineHtml),
            Event::DisplayMath(src) => latex_converter
                .latex_to_html(&src, RenderMode::Display)
                .map(Into::into)
                .map(Event::InlineHtml),
            _ => Ok(event),
        });
    }

    Ok(())
}
