use anyhow::{Context, Result};
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

async fn download_font(client: Client, base_url: Arc<str>, font_path: String) -> Result<()> {
    let font_url = format!("{base_url}{font_path}");

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

    let target_path = Path::new(KATEX_DIR).join(&font_path);

    if let Some(parent) = target_path.parent() {
        create_dir_all(parent).context("failed to create KaTeX font directory")?;
    }

    write(target_path, font).with_context(|| format!("failed to save KaTeX font ({font_path})"))?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let version_matcher = Regex::new(r#"version:"(.+?)""#).unwrap();

    let top_font_matcher =
        Regex::new(r"(src:url\(.+?\) format\(.+?\))(,url\(.+?\) format\(.+?\))+").unwrap();

    let font_url_matcher = Regex::new(r"url\((.+?)\) format\(.+?\)").unwrap();

    let client = Client::builder()
        .https_only(true)
        .timeout(Duration::from_secs(15))
        .use_rustls_tls()
        .build()
        .context("failed to build HTTP client")?;

    let js_source = client
        .get(JS_URL)
        .send()
        .await
        .context("failed to fetch KaTeX JS")?
        .text()
        .await
        .context("failed to convert KaTeX JS fetch response to text")?;

    let version = version_matcher
        .captures(&js_source)
        .unwrap()
        .extract::<1>()
        .1[0];

    write(Path::new(KATEX_DIR).join("katex.js"), &js_source).context("failed to save KaTeX JS")?;

    write(Path::new(KATEX_DIR).join("version.txt"), version)
        .context("failed to save KaTeX version")?;

    let dist_url: Arc<str> = Arc::from(format!(
        "https://cdn.jsdelivr.net/npm/katex@{version}/dist/"
    ));

    let css_source = client
        .get(format!("{dist_url}katex.min.css"))
        .send()
        .await
        .context("failed to fetch KaTeX CSS")?
        .text()
        .await
        .context("failed to convert KaTeX CSS fetch response to text")?;

    let css_source = top_font_matcher.replace_all(&css_source, "$1");

    write(Path::new(KATEX_DIR).join("katex.css"), &*css_source)
        .context("failed to save KaTeX CSS")?;

    let mut tasks = JoinSet::new();

    for capture in font_url_matcher.captures_iter(&css_source) {
        let font_path = capture.extract::<1>().1[0];
        tasks.spawn(download_font(
            client.clone(),
            dist_url.clone(),
            font_path.to_owned(),
        ));
    }

    while let Some(result) = tasks.join_next().await {
        result
            .expect("task should not panic or abort")
            .context("failed to download KaTeX font")?;
    }

    Ok(())
}
