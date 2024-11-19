use anyhow::{Context, Result};
use reqwest::Client;
use std::time::Duration;

const BASE_URL: &str = "https://cdn.jsdelivr.net/npm/katex/dist/";

#[tokio::main]
async fn main() -> Result<()> {
    let client = Client::builder()
        .https_only(true)
        .timeout(Duration::from_secs(15))
        .use_rustls_tls()
        .build()
        .context("failed to build HTTP client")?;

    todo!()
}
