//! Utility for converting images in articles to AVIF.

use anyhow::{anyhow, Context, Result};
use camino::{Utf8Component, Utf8Path};
use image::{codecs::avif::AvifEncoder, GenericImageView, ImageEncoder, ImageReader};
use std::{
    fs::{copy, File},
    io::BufWriter,
    path::Path,
};

// In debug builds, we use the fastest encoding speed for the fastest site build times.
// In release builds, we use the slowest encoding speed for the best compression.
#[cfg(debug_assertions)]
const ENCODER_SPEED: u8 = 10;
#[cfg(not(debug_assertions))]
const ENCODER_SPEED: u8 = 1;

/// Processes an image link by converting the linked image to AVIF and saving it to an output path.
/// This function outputs a string containing an HTML <img> element
/// with `src`, `alt`, dimension, and rendering attributes.
/// If the provided `id` is not empty, an `id` attribute is also added.
///
/// # Errors
/// This function returns an error if:
/// - the input image path is empty
/// - the input image path is not normalized or relative
/// - the file at the input image path cannot be opened or read from
/// - the file at the output file path cannot be created or written to
pub fn process_image(
    input_article_dir: &Path,
    output_article_dir: &Path,
    image_path: &str,
    alt_text: &str,
    title: &str,
    id: &str,
) -> Result<String> {
    if image_path.is_empty() {
        return Err(anyhow!("no source provided for image"));
    }

    let image_path = Utf8Path::new(image_path);

    if !image_path.is_relative()
        || image_path
            .components()
            .any(|part| matches!(part, Utf8Component::ParentDir | Utf8Component::Normal("..")))
    {
        return Err(anyhow!(
            "image source is not a normalized relative file path ({image_path})"
        ));
    }

    let input_path = input_article_dir.join(image_path);
    let output_path = output_article_dir.join(image_path).with_extension("avif");

    let image = ImageReader::open(&input_path)
        .with_context(|| format!("failed to open file at {input_path:?}"))?
        .decode()
        .with_context(|| format!("failed to read image from {input_path:?}"))?;

    let (width, height) = image.dimensions();

    // If the input image path ends with ".avif",
    // we assume it is already encoded in AVIF and simply copy it to the output destination.
    if input_path.extension().is_some_and(|ext| ext == "avif") {
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

    let image_src = image_path.with_extension("avif");

    // Build image HTML representation
    let mut html = format!(
        r#"<img src="{image_src}" alt="{alt_text}" width="{width}" height="{height}" decoding="async" loading="lazy""#
    );
    if !title.is_empty() {
        html.push_str(&format!(" title=\"{title}\""));
    }
    if !id.is_empty() {
        html.push_str(&format!(" id=\"{id}\""));
    }
    html.push('>');

    Ok(html)
}
