use anyhow::{anyhow, Context, Result};
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd, TextMergeStream};
use ssg::{LatexConverter, RenderMode, SyntaxHighlighter};
use std::{
    env::args,
    fs::{read_dir, read_to_string},
    path::PathBuf,
};

fn main() -> Result<()> {
    let content_path: PathBuf = args()
        .next()
        .ok_or_else(|| anyhow!("path to articles was not provided"))?
        .into();

    let markdown_parser_options = Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_MATH;

    let syntax_highlighter = SyntaxHighlighter::new();

    let latex_converter =
        LatexConverter::new().context("failed to initialize LaTeX-to-HTML conversion engine")?;

    for article_dir in
        read_dir(content_path).context("failed to start traversal of all articles")?
    {
        let article_dir_path = article_dir
            .context("failed to access article directory")?
            .path();

        let article_text = read_to_string(article_dir_path.join("index.md"))
            .context("failed to read article text file")?;

        let mut is_in_code_block = false;
        let mut code_language = None;

        let parser = TextMergeStream::new(Parser::new_ext(&article_text, markdown_parser_options))
            .map(|event| match event {
                Event::Start(Tag::CodeBlock(ref kind)) => {
                    is_in_code_block = true;
                    code_language = match kind {
                        CodeBlockKind::Indented => None,
                        CodeBlockKind::Fenced(lang) => (!lang.is_empty()).then(|| lang.clone()),
                    };
                    Ok(event)
                }
                Event::End(TagEnd::CodeBlock) => {
                    is_in_code_block = false;
                    Ok(event)
                }
                Event::Text(ref text) => {
                    if is_in_code_block {
                        syntax_highlighter
                            .highlight(text, code_language.as_deref())
                            .map_err(|e| e.context("failed to highlight text block"))
                            .map(Into::into)
                            .map(Event::InlineHtml)
                    } else {
                        Ok(event)
                    }
                }
                Event::InlineMath(src) => latex_converter
                    .latex_to_html(&src, RenderMode::Inline)
                    .map_err(|e| e.context("failed to convert LaTeX to HTML"))
                    .map(Into::into)
                    .map(Event::InlineHtml),
                Event::DisplayMath(src) => latex_converter
                    .latex_to_html(&src, RenderMode::Display)
                    .map_err(|e| e.context("failed to convert LaTeX to HTML"))
                    .map(Into::into)
                    .map(Event::InlineHtml),
                _ => Ok(event),
            });
    }

    Ok(())
}
