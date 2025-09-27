//! Utility for converting images in articles to AVIF.

use crate::builder::create_img_html;
use anyhow::{bail, Context, Result};
use camino::{Utf8Component, Utf8Path};
use image::{codecs::avif::AvifEncoder, GenericImageView, ImageEncoder, ImageReader};
use pulldown_cmark::CowStr;
use std::{fs::File, io::BufWriter, ops::Range};

pub const OUTPUT_IMAGE_EXTENSION: &str = "avif";

// In debug builds, we use the fastest encoding speed for the fastest site build times.
// In release builds, we use the slowest encoding speed for the best compression.
#[cfg(debug_assertions)]
const ENCODER_SPEED: u8 = 10;
#[cfg(not(debug_assertions))]
const ENCODER_SPEED: u8 = 1;

pub struct ActiveImageState<'a> {
    nesting_level: usize,
    url: CowStr<'a>,
    dimensions: Option<Dimensions>,
    title: CowStr<'a>,
    id: CowStr<'a>,
    alt_text_range: Range<usize>,
}

impl<'a> ActiveImageState<'a> {
    const INITIAL_NESTING_LEVEL: usize = 1;
    const INITIAL_START_INDEX: usize = usize::MAX;
    const INITIAL_END_INDEX: usize = usize::MIN;

    /// Creates a context for tracking the character range of an image's alt text within a Markdown source.
    #[must_use]
    pub fn new(
        url: CowStr<'a>,
        dimensions: Option<Dimensions>,
        title: CowStr<'a>,
        id: CowStr<'a>,
    ) -> Self {
        Self {
            nesting_level: Self::INITIAL_NESTING_LEVEL,
            url,
            dimensions,
            title,
            id,
            alt_text_range: Range {
                start: Self::INITIAL_START_INDEX,
                end: Self::INITIAL_END_INDEX,
            },
        }
    }

    /// Increments the nesting level.
    /// This is used when the start of an image element is encountered within the context.
    pub fn nest(&mut self) {
        debug_assert_ne!(self.nesting_level, usize::MAX);
        self.nesting_level += 1;
    }

    /// Decrements the nesting level.
    /// This is used when the end of an image element is encountered within the context.
    pub fn unnest(&mut self) {
        debug_assert_ne!(self.nesting_level, usize::MIN);
        self.nesting_level -= 1;
    }

    /// Returns a Boolean indicating if the context has ended.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.nesting_level >= Self::INITIAL_NESTING_LEVEL
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
        debug_assert_eq!(self.nesting_level, Self::INITIAL_NESTING_LEVEL - 1);

        let alt_text = if self.alt_text_range.start == Self::INITIAL_START_INDEX
            || self.alt_text_range.end == Self::INITIAL_END_INDEX
        {
            // self.update_alt_text_range() was never called, so the image has no alt text
            ""
        } else {
            &markdown_source[self.alt_text_range]
        };

        let dimension_strs = self
            .dimensions
            .map(|Dimensions { width, height }| (width.to_string(), height.to_string()));

        // Build image HTML representation
        let mut attrs = Vec::with_capacity(8);
        attrs.push(("src", self.url.as_ref()));
        attrs.push(("alt", alt_text));
        // Asynchronous image decoding improves the rendering performance of other elements.
        // https://www.tunetheweb.com/blog/what-does-the-image-decoding-attribute-actually-do/
        attrs.push(("decoding", "async"));
        attrs.push(("loading", "lazy"));

        if let Some((width_str, height_str)) = &dimension_strs {
            attrs.push(("width", width_str));
            attrs.push(("height", height_str));
        }
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
    input_article_dir: &Utf8Path,
    output_article_dir: &Utf8Path,
    image_path: &str,
) -> Result<Dimensions> {
    let input_path = input_article_dir.join(image_path);
    let output_path = output_article_dir
        .join(image_path)
        .with_extension(OUTPUT_IMAGE_EXTENSION);

    let image = ImageReader::open(&input_path)
        .with_context(|| format!("failed to open file at {input_path}"))?
        .decode()
        .with_context(|| format!("failed to read image from {input_path}"))?;

    let (width, height) = image.dimensions();

    let writer = BufWriter::new(
        File::create(&output_path)
            .with_context(|| format!("failed to create file at {output_path}"))?,
    );

    AvifEncoder::new_with_speed_quality(writer, ENCODER_SPEED, 80)
        .write_image(image.as_bytes(), width, height, image.color().into())
        .with_context(|| format!("failed to write image to {output_path}"))?;

    Ok(Dimensions { width, height })
}

#[derive(Clone, Copy)]
pub struct Dimensions {
    width: u32,
    height: u32,
}
