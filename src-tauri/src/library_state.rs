//! Background library cache.
//!
//! The `LibraryCache` is the in-memory snapshot returned by the
//! background scanner. The frontend can poll it via
//! `get_cached_installed_games` without paying the cost of a full
//! `detect_installed_games` (Steam manifest reads + Epic + GOG) on every
//! refresh.
//!
//! The cache intentionally holds a flat list — matching against the
//! catalog is the catalog loader's job, not the scanner's. The state
//! also tracks when the cache was last populated so the UI can show
//! "last refreshed …" without a separate round-trip.

use crate::steam::{detect_installed_games_blocking, InstalledGame};
use serde::Serialize;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedLibrary {
    pub generated_at_millis: u64,
    pub games: Vec<InstalledGame>,
}

#[derive(Default)]
pub struct LibraryCache {
    inner: Arc<Mutex<Option<CachedLibrary>>>,
}

impl LibraryCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn shared(&self) -> Arc<Mutex<Option<CachedLibrary>>> {
        self.inner.clone()
    }

    pub fn snapshot(&self) -> Option<CachedLibrary> {
        self.inner.lock().ok().and_then(|guard| guard.clone())
    }

    pub fn replace(&self, games: Vec<InstalledGame>) -> CachedLibrary {
        let cached = CachedLibrary {
            generated_at_millis: now_millis(),
            games,
        };
        if let Ok(mut guard) = self.inner.lock() {
            *guard = Some(cached.clone());
        }
        cached
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
    // If the system clock is before the epoch we still want a
    // monotonic-ish timestamp; subtract via `saturating_sub` is not
    // available for `Duration`, so we fall back to zero.
    .unwrap_or_else(|_| {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH.checked_sub(Duration::from_secs(0)).unwrap())
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    })
}

/// Re-scan Steam, Epic, and GOG and replace the cache with the result.
/// The frontend can call this when the user clicks "Refresh" or on a
/// timer (the latter is recommended every 60–120s while the window is
/// focused).
#[tauri::command]
pub async fn refresh_installed_games_async(
    state: tauri::State<'_, LibraryCache>,
) -> Result<CachedLibrary, String> {
    let cache = state.shared();
    let cached = run_scan(cache).await?;
    Ok(cached)
}

async fn run_scan(cache: Arc<Mutex<Option<CachedLibrary>>>) -> Result<CachedLibrary, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<CachedLibrary, String> {
        let mut games = detect_installed_games_blocking()?;
        let gog_games = crate::gog::detect_gog_installed_games_blocking();
        for game in gog_games {
            let key = format!("{}:{}", game.store, game.app_id);
            if !games.iter().any(|existing| {
                format!("{}:{}", existing.store, existing.app_id) == key
            }) {
                games.push(game);
            }
        }
        games.sort_by(|a, b| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
                .then_with(|| a.store.cmp(&b.store))
        });
        Ok(replace_in_arc(&cache, games))
    })
    .await
    .map_err(|error| format!("Background library scan task failed: {error}"))?
}

fn replace_in_arc(
    cache: &Arc<Mutex<Option<CachedLibrary>>>,
    games: Vec<InstalledGame>,
) -> CachedLibrary {
    let cached = CachedLibrary {
        generated_at_millis: now_millis(),
        games,
    };
    if let Ok(mut guard) = cache.lock() {
        *guard = Some(cached.clone());
    }
    cached
}

/// Return the last cached snapshot. Returns an empty list when the
/// scanner has never been invoked; the frontend should treat that as a
/// hint to call `refresh_installed_games_async` immediately.
#[tauri::command]
pub fn get_cached_installed_games(
    state: tauri::State<'_, LibraryCache>,
) -> Result<CachedLibrary, String> {
    Ok(state.snapshot().unwrap_or(CachedLibrary {
        generated_at_millis: 0,
        games: Vec::new(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_then_snapshot_returns_same_games() {
        let cache = LibraryCache::new();
        assert!(cache.snapshot().is_none());

        let installed = InstalledGame {
            store: "steam".to_owned(),
            app_id: "1245620".to_owned(),
            name: "Elden Ring".to_owned(),
            install_dir: "C:/Games/EldenRing".to_owned(),
            library_path: "C:/SteamLibrary".to_owned(),
        };
        let cached = cache.replace(vec![installed.clone()]);
        assert_eq!(cached.games.len(), 1);
        let snap = cache.snapshot().expect("snapshot present");
        assert_eq!(snap.games.len(), 1);
        assert_eq!(snap.games[0].app_id, installed.app_id);
    }

    #[test]
    fn snapshot_is_empty_when_cache_never_populated() {
        let cache = LibraryCache::new();
        let snap = cache.snapshot();
        assert!(snap.is_none());
    }
}
