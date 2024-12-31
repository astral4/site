use aho_corasick::AhoCorasick;
use anyhow::{Context, Result};
use camino::Utf8Path;
use common::OUTPUT_FONTS_DIR_ABSOLUTE;
use regex::Regex;
use reqwest::Client;
use std::{
    fs::{create_dir_all, write},
    path::Path,
    sync::Arc,
    time::Duration,
};
use tokio::task::JoinSet;

const JS_URL: &str = "https://cdn.jsdelivr.net/npm/katex/dist/katex.min.js";
const KATEX_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../katex/");

#[tokio::main]
async fn main() -> Result<()> {
    // Build regexes
    let version_matcher = Regex::new(r#"version:"(.+?)""#).unwrap();
    let top_font_matcher =
        Regex::new(r"(src:url\(.+?\) format\(.+?\))(,url\(.+?\) format\(.+?\))+").unwrap();
    let font_url_matcher = Regex::new(r"url\((.+?)\) format\(.+?\)").unwrap();

    // Initialize HTTP client
    let client = Client::builder()
        .https_only(true)
        .timeout(Duration::from_secs(15))
        .use_rustls_tls()
        .build()
        .context("failed to build HTTP client")?;

    // Fetch latest version of KaTeX JS source
    let js_source = client
        .get(JS_URL)
        .send()
        .await
        .context("failed to fetch KaTeX JS")?
        .text()
        .await
        .context("failed to convert KaTeX JS fetch response to text")?;

    // Extract latest version number
    let version = version_matcher
        .captures(&js_source)
        .unwrap()
        .extract::<1>()
        .1[0];

    // Save KaTeX JS source and version number
    write(Path::new(KATEX_DIR).join("katex.js"), &js_source).context("failed to save KaTeX JS")?;

    write(Path::new(KATEX_DIR).join("version.txt"), version)
        .context("failed to save KaTeX version")?;

    // Construct permalink for fetching CSS and font assets
    // We pin the version in case the latest version changes between fetching the JS source and fetching other assets
    let dist_url: Arc<str> = Arc::from(format!(
        "https://cdn.jsdelivr.net/npm/katex@{version}/dist/"
    ));

    // Fetch KaTeX CSS source
    let css_source = client
        .get(format!("{dist_url}katex.min.css"))
        .send()
        .await
        .context("failed to fetch KaTeX CSS")?
        .text()
        .await
        .context("failed to convert KaTeX CSS fetch response to text")?;

    // Only use the "first-choice" format for every font
    // This is for the purpose of only supporting WOFF2; WOFF and TTF don't need to be served
    let css_source = top_font_matcher.replace_all(&css_source, "$1");

    let mut tasks = JoinSet::new();
    let mut font_paths = Vec::new();

    // Get font URLs and concurrently fetch fonts
    for capture in font_url_matcher.captures_iter(&css_source) {
        let font_path = capture.extract::<1>().1[0];

        tasks.spawn(download_font(
            client.clone(),
            dist_url.clone(),
            font_path.to_owned(),
        ));

        font_paths.push(font_path);
    }

    // Replace font paths in KaTeX CSS source
    let new_font_paths: Vec<_> = font_paths
        .iter()
        .map(|path| {
            let font_file_name = Utf8Path::new(path)
                .file_name()
                .expect("font path should have a file name");

            Utf8Path::new(OUTPUT_FONTS_DIR_ABSOLUTE).join(font_file_name)
        })
        .collect();

    let css_source = AhoCorasick::new(font_paths)
        .expect("automaton construction should succeed")
        .replace_all(&css_source, &new_font_paths);

    // Save KaTeX CSS source
    write(Path::new(KATEX_DIR).join("katex.css"), css_source)
        .context("failed to save KaTeX CSS")?;

    // Wait for all concurrent tasks to finish
    while let Some(result) = tasks.join_next().await {
        result
            .expect("task should not panic or abort")
            .context("failed to download KaTeX font")?;
    }

    Ok(())
}

async fn download_font(client: Client, base_url: Arc<str>, font_path: String) -> Result<()> {
    let font_url = format!("{base_url}{font_path}");

    // Fetch KaTeX font
    let font = client
        .get(&font_url)
        .send()
        .await
        .with_context(|| format!("failed to fetch KaTeX font at {font_url}"))?
        .bytes()
        .await
        .with_context(|| {
            format!("failed to convert KaTeX font fetch response to binary ({font_url})")
        })?;

    // Save KaTeX font
    let target_path = Path::new(KATEX_DIR).join(&font_path);

    create_dir_all(target_path.parent().unwrap())
        .context("failed to create KaTeX font directory")?;

    write(target_path, font).with_context(|| format!("failed to save KaTeX font ({font_path})"))?;

    Ok(())
}
