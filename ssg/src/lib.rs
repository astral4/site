mod builder;
mod config;
mod css;
mod frontmatter;
mod highlight;
mod image;
mod latex;

pub use builder::{PageBuilder, PageKind};
pub use config::{Config, Fragment};
pub use css::{transform_css, CssOutput, Font};
pub use frontmatter::Frontmatter;
pub use highlight::SyntaxHighlighter;
pub use image::{convert_image, validate_image_src, ActiveImageState};
pub use latex::{LatexConverter, RenderMode};

pub use common::OUTPUT_FONTS_DIR;

pub const OUTPUT_CSS_DIR: &str = "stylesheets/";
pub const OUTPUT_SITE_CSS_FILE: &str = "stylesheets/site.css";
const OUTPUT_SITE_CSS_FILE_ABSOLUTE: &str = "/stylesheets/site.css";
const OUTPUT_KATEX_CSS_FILE: &str = "stylesheets/katex.css";
pub const OUTPUT_CONTENT_DIR: &str = "writing/";

const KATEX_CSS: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../katex/katex.css"));
const KATEX_FONTS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../katex/fonts/");

use anyhow::{Context, Result};
use include_dir::{include_dir, Dir};
use std::{fs::write, path::Path};

/// Saves the KaTeX CSS and font files for math markup to the output directory.
///
/// # Errors
/// This function returns an error if files cannot be written to the destination.
pub fn save_math_assets(output_dir: &Path) -> Result<()> {
    write(output_dir.join(OUTPUT_KATEX_CSS_FILE), KATEX_CSS)
        .context("failed to write KaTeX CSS to output destination")?;

    KATEX_FONTS
        .extract(output_dir.join(OUTPUT_FONTS_DIR))
        .context("failed to write KaTeX fonts to output destination")?;

    Ok(())
}
