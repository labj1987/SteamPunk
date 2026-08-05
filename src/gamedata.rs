//! gamedata.rs — resolves a game's name and cover art from a Steam AppID,
//! via the public (no-auth) Steam Store API and CDN, caching both locally
//! so a trainer's game info is fetched once at add time, never on every
//! app launch.

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::time::Duration;

fn cache_dir() -> Result<PathBuf> {
    Ok(crate::library::data_dir()?.join("cache"))
}

fn name_cache_path(appid: u32) -> Result<PathBuf> {
    Ok(cache_dir()?.join(format!("{appid}.name.txt")))
}

fn cover_cache_path(appid: u32) -> Result<PathBuf> {
    Ok(cache_dir()?.join(format!("{appid}.jpg")))
}

/// The cached game name, if a fetch has previously succeeded. No network
/// access — a cache miss just means the caller falls back to the trainer's
/// filename-derived title.
pub fn cached_name(appid: u32) -> Option<String> {
    let path = name_cache_path(appid).ok()?;
    let text = std::fs::read_to_string(path).ok()?;
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// The cached cover-art image path, if a fetch has previously succeeded.
pub fn cached_cover(appid: u32) -> Option<PathBuf> {
    let path = cover_cache_path(appid).ok()?;
    path.is_file().then_some(path)
}

/// Fetches the game's name and cover art for `appid` and caches both to
/// disk. Called once, right after a trainer is associated with an AppID —
/// `cached_name`/`cached_cover` serve every later read, including across
/// app restarts, so this never runs again for the same AppID.
///
/// The name fetch failing is a real error (nothing to show). The cover
/// fetch failing is logged but not propagated — a game with a name and no
/// art is still strictly better than falling back to the filename.
pub async fn fetch_and_cache(appid: u32) -> Result<()> {
    let dir = cache_dir()?;
    std::fs::create_dir_all(&dir)?;

    let client = reqwest::Client::builder()
        .user_agent("steampunk (https://github.com/labj1987/steampunk)")
        .timeout(Duration::from_secs(10))
        .build()?;

    let name = fetch_name(&client, appid).await?;
    std::fs::write(name_cache_path(appid)?, &name)
        .with_context(|| format!("caching name for AppID {appid}"))?;

    if let Err(e) = fetch_cover(&client, appid).await {
        crate::applog::log(&format!("gamedata: cover art fetch failed for AppID {appid}: {e}"));
    }

    Ok(())
}

async fn fetch_name(client: &reqwest::Client, appid: u32) -> Result<String> {
    let url = format!("https://store.steampowered.com/api/appdetails?appids={appid}");
    let body: serde_json::Value = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("requesting appdetails for AppID {appid}"))?
        .error_for_status()
        .with_context(|| format!("appdetails returned an error status for AppID {appid}"))?
        .json()
        .await
        .context("parsing appdetails response as JSON")?;

    let name = body
        .get(appid.to_string())
        .and_then(|v| v.get("data"))
        .and_then(|v| v.get("name"))
        .and_then(|v| v.as_str())
        .with_context(|| format!("appdetails response for AppID {appid} has no data.name — is the AppID valid?"))?;
    Ok(name.to_string())
}

/// Preferred: the horizontal library capsule art, matching Steam's own
/// library-list aesthetic. Some older AppIDs never got this asset, so a
/// 404 there falls back to the header image every app page has.
async fn fetch_cover(client: &reqwest::Client, appid: u32) -> Result<()> {
    let library_url =
        format!("https://cdn.akamai.steamstatic.com/steam/apps/{appid}/library_600x338.jpg");
    let header_url = format!("https://cdn.akamai.steamstatic.com/steam/apps/{appid}/header.jpg");

    let bytes = match download_image(client, &library_url).await {
        Ok(b) => b,
        Err(_) => download_image(client, &header_url).await?,
    };
    std::fs::write(cover_cache_path(appid)?, bytes)
        .with_context(|| format!("caching cover art for AppID {appid}"))?;
    Ok(())
}

async fn download_image(client: &reqwest::Client, url: &str) -> Result<Vec<u8>> {
    let resp = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("requesting {url}"))?
        .error_for_status()
        .with_context(|| format!("{url} returned an error status"))?;
    Ok(resp.bytes().await?.to_vec())
}
