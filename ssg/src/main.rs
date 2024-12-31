use anyhow::{anyhow, Context, Result};
use foldhash::{HashSet, HashSetExt};
use glob::glob;
use pulldown_cmark::{
    html::push_html, CodeBlockKind, Event, Options, Parser, Tag, TagEnd, TextMergeStream,
};
use ssg::{
    process_image, save_math_assets, transform_css, Config, CssOutput, Fragment, Frontmatter,
    LatexConverter, PageBuilder, PageKind, RenderMode, SyntaxHighlighter, OUTPUT_CONTENT_DIR,
    OUTPUT_CSS_DIR, OUTPUT_FONTS_DIR, OUTPUT_SITE_CSS_FILE,
};
use std::{
    fs::{create_dir, create_dir_all, read_to_string, write},
    path::{Path, PathBuf},
};

fn main() -> Result<()> {
    // Read configuration
    let config = Config::from_env().context("failed to read configuration file")?;

    // Create output directories
    create_dir_all(&config.output_dir).context("failed to create output directory")?;
    create_dir(config.output_dir.join(OUTPUT_CSS_DIR))
        .context("failed to create output CSS directory")?;
    create_dir(config.output_dir.join(OUTPUT_FONTS_DIR))
        .context("failed to create output fonts directory")?;
    create_dir(config.output_dir.join(OUTPUT_CONTENT_DIR))
        .context("failed to create output articles directory")?;

    // Process site CSS file
    let CssOutput { css, top_fonts } = read_to_string(&config.site_css_file)
        .context("failed to read site CSS file")
        .and_then(|css| transform_css(&css).context("failed to minify site CSS"))?;

    write(config.output_dir.join(OUTPUT_SITE_CSS_FILE), css)
        .context("failed to write site CSS to output destination")?;

    save_math_assets(&config.output_dir)
        .context("failed to write math CSS to output destination")?;

    // Get site HTML template text
    let template_text = read_to_string(config.template_html_file)
        .context("failed to read site HTML template file")?;

    // Create page builder (template for every page)
    let page_builder = PageBuilder::new(&config.name, &top_fonts, &template_text)
        .context("failed to process site HTML template")?;

    // Process all fragment files
    for fragment in config.fragments {
        process_fragment(&fragment, &config.output_dir, &page_builder)
            .with_context(|| format!("failed to process fragment at {:?}", fragment.path))?;
    }

    // Check for duplicate slugs from articles' frontmatter so every article has a unique output directory
    let mut slug_tracker = HashSet::new();

    // Initialize syntax highlighter for article text
    let syntax_highlighter = SyntaxHighlighter::new();

    // Initialize LaTeX-to-HTML converter for article text
    let latex_converter =
        LatexConverter::new().context("failed to initialize LaTeX-to-HTML converter")?;

    let article_match_pattern: PathBuf = [config.articles_dir.as_str(), "**", "index.md"]
        .iter()
        .collect();

    // Process all articles
    for entry in
        glob(article_match_pattern.to_str().unwrap()).expect("article glob pattern is valid")
    {
        let input_article_dir = {
            let mut entry_path = entry.context("failed to access entry in articles directory")?;
            entry_path.pop();

            if !entry_path.is_dir() {
                continue;
            }

            entry_path
        };

        (|| {
            // Get article text
            let article_text = read_to_string(input_article_dir.join("index.md"))
                .context("failed to read article file")?;

            // Parse frontmatter from article text
            let article_frontmatter = Frontmatter::from_text(&article_text)
                .context("failed to read article frontmatter")?;

            // Check for article slug collisions
            if !slug_tracker.insert(article_frontmatter.slug.clone()) {
                return Err(anyhow!(
                    "duplicate article slug found: {}",
                    article_frontmatter.slug
                ));
            }

            // Create output article directory
            let output_article_dir = config
                .output_dir
                .join(OUTPUT_CONTENT_DIR)
                .join(&*article_frontmatter.slug);

            create_dir(&output_article_dir).with_context(|| {
                format!("failed to create output article directory at {output_article_dir:?}")
            })?;

            // Convert article from Markdown to HTML
            let article_html = build_article(
                &article_text,
                &article_frontmatter,
                &syntax_highlighter,
                &latex_converter,
                &input_article_dir,
                &output_article_dir,
                &page_builder,
            )
            .context("failed to build article HTML")?;

            // Write article HTML to a file in the output article directory
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

fn process_fragment(
    fragment: &Fragment,
    output_dir: &Path,
    page_builder: &PageBuilder,
) -> Result<()> {
    // Get fragment text
    let fragment_text = read_to_string(&fragment.path).context("failed to read fragment file")?;

    // Build complete page from fragment
    let html = page_builder
        .build_page(&fragment.title, &fragment_text, PageKind::Fragment)
        .context("failed to parse fragment as valid HTML")?;

    // Write page HTML to a file in the output directory
    let output_path = output_dir
        .join(fragment.path.file_name().unwrap()) // File stem is guaranteed to be Some(_) from `Config::from_env()`
        .with_extension("html");

    write(&output_path, html)
        .with_context(|| format!("failed to write HTML to {output_path:?}"))?;

    Ok(())
}

fn build_article(
    markdown: &str,
    frontmatter: &Frontmatter,
    syntax_highlighter: &SyntaxHighlighter,
    latex_converter: &LatexConverter,
    input_dir: &Path,
    output_dir: &Path,
    page_builder: &PageBuilder,
) -> Result<String> {
    // Transform Markdown components
    let mut is_in_code_block = false;
    let mut code_language = None;
    let mut contains_math = false;

    let events = TextMergeStream::new(Parser::new_ext(
        markdown,
        Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
            | Options::ENABLE_MATH,
    ))
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
        }) => process_image(input_dir, output_dir, &dest_url, &title, &id)
            .context("failed to process image")
            .map(|html| Event::InlineHtml(html.into())),
        Event::InlineMath(src) => {
            contains_math = true;
            latex_converter
                .latex_to_html(&src, RenderMode::Inline)
                .context("failed to convert LaTeX to HTML")
                .map(|html| Event::InlineHtml(html.into()))
        }
        Event::DisplayMath(src) => {
            contains_math = true;
            latex_converter
                .latex_to_html(&src, RenderMode::Display)
                .context("failed to convert LaTeX to HTML")
                .map(|html| Event::InlineHtml(html.into()))
        }
        _ => Ok(event),
    })
    .collect::<Result<Vec<_>>>()?;

    // Serialize article body to HTML
    let mut article_body = String::with_capacity(markdown.len() * 3 / 2);

    push_html(&mut article_body, events.into_iter());

    // Build complete page
    page_builder
        .build_page(
            &frontmatter.title,
            &article_body,
            PageKind::Article {
                contains_math,
                created: frontmatter.created,
                updated: frontmatter.updated,
            },
        )
        .context("failed to parse processed article body as valid HTML")
}
