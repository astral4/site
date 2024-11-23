use anyhow::{anyhow, Context, Result};
use foldhash::{HashSet, HashSetExt};
use pulldown_cmark::{CodeBlockKind, Event, Tag, TagEnd};
use ssg::{
    parse_markdown, process_image, Config, Frontmatter, LatexConverter, RenderMode,
    SyntaxHighlighter,
};
use std::fs::{create_dir_all, read_dir, read_to_string};

const OUTPUT_CONTENT_DIR: &str = "writing/";

fn main() -> Result<()> {
    let config = Config::from_env().context("failed to read configuration file")?;

    let mut slug_tracker = HashSet::new();

    let syntax_highlighter = SyntaxHighlighter::new();

    let latex_converter =
        LatexConverter::new().context("failed to initialize LaTeX-to-HTML converter")?;

    for entry in
        read_dir(config.articles_dir).context("failed to start traversal of all articles")?
    {
        let input_article_dir = entry.context("failed to access article directory")?.path();

        if !input_article_dir.is_dir() {
            continue;
        }

        (|| {
            let article_text = read_to_string(input_article_dir.join("index.md"))
                .context("failed to read article text file")?;

            let article_frontmatter = Frontmatter::from_text(&article_text)
                .context("failed to read article frontmatter")?;

            if !slug_tracker.insert(article_frontmatter.slug.clone()) {
                return Err(anyhow!(
                    "duplicate article slug found: {}",
                    article_frontmatter.slug
                ));
            }

            let output_article_dir = config
                .output_dir
                .join(OUTPUT_CONTENT_DIR)
                .join(&article_frontmatter.slug);

            create_dir_all(&output_article_dir)
                .context("failed to create output article directory")?;

            let mut is_in_code_block = false;
            let mut code_language = None;

            let events = parse_markdown(&article_text)
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
                    Event::Text(text) if is_in_code_block => syntax_highlighter
                        .highlight(&text, code_language.as_deref())
                        .context("failed to highlight text block")
                        .map(|html| Event::InlineHtml(html.into())),
                    Event::Start(Tag::Image {
                        dest_url,
                        title,
                        id,
                        ..
                    }) => process_image(
                        &input_article_dir,
                        &output_article_dir,
                        &dest_url,
                        &title,
                        (!id.is_empty()).then_some(&id),
                    )
                    .context("failed to process image")
                    .map(|html| Event::InlineHtml(html.into())),
                    Event::InlineMath(src) => latex_converter
                        .latex_to_html(&src, RenderMode::Inline)
                        .context("failed to convert LaTeX to HTML")
                        .map(|html| Event::InlineHtml(html.into())),
                    Event::DisplayMath(src) => latex_converter
                        .latex_to_html(&src, RenderMode::Display)
                        .context("failed to convert LaTeX to HTML")
                        .map(|html| Event::InlineHtml(html.into())),
                    _ => Ok(event),
                })
                .collect::<Result<Vec<_>>>()?;

            Ok(())
        })()
        .with_context(|| format!("failed to process article from {input_article_dir:?}"))?;
    }

    Ok(())
}
