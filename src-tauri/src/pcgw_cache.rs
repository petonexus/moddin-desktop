//! PCGamingWiki enrichment cache.
//!
//! PCGamingWiki is a community-maintained wiki that documents engine,
//! save location, anti-cheat, known issues, and other practical info
//! for PC games. Moddin reads a small subset of that data to enrich the
//! game detail view: engine confirmation, save location pointers,
//! mod-relevant categories, and a handful of `Issues` lines that may
//! explain a failure mode Moddin cannot otherwise detect.
//!
//! ## Why a cache, not a live fetch
//!
//! The PCGamingWiki API explicitly asks third-party clients to identify
//! themselves with a meaningful `User-Agent` and to limit request
//! rate. Moddin:
//!
//! - Identifies itself as `Moddin-Desktop/0.1 (+repository URL)`.
//! - Caches the parsed summary locally under
//!   `%LOCALAPPDATA%/Moddin/pcgw/<slug>.json` so the second view of the
//!   same game does not hit the API at all.
//! - Returns a `staleness_hours` field so the UI can offer a
//!   "Refresh from PCGamingWiki" button instead of silently refetching.
//!
//! This module does **not** touch the game catalog or the recipe store.
//! PCGamingWiki data is treated strictly as user-visible enrichment,
//! never as a source of truth for what Moddin should install.
//!
//! ## Wiki subset
//!
//! We do **not** ship a full MediaWiki parser. The cache stores:
//!
//! - engine string from the `{{Game data|...}}` template (`Engine` key);
//! - `Executable name`, `Install location`, `Save game data location`
//!   fields when present;
//! - the first N categories (usually enough to identify VR / FPS /
//!   moddable / online-only);
//! - the first M lines of the `Issues` section, plain text only.
//!
//! Anything more aggressive would be a maintenance burden; the rest of
//! the page is one click away via the canonical `page_url` returned
//! alongside the summary.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    env, fs,
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const API_ENDPOINT: &str = "https://www.pcgamingwiki.com/w/api.php";
const MAX_FETCH_BYTES: u64 = 4 * 1024 * 1024;
const MAX_CATEGORIES: usize = 32;
const MAX_ISSUE_LINES: usize = 16;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PcgwSummary {
    pub slug: String,
    pub page_url: String,
    pub fetched_at_millis: u64,
    pub source_revision_id: Option<u64>,
    pub engine: Option<String>,
    pub executable_name: Option<String>,
    pub install_location: Option<String>,
    pub save_game_data_location: Option<String>,
    pub categories: Vec<String>,
    pub issue_lines: Vec<String>,
    pub raw_excerpt: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PcgwLookupRequest {
    pub slug: String,
    /// When `true`, bypass the local cache and force a fresh fetch. The
    /// caller is responsible for honoring the wiki's rate limit; Moddin
    /// does not arbitrate that.
    #[serde(default)]
    pub force_refresh: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PcgwLookupResult {
    pub summary: PcgwSummary,
    /// Hours since the underlying fetch. `None` if this call did the
    /// fetch itself.
    pub staleness_hours: Option<f64>,
    /// `true` when the cache was missing or the user requested a
    /// refresh. Useful for the UI to show a toast.
    pub from_network: bool,
}

fn cache_root() -> PathBuf {
    env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir)
        .join("Moddin")
        .join("pcgw")
}

fn is_valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 256
        && slug
            .chars()
            .all(|character| character.is_ascii_alphanumeric()
                || matches!(character, '-' | '_' | '.' | '(' | ')' | ' ' | ':' | ','))
}

fn slug_to_filename(slug: &str) -> String {
    slug.chars()
        .map(|character| match character {
            '/' | '\\' | '<' | '>' | ':' | '"' | '|' | '?' | '*' => '_',
            other => other,
        })
        .collect()
}

fn cache_path(slug: &str) -> Option<PathBuf> {
    if !is_valid_slug(slug) {
        return None;
    }
    Some(cache_root().join(format!("{}.json", slug_to_filename(slug))))
}

fn read_cache(slug: &str) -> Option<PcgwSummary> {
    let path = cache_path(slug)?;
    let contents = fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

fn write_cache(summary: &PcgwSummary) -> Result<(), String> {
    let path = cache_path(&summary.slug).ok_or_else(|| "Invalid PCGW slug.".to_owned())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create PCGW cache directory: {error}"))?;
    }
    let json = serde_json::to_vec_pretty(summary)
        .map_err(|error| format!("Could not serialize PCGW cache: {error}"))?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, &json).map_err(|error| format!("Could not write PCGW cache: {error}"))?;
    fs::rename(&tmp, &path).map_err(|error| format!("Could not commit PCGW cache: {error}"))?;
    Ok(())
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn user_agent() -> &'static str {
    "Moddin-Desktop/0.1 (+https://github.com/petonexus/moddin-desktop)"
}

#[derive(Debug, Deserialize)]
struct ParseResponse {
    parse: Option<ParseBody>,
    error: Option<ParseError>,
}

#[derive(Debug, Deserialize)]
struct ParseBody {
    title: Option<String>,
    pageid: Option<u64>,
    wikitext: Option<ParseWikitext>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ParseWikitext {
    #[serde(default)]
    revid: Option<u64>,
    #[serde(default)]
    contentmodel: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ParseError {
    code: String,
    info: Option<String>,
}

async fn fetch_wikitext(slug: &str) -> Result<(String, Option<u64>, Option<String>), String> {
    if !is_valid_slug(slug) {
        return Err("PCGW slug must be a non-empty string of safe characters.".to_owned());
    }

    let client = Client::builder()
        .user_agent(user_agent())
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| format!("Could not create PCGW client: {error}"))?;

    let response = client
        .get(API_ENDPOINT)
        .query(&[
            ("action", "parse"),
            ("page", slug),
            ("format", "json"),
            ("formatversion", "2"),
            ("prop", "wikitext|revisions"),
            ("redirects", "1"),
        ])
        .send()
        .await
        .map_err(|error| format!("Could not reach PCGamingWiki: {error}"))?;

    let status = response.status();
    if status.as_u16() == 404 {
        return Err(format!("PCGamingWiki has no page for '{slug}'."));
    }
    if !status.is_success() {
        return Err(format!("PCGamingWiki returned status {status}."));
    }

    let body = response
        .bytes()
        .await
        .map_err(|error| format!("Could not read PCGamingWiki response: {error}"))?;
    if body.len() as u64 > MAX_FETCH_BYTES {
        return Err("PCGamingWiki response exceeded the 4 MB safety limit.".to_owned());
    }

    let parsed: ParseResponse = serde_json::from_slice(&body)
        .map_err(|error| format!("Could not parse PCGamingWiki response: {error}"))?;

    if let Some(error) = parsed.error {
        return Err(format!(
            "PCGamingWiki rejected the request: {} {}",
            error.code,
            error.info.unwrap_or_default()
        ));
    }

    let parse = parsed
        .parse
        .ok_or_else(|| "PCGamingWiki response was missing the `parse` block.".to_owned())?;
    let text = parse
        .title
        .ok_or_else(|| "PCGamingWiki response was missing a page title.".to_owned())?;
    let revision_id = parse.wikitext.as_ref().and_then(|value| value.revid);
    let content_model = parse
        .wikitext
        .and_then(|value| value.contentmodel);
    let raw = serde_json::to_string(&text)
        .map_err(|error| format!("Could not decode PCGamingWiki body: {error}"))?;
    let raw = raw.trim_matches('"').to_owned();
    Ok((raw, revision_id, content_model))
}

/// Pull the value of a named key out of a `{{Game data|...}}` template
/// line. Templates look like:
/// `{{Game data
/// |Engine     = Unreal Engine 5
/// |Executable = game.exe
/// }}`
fn extract_game_data_field(wikitext: &str, key: &str) -> Option<String> {
    let lower_key = key.to_ascii_lowercase();
    let mut in_template = false;
    for line in wikitext.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("{{Game data") {
            in_template = true;
            continue;
        }
        if in_template && trimmed.starts_with("}}") {
            in_template = false;
            continue;
        }
        if !in_template {
            continue;
        }
        let Some((raw_key, raw_value)) = trimmed.split_once('=') else {
            continue;
        };
        let cleaned_key = raw_key
            .trim()
            .trim_start_matches('|')
            .trim()
            .to_ascii_lowercase();
        if cleaned_key != lower_key {
            continue;
        }
        let value = raw_value.trim();
        if value.is_empty() {
            continue;
        }
        // Strip simple wikitext markup from the value: `[[Target|Display]]`
        // → `Display`, `[[Target]]` → `Target`, leading/trailing `'''`.
        let stripped = strip_wikilinks(value);
        let stripped = stripped.trim_matches('\'').trim().to_owned();
        if stripped.is_empty() {
            continue;
        }
        return Some(stripped);
    }
    None
}

fn strip_wikilinks(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '[' && chars.peek() == Some(&'[') {
            chars.next();
            let mut inner = String::new();
            while let Some(next) = chars.next() {
                if next == ']' && chars.peek() == Some(&']') {
                    chars.next();
                    break;
                }
                inner.push(next);
            }
            let display = inner.split('|').last().unwrap_or(&inner);
            output.push_str(display);
            continue;
        }
        output.push(character);
    }
    output
}

fn extract_categories(wikitext: &str) -> Vec<String> {
    wikitext
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let after = trimmed.strip_prefix("[[Category:")?;
            let end = after.find("]]")?;
            Some(after[..end].trim().to_owned())
        })
        .take(MAX_CATEGORIES)
        .collect()
}

fn extract_issue_lines(wikitext: &str) -> Vec<String> {
    // Find the `==Issues==` (or `===...===`) section and emit the next
    // non-empty bullet lines as plain text. This is intentionally crude —
    // we only need enough text for the UI to decide whether a fuller
    // fetch is warranted.
    let mut lines = wikitext.lines();
    let mut in_issues = false;
    let mut collected = Vec::new();
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.starts_with("==") {
            let lower = trimmed.to_ascii_lowercase();
            if lower.starts_with("==issues") || lower.contains("issues==") {
                in_issues = true;
                continue;
            }
            if in_issues && trimmed.starts_with("==") && !lower.contains("issues") {
                in_issues = false;
            }
        }
        if !in_issues {
            continue;
        }
        let plain = strip_wikilinks(trimmed)
            .trim_start_matches('*')
            .trim_start_matches('#')
            .trim()
            .to_owned();
        if plain.is_empty() {
            continue;
        }
        collected.push(plain);
        if collected.len() >= MAX_ISSUE_LINES {
            break;
        }
    }
    collected
}

fn build_summary(
    slug: &str,
    wikitext: &str,
    revision_id: Option<u64>,
) -> PcgwSummary {
    let engine = extract_game_data_field(wikitext, "Engine");
    let executable_name = extract_game_data_field(wikitext, "Executable name")
        .or_else(|| extract_game_data_field(wikitext, "Executable"));
    let install_location = extract_game_data_field(wikitext, "Install location");
    let save_game_data_location = extract_game_data_field(wikitext, "Save game data location");
    let categories = extract_categories(wikitext);
    let issue_lines = extract_issue_lines(wikitext);
    let raw_excerpt = first_lines(wikitext, 24);
    let page_url = format!(
        "https://www.pcgamingwiki.com/wiki/{}",
        url_encode(slug)
    );

    PcgwSummary {
        slug: slug.to_owned(),
        page_url,
        fetched_at_millis: now_millis(),
        source_revision_id: revision_id,
        engine,
        executable_name,
        install_location,
        save_game_data_location,
        categories,
        issue_lines,
        raw_excerpt: Some(raw_excerpt),
    }
}

fn first_lines(wikitext: &str, count: usize) -> String {
    wikitext
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(count)
        .collect::<Vec<_>>()
        .join("\n")
}

fn url_encode(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                output.push(byte as char);
            }
            other => output.push_str(&format!("%{:02X}", other)),
        }
    }
    output
}

/// Look up a PCGamingWiki page by slug. Reads from local cache when
/// fresh and falls back to a network fetch on miss. The caller can pass
/// `force_refresh = true` to bypass the cache.
#[tauri::command]
pub async fn lookup_pcgw_summary(
    request: PcgwLookupRequest,
) -> Result<PcgwLookupResult, String> {
    if !is_valid_slug(&request.slug) {
        return Err("PCGW slug must be a non-empty string of safe characters.".to_owned());
    }
    let cached = read_cache(&request.slug);
    if !request.force_refresh {
        if let Some(cached) = cached.clone() {
            let age_hours = now_millis()
                .saturating_sub(cached.fetched_at_millis)
                as f64
                / 3_600_000.0;
            return Ok(PcgwLookupResult {
                summary: cached,
                staleness_hours: Some(age_hours),
                from_network: false,
            });
        }
    }

    let (wikitext, revision_id, _content_model) = fetch_wikitext(&request.slug).await?;
    let summary = build_summary(&request.slug, &wikitext, revision_id);
    write_cache(&summary)?;
    Ok(PcgwLookupResult {
        summary,
        staleness_hours: Some(0.0),
        from_network: true,
    })
}

/// Returns the cached summary without touching the network. Used by the
/// UI on game-detail mount to show the enrichment card before the
/// user has ever clicked "Refresh from PCGamingWiki".
#[tauri::command]
pub fn get_pcgw_cache(slug: String) -> Result<Option<PcgwSummary>, String> {
    if !is_valid_slug(&slug) {
        return Err("PCGW slug must be a non-empty string of safe characters.".to_owned());
    }
    Ok(read_cache(&slug))
}

/// Clears the local cache for a single slug. The next `lookup_pcgw_summary`
/// call will fall back to the network.
#[tauri::command]
pub fn clear_pcgw_cache(slug: String) -> Result<(), String> {
    let Some(path) = cache_path(&slug) else {
        return Err("PCGW slug must be a non-empty string of safe characters.".to_owned());
    };
    if path.is_file() {
        fs::remove_file(&path)
            .map_err(|error| format!("Could not clear PCGW cache: {error}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_validation_rejects_path_traversal_and_control_characters() {
        assert!(is_valid_slug("Elden Ring"));
        assert!(is_valid_slug("The_Binding_of_Isaac"));
        assert!(!is_valid_slug(""));
        assert!(!is_valid_slug("../etc/passwd"));
        assert!(!is_valid_slug("name\ttab"));
        assert!(!is_valid_slug("name/withslash"));
    }

    #[test]
    fn slug_filename_sanitizes_path_separators() {
        assert_eq!(slug_to_filename("Foo/Bar: Baz"), "Foo_Bar_ Baz");
        assert_eq!(slug_to_filename("ok name"), "ok name");
    }

    #[test]
    fn extract_game_data_field_handles_typical_template() {
        let wikitext = "\
==Availability==
{{Game data
|Engine     = Unreal Engine 5
|Executable = game.exe
|Install location = C:\\\\Games\\\\Game
|Save game data location = %APPDATA%\\\\Game\\\\Save
}}
";
        assert_eq!(
            extract_game_data_field(wikitext, "Engine").as_deref(),
            Some("Unreal Engine 5")
        );
        assert_eq!(
            extract_game_data_field(wikitext, "Executable").as_deref(),
            Some("game.exe")
        );
        assert_eq!(
            extract_game_data_field(wikitext, "Install location").as_deref(),
            Some("C:\\\\Games\\\\Game")
        );
        assert_eq!(
            extract_game_data_field(wikitext, "Save game data location").as_deref(),
            Some("%APPDATA%\\\\Game\\\\Save")
        );
        assert_eq!(
            extract_game_data_field(wikitext, "Steam app ID").as_deref(),
            None
        );
    }

    #[test]
    fn extract_game_data_field_handles_display_aliases() {
        let wikitext = "\
{{Game data
|Executable name = [[Game.exe|the game]]
}}
";
        assert_eq!(
            extract_game_data_field(wikitext, "Executable name").as_deref(),
            Some("the game")
        );
    }

    #[test]
    fn extract_categories_collects_unique_entries() {
        let wikitext = "\
[[Category:Action]]
[[Category:Singleplayer]]
[[Category:VR mods]]
";
        assert_eq!(
            extract_categories(wikitext),
            vec!["Action".to_owned(), "Singleplayer".to_owned(), "VR mods".to_owned()]
        );
    }

    #[test]
    fn extract_issue_lines_stops_at_section_boundary() {
        let wikitext = "\
==Availability==
Game is available on Steam.

==Issues==
* Crashes when alt-tabbing on Windows 11.
* Anti-cheat prevents mod loading on multiplayer.

==Other==
''This section should be ignored.''
";
        let lines = extract_issue_lines(wikitext);
        assert_eq!(
            lines,
            vec![
                "Crashes when alt-tabbing on Windows 11.".to_owned(),
                "Anti-cheat prevents mod loading on multiplayer.".to_owned(),
            ]
        );
    }

    #[test]
    fn strip_wikilinks_extracts_display_label() {
        assert_eq!(strip_wikilinks("[[Game.exe|the game]]"), "the game");
        assert_eq!(strip_wikilinks("[[Game.exe]]"), "Game.exe");
        assert_eq!(strip_wikilinks("plain text"), "plain text");
    }

    #[test]
    fn url_encode_handles_unsafe_characters() {
        assert_eq!(url_encode("Elden Ring"), "Elden%20Ring");
        assert_eq!(url_encode("Sid Meier's Civilization VI"), "Sid%20Meier%27s%20Civilization%20VI");
    }

    #[test]
    fn build_summary_pulls_expected_fields() {
        let wikitext = "\
==Availability==
{{Game data
|Engine     = REDengine
|Executable = Cyberpunk2077.exe
|Save game data location = %USERPROFILE%\\\\Saved Games\\\\CD Projekt Red\\\\Cyberpunk 2077
}}
[[Category:Open world]]
[[Category:VR mods]]
==Issues==
* Cutscenes flicker with HDR on.
";
        let summary = build_summary("Cyberpunk 2077", wikitext, Some(42));
        assert_eq!(summary.slug, "Cyberpunk 2077");
        assert_eq!(summary.engine.as_deref(), Some("REDengine"));
        assert_eq!(summary.executable_name.as_deref(), Some("Cyberpunk2077.exe"));
        assert_eq!(summary.source_revision_id, Some(42));
        assert_eq!(summary.categories, vec!["Open world".to_owned(), "VR mods".to_owned()]);
        assert_eq!(summary.issue_lines.len(), 1);
        assert!(summary.page_url.contains("Cyberpunk%202077"));
    }

    #[test]
    fn build_summary_returns_none_for_missing_fields() {
        let wikitext = "Hello world.";
        let summary = build_summary("Empty", wikitext, None);
        assert_eq!(summary.slug, "Empty");
        assert!(summary.engine.is_none());
        assert!(summary.executable_name.is_none());
        assert!(summary.categories.is_empty());
        assert!(summary.issue_lines.is_empty());
        assert!(summary.raw_excerpt.unwrap().contains("Hello world."));
    }

    #[test]
    fn cache_path_rejects_invalid_slugs() {
        assert!(cache_path("").is_none());
        assert!(cache_path("../escape").is_none());
        assert!(cache_path("Elden Ring").is_some());
    }
}
