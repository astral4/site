//! Utility for converting images in articles to AVIF.

use crate::builder::create_img_html;
use anyhow::{bail, Context, Result};
use camino::{Utf8Component, Utf8Path};
use image::{codecs::avif::AvifEncoder, GenericImageView, ImageEncoder, ImageReader};
use std::{
    fs::{copy, File},
    io::BufWriter,
    ops::Range,
    path::Path,
};

const OUTPUT_FORMAT_EXTENSION: &str = "avif";

// In debug builds, we use the fastest encoding speed for the fastest site build times.
// In release builds, we use the slowest encoding speed for the best compression.
#[cfg(debug_assertions)]
const ENCODER_SPEED: u8 = 10;
#[cfg(not(debug_assertions))]
const ENCODER_SPEED: u8 = 1;

pub struct ActiveImageState {
    nesting_level: usize,
    url: Box<str>,
    width: u32,
    height: u32,
    title: Box<str>,
    id: Box<str>,
    alt_text_range: Range<usize>,
}

impl ActiveImageState {
    const INIT_NESTING_LEVEL: usize = 1;

    /// Creates a context for tracking the character range of an image's alt text within a Markdown source.
    #[must_use]
    pub fn new(url: &str, dimensions: (u32, u32), title: &str, id: &str) -> Self {
        let (width, height) = dimensions;
        Self {
            nesting_level: Self::INIT_NESTING_LEVEL,
            url: url.into(),
            width,
            height,
            title: title.into(),
            id: id.into(),
            alt_text_range: Range {
                start: usize::MAX,
                end: usize::MIN,
            },
        }
    }

    /// Increments the nesting level.
    /// This is used when the start of an image element is encountered within the context.
    pub fn nest(&mut self) {
        self.nesting_level += 1;
    }

    /// Decrements the nesting level.
    /// This is used when the end of an image element is encountered within the context.
    pub fn unnest(&mut self) {
        self.nesting_level -= 1;
    }

    /// Returns a Boolean indicating if the context has ended.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.nesting_level >= Self::INIT_NESTING_LEVEL
    }

    /// Updates the character range of this context's alt text.
    /// This is used when encountering any item within the context;
    /// every item has a character range corresponding to its location in the Markdown text.
    pub fn update_alt_text_range(&mut self, range: Range<usize>) {
        let Range { start, end } = range;
        if start < self.alt_text_range.start {
            self.alt_text_range.start = start;
        }
        if end > self.alt_text_range.end {
            self.alt_text_range.end = end;
        }
    }

    /// Consumes the context, returning a complete `<img>` element as a string of HTML.
    /// The input Markdown source is used for retrieving the image's alt text.
    #[must_use]
    pub fn into_html(self, markdown_source: &str) -> String {
        debug_assert_eq!(self.nesting_level, Self::INIT_NESTING_LEVEL - 1);

        let image_src = Utf8Path::new(&self.url).with_extension(OUTPUT_FORMAT_EXTENSION);
        let alt_text = &markdown_source[self.alt_text_range];
        let (width_str, height_str) = (self.width.to_string(), self.height.to_string());

        // Build image HTML representation
        let mut attrs = Vec::with_capacity(8);
        attrs.push(("src", image_src.as_str()));
        attrs.push(("alt", alt_text));
        attrs.push(("width", &width_str));
        attrs.push(("height", &height_str));
        // Asynchronous image decoding improves the rendering performance of other elements.
        // https://www.tunetheweb.com/blog/what-does-the-image-decoding-attribute-actually-do/
        attrs.push(("decoding", "async"));
        attrs.push(("loading", "lazy"));

        if !self.title.is_empty() {
            attrs.push(("title", &self.title));
        }
        if !self.id.is_empty() {
            attrs.push(("id", &self.id));
        }

        create_img_html(&attrs)
    }
}

/// Validates the input image source.
///
/// # Errors
/// This function returns an error if:
/// - the input source is an empty string
/// - the input source is not a relative path
/// - the input source is a path with parent-referencing components ("..")
pub fn validate_image_src(url: &str) -> Result<()> {
    if url.is_empty() {
        bail!("no source provided for image");
    }

    let url = Utf8Path::new(url);

    if !url.is_relative()
        || url
            .components()
            .any(|part| matches!(part, Utf8Component::ParentDir | Utf8Component::Normal("..")))
    {
        bail!("image source is not a normalized relative file path ({url})");
    }

    Ok(())
}

/// Converts the image at the input path to AVIF and saves it to an output path.
/// This function outputs a (width, height) tuple of the image's dimensions.
///
/// # Errors
/// This function returns an error if:
/// - the file at the input image path cannot be opened or read from
/// - the file at the output file path cannot be created or written to
pub fn convert_image(
    input_article_dir: &Path,
    output_article_dir: &Path,
    image_path: &str,
) -> Result<(u32, u32)> {
    let input_path = input_article_dir.join(image_path);
    let output_path = output_article_dir
        .join(image_path)
        .with_extension(OUTPUT_FORMAT_EXTENSION);

    let image = ImageReader::open(&input_path)
        .with_context(|| format!("failed to open file at {input_path:?}"))?
        .decode()
        .with_context(|| format!("failed to read image from {input_path:?}"))?;

    let (width, height) = image.dimensions();

    // If the input image path ends with ".avif",
    // we assume it is already encoded in AVIF and simply copy it to the output destination.
    if input_path
        .extension()
        .is_some_and(|ext| ext == OUTPUT_FORMAT_EXTENSION)
    {
        copy(&input_path, &output_path).with_context(|| {
            format!("failed to copy file from {input_path:?} to {output_path:?}")
        })?;
    } else {
        let writer = BufWriter::new(
            File::create(&output_path)
                .with_context(|| format!("failed to create file at {output_path:?}"))?,
        );

        AvifEncoder::new_with_speed_quality(writer, ENCODER_SPEED, 80)
            .write_image(image.as_bytes(), width, height, image.color().into())
            .with_context(|| format!("failed to write image to {output_path:?}"))?;
    }

    Ok((width, height))
}
