use anyhow::{anyhow, Context, Result};
use foldhash::{HashMap, HashMapExt, HashSet, HashSetExt};
use glob::glob;
use pulldown_cmark::{
    html::push_html, CodeBlockKind, Event, Options, Parser, Tag, TagEnd, TextMergeWithOffset,
};
use same_file::Handle;
use ssg::{
    process_image, save_math_assets, transform_css, Config, CssOutput, Fragment, Frontmatter,
    LatexConverter, PageBuilder, PageKind, RenderMode, SyntaxHighlighter, OUTPUT_CONTENT_DIR,
    OUTPUT_CSS_DIR, OUTPUT_FONTS_DIR, OUTPUT_SITE_CSS_FILE,
};
use std::{
    collections::hash_map::Entry,
    fs::{create_dir, create_dir_all, read_to_string, write},
    ops::Range,
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
    let mut article_slugs = HashSet::new();

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
            if !article_slugs.insert(article_frontmatter.slug.clone()) {
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
    let mut events = Vec::new();

    // Check for duplicate image links to avoid redundant image processing
    let mut image_links: HashMap<_, String> = HashMap::new();
    // Track image parsing state to support image alt text
    let mut active_image_state: Option<ActiveImageState> = None;
    // Track code block parsing state to support syntax highlighting
    let mut is_in_code_block = false;
    let mut code_language = None;
    // Record existence of math markup to support KaTeX formatting
    let mut contains_math = false;

    for (event, offset) in TextMergeWithOffset::new(
        Parser::new_ext(
            markdown,
            Options::ENABLE_STRIKETHROUGH
                | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
                | Options::ENABLE_MATH,
        )
        .into_offset_iter(),
    ) {
        if let Some(state) = active_image_state.as_mut() {
            if matches!(event, Event::Start(Tag::Image { .. })) {
                state.nest();
            }
            if matches!(event, Event::End(TagEnd::Image)) {
                state.unnest();
            }
            if state.is_active() {
                state.update_alt_text_range(offset);
                continue;
            }
        }

        events.push(match event {
            Event::Start(Tag::CodeBlock(ref kind)) => {
                is_in_code_block = true;
                code_language = match kind {
                    CodeBlockKind::Indented => None,
                    CodeBlockKind::Fenced(lang) => (!lang.is_empty()).then(|| lang.clone()),
                };
                event
            }
            Event::End(TagEnd::CodeBlock) => {
                is_in_code_block = false;
                event
            }
            Event::Text(text) if is_in_code_block => syntax_highlighter
                .highlight(&text, code_language.as_deref())
                .context("failed to highlight text block")
                .map(|html| Event::InlineHtml(html.into()))?,
            Event::Start(Tag::Image {
                dest_url,
                title,
                id,
                ..
            }) => {
                active_image_state
                    .get_or_insert_with(|| ActiveImageState::new(&dest_url, &title, &id));
                continue;
            }
            Event::End(TagEnd::Image) => {
                let active_image = active_image_state.unwrap();
                let input_handle = active_image.input_path_handle(input_dir)?;

                let html = match image_links.entry(input_handle) {
                    Entry::Occupied(entry) => entry.get().clone(),
                    Entry::Vacant(entry) => {
                        let html = active_image
                            .transform_image(input_dir, output_dir, markdown)
                            .context("failed to process image")?;
                        entry.insert(html.clone());
                        html
                    }
                };

                active_image_state = None;

                Event::InlineHtml(html.into())
            }
            Event::InlineMath(src) => {
                contains_math = true;
                latex_converter
                    .latex_to_html(&src, RenderMode::Inline)
                    .context("failed to convert LaTeX to HTML")
                    .map(|html| Event::InlineHtml(html.into()))?
            }
            Event::DisplayMath(src) => {
                contains_math = true;
                latex_converter
                    .latex_to_html(&src, RenderMode::Display)
                    .context("failed to convert LaTeX to HTML")
                    .map(|html| Event::InlineHtml(html.into()))?
            }
            _ => event,
        });
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

struct ActiveImageState {
    nesting_level: usize,
    url: Box<str>,
    title: Box<str>,
    id: Box<str>,
    alt_text_range: Range<usize>,
}

impl ActiveImageState {
    const INIT_NESTING_LEVEL: usize = 1;

    fn new(url: &str, title: &str, id: &str) -> Self {
        Self {
            nesting_level: Self::INIT_NESTING_LEVEL,
            url: url.into(),
            title: title.into(),
            id: id.into(),
            alt_text_range: Range {
                start: usize::MAX,
                end: usize::MIN,
            },
        }
    }

    fn nest(&mut self) {
        self.nesting_level += 1;
    }

    fn unnest(&mut self) {
        self.nesting_level -= 1;
    }

    fn is_active(&self) -> bool {
        self.nesting_level >= Self::INIT_NESTING_LEVEL
    }

    fn update_alt_text_range(&mut self, range: Range<usize>) {
        let Range { start, end } = range;
        if start < self.alt_text_range.start {
            self.alt_text_range.start = start;
        }
        if end > self.alt_text_range.end {
            self.alt_text_range.end = end;
        }
    }

    fn input_path_handle(&self, input_dir: &Path) -> Result<Handle> {
        let image_path = input_dir.join(&*self.url);

        Handle::from_path(&image_path)
            .with_context(|| format!("failed to open file at {image_path:?}"))
    }

    fn transform_image(
        self,
        input_dir: &Path,
        output_dir: &Path,
        article_src: &str,
    ) -> Result<String> {
        debug_assert_eq!(self.nesting_level, Self::INIT_NESTING_LEVEL - 1);

        let alt_text = &article_src[self.alt_text_range];

        process_image(
            input_dir,
            output_dir,
            &self.url,
            alt_text,
            &self.title,
            &self.id,
        )
    }
}
