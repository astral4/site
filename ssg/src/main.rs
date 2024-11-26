use anyhow::{anyhow, Context, Result};
use foldhash::{HashSet, HashSetExt};
use pulldown_cmark::{html::push_html, CodeBlockKind, Event, Tag, TagEnd};
use ssg::{
    parse_markdown, process_image, transform_css, Config, CssOutput, Frontmatter, LatexConverter,
    PageBuilder, RenderMode, SyntaxHighlighter, OUTPUT_CSS_DIRECTORY, OUTPUT_SITE_CSS_FILE,
};
use std::fs::{create_dir, create_dir_all, read_dir, read_to_string, write};

const OUTPUT_CONTENT_DIR: &str = "writing/";

fn main() -> Result<()> {
    let config = Config::from_env().context("failed to read configuration file")?;

    (|| {
        create_dir_all(&config.output_dir).context("failed to create output directory")?;
        create_dir(config.output_dir.join(OUTPUT_CSS_DIRECTORY))
            .context("failed to create output CSS directory")?;
        create_dir(config.output_dir.join(OUTPUT_CONTENT_DIR))
            .context("failed to create output articles directory")
    })()
    .context("failed to create output directories")?;

    let CssOutput { css, top_fonts } = read_to_string(&config.site_css_file)
        .context("failed to read site CSS file")
        .and_then(|css| transform_css(&css).context("failed to minify site CSS"))?;

    write(config.output_dir.join(OUTPUT_SITE_CSS_FILE), css)
        .context("failed to write site CSS to output ")?;

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

        let mut article_contains_math = false;

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
                .join(&*article_frontmatter.slug);

            create_dir(&output_article_dir).with_context(|| {
                format!("failed to create output article directory at {output_article_dir:?}")
            })?;

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
                        &id,
                    )
                    .context("failed to process image")
                    .map(|html| Event::InlineHtml(html.into())),
                    Event::InlineMath(src) => {
                        article_contains_math = true;
                        latex_converter
                            .latex_to_html(&src, RenderMode::Inline)
                            .context("failed to convert LaTeX to HTML")
                            .map(|html| Event::InlineHtml(html.into()))
                    }
                    Event::DisplayMath(src) => {
                        article_contains_math = true;
                        latex_converter
                            .latex_to_html(&src, RenderMode::Display)
                            .context("failed to convert LaTeX to HTML")
                            .map(|html| Event::InlineHtml(html.into()))
                    }
                    _ => Ok(event),
                })
                .collect::<Result<Vec<_>>>()?;

            let mut article_body = String::with_capacity(article_text.len() * 3 / 2);
            push_html(&mut article_body, events.into_iter());

            let article_html = PageBuilder::new(&article_body)
                .context("failed to parse processed article body as valid HTML")?
                .build_page(&article_frontmatter.title, &config.name);

            let output_article_path = output_article_dir.join("index.html");
            write(&output_article_path, article_html).with_context(|| {
                format!("failed to write article HTML to {output_article_path:?}")
            })?;

            Ok(())
        })()
        .with_context(|| format!("failed to process article from {input_article_dir:?}"))?;
    }

    Ok(())
}
