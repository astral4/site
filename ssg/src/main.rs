use anyhow::{bail, Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use foldhash::{HashMap, HashMapExt, HashSet, HashSetExt};
use glob::glob;
use pulldown_cmark::{
    html::push_html, CodeBlockKind, CowStr, Event, Options, Parser, Tag, TagEnd,
    TextMergeWithOffset,
};
use same_file::Handle;
use ssg::{
    convert_image, save_math_assets, transform_css, validate_image_src, ActiveImageState,
    ArchiveBuilder, Config, CssOutput, Frontmatter, LatexConverter, PageBuilder, PageKind,
    RenderMode, SyntaxHighlighter, OUTPUT_CONTENT_DIR, OUTPUT_CSS_DIR, OUTPUT_FONTS_DIR,
    OUTPUT_IMAGE_EXTENSION, OUTPUT_SITE_CSS_FILE,
};
use std::{
    collections::hash_map::Entry,
    fs::{copy, create_dir, create_dir_all, read_to_string, write},
    path::Path,
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
    let CssOutput {
        css,
        font_css,
        top_fonts,
    } = read_to_string(&config.site_css_file)
        .context("failed to read site CSS file")
        .and_then(|css| transform_css(&css).context("failed to minify site CSS"))?;

    write(config.output_dir.join(OUTPUT_SITE_CSS_FILE), css)
        .context("failed to write site CSS to output destination")?;

    save_math_assets(&config.output_dir)
        .context("failed to write math CSS to output destination")?;

    // Get site HTML templates
    let head_template_text = read_to_string(config.head_template_html_file)
        .context("failed to read head HTML template file")?;
    let body_template_text = read_to_string(config.body_template_html_file)
        .context("failed to read body HTML template file")?;

    // Create page builder (template for every page)
    let page_builder = PageBuilder::new(
        &head_template_text,
        &body_template_text,
        &top_fonts,
        &font_css,
    )
    .context("failed to process HTML templates")?;

    // Check for duplicate fragment file stems so every fragment has a unique output path
    let mut fragment_stems = HashSet::new();

    // Process all fragment files
    for fragment in config.fragments {
        // Get fragment path's stem; determines the output path
        let stem = fragment.path.file_stem().expect(
        "fragment path should include file name if validation in `Config::from_env()` was successful",
        );

        (|| {
            // Check for fragment stem collisions
            if !fragment_stems.insert(stem.to_owned()) {
                bail!("duplicate fragment slug found: {stem:?}");
            }

            // Get fragment text
            let fragment_text =
                read_to_string(&fragment.path).context("failed to read fragment file")?;

            // Build complete page from fragment
            let html = page_builder
                .build_page(&fragment.title, &fragment_text, PageKind::Fragment)
                .context("failed to parse fragment as valid HTML")?;

            // Create output path
            let output_path = if stem == "index" {
                config.output_dir.join("index.html")
            } else {
                let dir = config.output_dir.join(stem);
                create_dir(&dir)
                    .with_context(|| format!("failed to create directory at {dir:?}"))?;
                dir.join("index.html")
            };

            write(&output_path, html)
                .with_context(|| format!("failed to write HTML to {output_path:?}"))?;

            Ok(())
        })()
        .with_context(|| format!("failed to process fragment at {:?}", fragment.path))?;
    }

    // Check for duplicate slugs from articles' frontmatter so every article has a unique output directory
    let mut article_slugs = HashSet::new();

    // Build a page linking to all articles
    let mut archive_builder = ArchiveBuilder::new();

    // Initialize syntax highlighter for article text
    let syntax_highlighter = SyntaxHighlighter::new(&config.code_theme);

    // Initialize LaTeX-to-HTML converter for article text
    let latex_converter =
        LatexConverter::new().context("failed to initialize LaTeX-to-HTML converter")?;

    // Process all articles
    let article_match_pattern: Utf8PathBuf = [config.articles_dir.as_str(), "**", "*.md"]
        .into_iter()
        .collect();

    for entry in glob(article_match_pattern.as_str()).expect("article glob pattern is valid") {
        let entry_path = entry.context("failed to access entry in articles directory")?;

        let input_article_dir = entry_path
            .parent()
            .expect("article file path should have parent");

        if !input_article_dir.is_dir() {
            continue;
        }

        (|| {
            // Get article text
            let article_text =
                read_to_string(&entry_path).context("failed to read article file")?;

            // Parse frontmatter from article text
            let article_frontmatter = Frontmatter::from_text(&article_text)
                .context("failed to read article frontmatter")?;

            // Check for article slug collisions
            if !article_slugs.insert(article_frontmatter.slug.clone()) {
                bail!("duplicate article slug found: {}", article_frontmatter.slug);
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
                input_article_dir,
                &output_article_dir,
                &page_builder,
            )
            .context("failed to build article HTML")?;

            // Write article HTML to a file in the output article directory
            let output_article_path = output_article_dir.join("index.html");
            write(&output_article_path, article_html).with_context(|| {
                format!("failed to write article HTML to {output_article_path:?}")
            })?;

            archive_builder.add_article(
                article_frontmatter.title,
                article_frontmatter.slug,
                article_frontmatter.created,
            );

            Ok(())
        })()
        .with_context(|| format!("failed to process article at {entry_path:?}"))?;
    }

    let archive_html = archive_builder.into_html(&page_builder);
    let output_path = config
        .output_dir
        .join(OUTPUT_CONTENT_DIR)
        .join("index.html");
    write(&output_path, archive_html)
        .with_context(|| format!("failed to write article archive HTML to {output_path:?}"))?;

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
    let mut events = Vec::new();

    // Check for duplicate image links to avoid redundant image processing
    let mut image_links = HashMap::new();
    // Track image parsing state to support image alt text
    let mut active_image_state: Option<ActiveImageState<'_>> = None;
    // Track code block parsing state to support syntax highlighting
    let mut is_in_code_block = false;
    let mut code_language = None;
    // Check for footnote references without definitions (and vice versa) so all footnote links work
    let mut footnote_references = HashSet::new();
    let mut footnote_definitions = HashSet::new();
    // Record existence of math markup to support KaTeX formatting
    let mut contains_math = false;

    for (event, offset) in TextMergeWithOffset::new(
        Parser::new_ext(
            markdown,
            Options::ENABLE_TABLES
                | Options::ENABLE_FOOTNOTES
                | Options::ENABLE_STRIKETHROUGH
                | Options::ENABLE_SMART_PUNCTUATION
                | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
                | Options::ENABLE_MATH,
        )
        .into_offset_iter(),
    ) {
        if let Some(state) = &mut active_image_state {
            match event {
                Event::Start(Tag::Image { .. }) => state.nest(),
                Event::End(TagEnd::Image) => state.unnest(),
                _ => {}
            }

            if state.is_active() {
                state.update_alt_text_range(offset);
            } else {
                // SAFETY: At this point, `active_image_state` is guaranteed to be `Some(_)`.
                let html = unsafe {
                    active_image_state
                        .take()
                        .unwrap_unchecked()
                        .into_html(markdown)
                };
                events.push(html_to_event(html));
            }

            continue;
        }

        events.push(match event {
            Event::Start(Tag::CodeBlock(ref kind)) => {
                is_in_code_block = true;
                code_language = match kind {
                    CodeBlockKind::Indented => None,
                    CodeBlockKind::Fenced(lang) => Some(lang.clone()),
                };
                event
            }
            Event::End(TagEnd::CodeBlock) => {
                is_in_code_block = false;
                event
            }
            Event::Text(text) if is_in_code_block => syntax_highlighter
                .highlight_block(&text, code_language.as_deref())
                .context("failed to highlight code block")
                .map(html_to_event)?,
            Event::Code(text) => syntax_highlighter
                .highlight_segment(&text)
                .context("failed to highlight inline code segment")
                .map(html_to_event)?,
            Event::FootnoteReference(ref id) => {
                footnote_references.insert(id.clone());
                event
            }
            Event::Start(Tag::FootnoteDefinition(ref id)) => {
                if !footnote_definitions.insert(id.clone()) {
                    bail!("found duplicate footnote definition ID: {id}");
                }
                event
            }
            Event::Start(Tag::Image {
                dest_url,
                title,
                id,
                ..
            }) => {
                debug_assert!(active_image_state.is_none());

                validate_image_src(&dest_url).context("image source is invalid")?;

                let input_path = input_dir.join(&*dest_url);
                let input_handle = Handle::from_path(&input_path)
                    .with_context(|| format!("failed to open file at {input_path:?}"))?;

                let new_state = if input_path
                    .extension()
                    .is_some_and(|ext| ext == OUTPUT_IMAGE_EXTENSION || ext == "svg")
                {
                    let output_path = output_dir.join(&*dest_url);
                    copy(&input_path, &output_path)
                        .with_context(|| {
                            format!("failed to copy file from {input_path:?} to {output_path:?}")
                        })
                        .context("failed to process image")?;

                    ActiveImageState::new(dest_url, None, title, id)
                } else {
                    // Check if image has already been processed
                    let dimensions = match image_links.entry(input_handle) {
                        Entry::Occupied(entry) => *entry.get(),
                        Entry::Vacant(entry) => {
                            let dimensions = convert_image(input_dir, output_dir, &dest_url)
                                .context("failed to process image")?;
                            *entry.insert(dimensions)
                        }
                    };

                    let output_path = Utf8Path::new(&dest_url)
                        .with_extension(OUTPUT_IMAGE_EXTENSION)
                        .into_string()
                        .into_boxed_str();

                    ActiveImageState::new(CowStr::Boxed(output_path), Some(dimensions), title, id)
                };

                active_image_state = Some(new_state);

                continue;
            }
            Event::InlineMath(src) => {
                contains_math = true;
                latex_converter
                    .latex_to_html(&src, RenderMode::Inline)
                    .context("failed to convert LaTeX to HTML")
                    .map(html_to_event)?
            }
            Event::DisplayMath(src) => {
                contains_math = true;
                latex_converter
                    .latex_to_html(&src, RenderMode::Display)
                    .context("failed to convert LaTeX to HTML")
                    .map(html_to_event)?
            }
            _ => event,
        });
    }

    // Check for footnote references without definitions
    for id in footnote_references {
        if !footnote_definitions.remove(&id) {
            bail!("found a footnote reference ID without a definition: {id}");
        }
    }

    // Check for footnote definitions without references
    if let Some(id) = footnote_definitions.iter().next() {
        bail!("found a footnote definition ID without references: {id}");
    }

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

fn html_to_event<'a>(html: String) -> Event<'a> {
    Event::InlineHtml(html.into())
}
