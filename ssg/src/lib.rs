mod builder;
mod config;
mod css;
mod frontmatter;
mod highlight;
mod image;
mod latex;

pub use builder::PageBuilder;
pub use config::Config;
pub use css::{transform_css, CssOutput, Font};
pub use frontmatter::Frontmatter;
pub use highlight::SyntaxHighlighter;
pub use image::process_image;
pub use latex::{LatexConverter, RenderMode};

use pulldown_cmark::{Event, Options, Parser, TextMergeStream};

pub const OUTPUT_CSS_DIRECTORY: &str = "/stylesheets/";
pub const OUTPUT_SITE_CSS_FILE: &str = "/stylesheets/site.css";

/// Parses the input string as Markdown, returning an iterator of Markdown parsing events.
/// The parser recognizes the following extensions to the CommonMark standard:
/// - strikethroughs
/// - YAML-style frontmatter
/// - math markup
pub fn parse_markdown(text: &str) -> impl Iterator<Item = Event<'_>> {
    TextMergeStream::new(Parser::new_ext(
        text,
        Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
            | Options::ENABLE_MATH,
    ))
}
