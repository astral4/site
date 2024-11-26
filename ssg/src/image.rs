//! Utility for converting images in articles to AVIF.

use anyhow::{anyhow, Context, Result};
use image::{codecs::avif::AvifEncoder, GenericImageView, ImageEncoder, ImageReader};
use std::{
    fs::{copy, File},
    io::BufWriter,
    path::Path,
};

/// Processes an image link by converting the linked image to AVIF and saving it to an output path.
/// This function outputs a string containing an HTML <img> element
/// with `src`, `alt`, dimension, and rendering attributes.
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
    id: &str,
) -> Result<String> {
    if image_path.is_empty() {
        return Err(anyhow!("no source provided for image"));
    }
    if !Path::new(image_path).is_relative()
        || Path::new(image_path)
            .components()
            .any(|part| part.as_os_str() == "..")
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
        // We use the slowest encoding speed for the best compression.
        AvifEncoder::new_with_speed_quality(writer, 1, 80)
            .write_image(image.as_bytes(), width, height, image.color().into())
            .with_context(|| format!("failed to write image to {output_path:?}"))?;
    }

    Ok(if id.is_empty() {
        format!("<img src=\"{image_path}\" alt=\"{alt_text}\" width=\"{width}\" height=\"{height}\" decoding=\"async\" loading=\"lazy\">")
    } else {
        format!("<img src=\"{image_path}\" alt=\"{alt_text}\" width=\"{width}\" height=\"{height}\" decoding=\"async\" loading=\"lazy\" id=\"{id}\">")
    })
}
