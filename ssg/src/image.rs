//! Utility for converting images in articles to AVIF.

use anyhow::{Context, Result};
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

    Ok((width, height))
}
