use anyhow::{Context, Result};
use regex::Regex;
use reqwest::Client;
use std::{fs::write, path::Path, time::Duration};

const JS_URL: &str = "https://cdn.jsdelivr.net/npm/katex/dist/katex.min.js";
const KATEX_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../katex/");

#[tokio::main]
async fn main() -> Result<()> {
    let version_matcher = Regex::new(r#"version:"(.+?)""#).unwrap();

    let top_font_matcher =
        Regex::new(r"(src:url\(.+?\) format\(.+?\))(,url\(.+?\) format\(.+?\))+").unwrap();

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

    let dist_url = format!("https://cdn.jsdelivr.net/npm/katex@{version}/dist/");

    let css_source = client
        .get(format!("{dist_url}katex.min.css"))
        .send()
        .await
        .context("failed to fetch KaTeX CSS")?
        .text()
        .await
        .context("failed to convert KaTeX CSS fetch response to text")?;

    let css_source = top_font_matcher.replace_all(&css_source, "$1");

    Ok(())
}
