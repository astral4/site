use anyhow::{Context, Result};
use regex::Regex;
use reqwest::Client;
use std::time::Duration;

const JS_URL: &str = "https://cdn.jsdelivr.net/npm/katex/dist/katex.min.js";

#[tokio::main]
async fn main() -> Result<()> {
    let client = Client::builder()
        .https_only(true)
        .timeout(Duration::from_secs(15))
        .use_rustls_tls()
        .build()
        .context("failed to build HTTP client")?;

    let version_matcher = Regex::new(r#"version:"([0-9]+?\.[0-9]+?\.[0-9]+?)""#).unwrap();

    let js_source = client
        .get(JS_URL)
        .send()
        .await
        .context("failed to fetch KaTeX JS source")?
        .text()
        .await
        .context("failed to convert JS fetch response to text")?;

    let version = version_matcher
        .captures(&js_source)
        .unwrap()
        .extract::<1>()
        .1[0];

    Ok(())
}
