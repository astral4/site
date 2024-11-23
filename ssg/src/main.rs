use anyhow::{anyhow, Context, Result};
use foldhash::{HashSet, HashSetExt};
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd, TextMergeStream};
use ssg::{read_input, Frontmatter, Input, LatexConverter, RenderMode, SyntaxHighlighter};
use std::fs::{read_dir, read_to_string};

fn main() -> Result<()> {
    let Input {
        content_dir,
        output_dir,
    } = read_input()?;

    let markdown_parser_options = Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_MATH;

    let mut slug_tracker = HashSet::new();

    let syntax_highlighter = SyntaxHighlighter::new();

    let latex_converter =
        LatexConverter::new().context("failed to initialize LaTeX-to-HTML conversion engine")?;

    for article_dir in read_dir(content_dir).context("failed to start traversal of all articles")? {
        let article_dir_path = article_dir
            .context("failed to access article directory")?
            .path();

        let article_text = read_to_string(article_dir_path.join("index.md"))
            .context("failed to read article text file")?;

        let article_frontmatter =
            Frontmatter::from_text(&article_text).context("failed to read article frontmatter")?;

        if !slug_tracker.insert(article_frontmatter.slug.clone()) {
            return Err(anyhow!(
                "duplicate article slugs found: {}",
                article_frontmatter.slug
            ));
        }

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
                            .context("failed to highlight text block")
                            .map(|text| Event::InlineHtml(text.into()))
                    } else {
                        Ok(event)
                    }
                }
                Event::InlineMath(src) => latex_converter
                    .latex_to_html(&src, RenderMode::Inline)
                    .context("failed to convert LaTeX to HTML")
                    .map(|text| Event::InlineHtml(text.into())),
                Event::DisplayMath(src) => latex_converter
                    .latex_to_html(&src, RenderMode::Display)
                    .context("failed to convert LaTeX to HTML")
                    .map(|text| Event::InlineHtml(text.into())),
                _ => Ok(event),
            });

        if is_in_code_block {
            return Err(anyhow!("found unclosed code block in article"));
        }
    }

    Ok(())
}
