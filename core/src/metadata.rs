// Copyright (C) 2026 M.R. Siavash Katebzadeh <mr@katebzadeh.xyz>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use spdlog::prelude::*;

use crate::{CoreResult, config::AppConfig, error::Error};

const SYNOPSIS_LIMIT: usize = 180;
const TRENDING_CACHE_TTL_SECS: u64 = 24 * 60 * 60;
const ANILIST_ENDPOINT: &str = "https://graphql.anilist.co";
const ANILIST_TRENDING_QUERY: &str = r"query ($perPage: Int!) {\n  Page(perPage: $perPage) {\n    media(type: ANIME, sort: TRENDING_DESC) {\n      id\n      siteUrl\n      title {\n        userPreferred\n        english\n        romaji\n      }\n      averageScore\n      season\n      seasonYear\n      description(asHtml: false)\n    }\n  }\n}";
const KITSU_TRENDING_URL: &str = "https://kitsu.io/api/edge/trending/anime";
const KITSU_ANIME_BASE_URL: &str = "https://kitsu.io/anime/";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MetadataSource {
    AniList,
    Kitsu,
}

impl MetadataSource {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            MetadataSource::AniList => "AniList",
            MetadataSource::Kitsu => "Kitsu",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendingEntry {
    pub id: String,
    pub title: String,
    pub score: Option<f32>,
    pub season: Option<String>,
    pub year: Option<u16>,
    pub synopsis: Option<String>,
    pub site_url: Option<String>,
    pub source: MetadataSource,
}

impl TrendingEntry {
    #[must_use]
    pub fn season_year_label(&self) -> Option<String> {
        match (self.season.as_deref(), self.year) {
            (Some(season), Some(year)) => Some(format!("{season} {year}")),
            (Some(season), None) => Some(season.to_string()),
            (None, Some(year)) => Some(year.to_string()),
            _ => None,
        }
    }

    #[must_use]
    pub fn score_label(&self) -> String {
        self.score
            .map_or_else(|| "N/A".to_string(), |score| format!("{score:.1}"))
    }

    #[must_use]
    pub fn detail_summary(&self) -> String {
        let mut parts = Vec::new();
        parts.push(format!("Title: {}", self.title));
        parts.push(format!(
            "Score: {} | Season: {} | Source: {}",
            self.score_label(),
            self.season_year_label()
                .unwrap_or_else(|| "TBA".to_string()),
            self.source.label()
        ));
        if let Some(synopsis) = self.synopsis.as_deref() {
            parts.push(format!("Synopsis: {synopsis}"));
        }
        if let Some(url) = self.site_url.as_deref() {
            parts.push(format!("More info: {url}"));
        }
        parts.join("\n")
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct TrendingSnapshot {
    entries: Vec<TrendingEntry>,
    fetched_at: u64,
}

impl TrendingSnapshot {
    fn new(entries: Vec<TrendingEntry>) -> Self {
        Self {
            entries,
            fetched_at: now_epoch_secs(),
        }
    }

    fn is_fresh(&self) -> bool {
        now_epoch_secs().saturating_sub(self.fetched_at) <= TRENDING_CACHE_TTL_SECS
    }

    fn load_optional(path: &Path) -> CoreResult<Option<Self>> {
        match fs::read_to_string(path) {
            Ok(contents) => {
                let snapshot: Self = serde_json::from_str(&contents).map_err(|source| {
                    Error::TrendingCacheParse {
                        path: path.to_path_buf(),
                        source,
                    }
                })?;
                Ok(Some(snapshot))
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(Error::TrendingCacheRead {
                path: path.to_path_buf(),
                source,
            }
            .into()),
        }
    }

    fn save(&self, path: &Path) -> CoreResult<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::TrendingCacheWrite {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        let payload =
            serde_json::to_string_pretty(self).map_err(|source| Error::TrendingCacheWrite {
                path: path.to_path_buf(),
                source: io::Error::other(source),
            })?;
        let tmp_path = path.with_extension("json.tmp");
        fs::write(&tmp_path, payload).map_err(|source| Error::TrendingCacheWrite {
            path: path.to_path_buf(),
            source,
        })?;
        fs::rename(&tmp_path, path).map_err(|source| Error::TrendingCacheWrite {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(())
    }
}

pub struct MetadataResolver {
    client: Client,
    cache: Option<TrendingSnapshot>,
    cache_path: PathBuf,
}

impl MetadataResolver {
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be constructed or the cache cannot be loaded.
    pub fn new(config: &AppConfig) -> CoreResult<Self> {
        let client = Client::builder()
            .user_agent("Animestan/0.1")
            .build()
            .map_err(Error::HttpClient)?;
        let cache_path = config.trending_cache_path();
        let cache = TrendingSnapshot::load_optional(&cache_path)?;
        Ok(Self {
            client,
            cache,
            cache_path,
        })
    }

    /// # Errors
    ///
    /// Propagates all errors from [`AppConfig::load_default`] and [`Self::new`].
    pub fn load_default() -> CoreResult<Self> {
        let config = AppConfig::load_default()?;
        Self::new(&config)
    }

    /// # Errors
    ///
    /// Returns an error when both `AniList` and `Kitsu` endpoints cannot be queried or parsed.
    pub fn fetch_trending(&mut self) -> CoreResult<Vec<TrendingEntry>> {
        if let Some(snapshot) = &self.cache {
            if snapshot.is_fresh() {
                debug!("returning cached trending entries");
                return Ok(snapshot.entries.clone());
            }
        }

        let entries = match self.fetch_from_anilist() {
            Ok(fetched) if !fetched.is_empty() => fetched,
            Ok(fetched) => fetched,
            Err(err) => {
                warn!("AniList trending failed: {err}");
                match self.fetch_from_kitsu() {
                    Ok(fallback) if !fallback.is_empty() => fallback,
                    Ok(fallback) => fallback,
                    Err(fallback_err) => {
                        warn!("Kitsu trending failed: {fallback_err}");
                        return Ok(self.cached_entries().unwrap_or_default());
                    }
                }
            }
        };

        Ok(self.update_cache(entries))
    }

    fn update_cache(&mut self, entries: Vec<TrendingEntry>) -> Vec<TrendingEntry> {
        let snapshot = TrendingSnapshot::new(entries.clone());
        if let Err(err) = snapshot.save(&self.cache_path) {
            warn!("failed to write trending cache: {err}");
        }
        self.cache = Some(snapshot);
        entries
    }

    fn cached_entries(&self) -> Option<Vec<TrendingEntry>> {
        self.cache.as_ref().map(|snapshot| snapshot.entries.clone())
    }

    fn fetch_from_anilist(&self) -> CoreResult<Vec<TrendingEntry>> {
        let variables = json!({ "perPage": 10 });
        let response = self
            .client
            .post(ANILIST_ENDPOINT)
            .json(&json!({
                "query": ANILIST_TRENDING_QUERY,
                "variables": variables,
            }))
            .send()
            .map_err(|source| Error::TrendingFetch {
                url: ANILIST_ENDPOINT.to_string(),
                source,
            })?;

        if !response.status().is_success() {
            return Err(Error::HttpStatus {
                url: ANILIST_ENDPOINT.to_string(),
                status: response.status().as_u16(),
            }
            .into());
        }

        let body = response.text().map_err(|source| Error::HttpBodyParse {
            url: ANILIST_ENDPOINT.to_string(),
            source,
        })?;
        let payload: AniListTrendingResponse =
            serde_json::from_str(&body).map_err(|source| Error::ResponseParse {
                url: ANILIST_ENDPOINT.to_string(),
                source,
            })?;

        Ok(payload
            .data
            .page
            .media
            .into_iter()
            .map(TrendingEntry::from_anilist)
            .collect())
    }

    fn fetch_from_kitsu(&self) -> CoreResult<Vec<TrendingEntry>> {
        let response = self
            .client
            .get(KITSU_TRENDING_URL)
            .query(&[("limit", "10")])
            .send()
            .map_err(|source| Error::TrendingFetch {
                url: KITSU_TRENDING_URL.to_string(),
                source,
            })?;

        if !response.status().is_success() {
            return Err(Error::HttpStatus {
                url: KITSU_TRENDING_URL.to_string(),
                status: response.status().as_u16(),
            }
            .into());
        }

        let body = response.text().map_err(|source| Error::HttpBodyParse {
            url: KITSU_TRENDING_URL.to_string(),
            source,
        })?;
        let payload: KitsuTrendingResponse =
            serde_json::from_str(&body).map_err(|source| Error::ResponseParse {
                url: KITSU_TRENDING_URL.to_string(),
                source,
            })?;

        Ok(payload
            .data
            .into_iter()
            .map(TrendingEntry::from_kitsu)
            .collect())
    }
}

impl TrendingEntry {
    fn from_anilist(anilist: AniListMedia) -> Self {
        Self {
            id: anilist.id.to_string(),
            title: anilist
                .title
                .user_preferred
                .or(anilist.title.english)
                .or(anilist.title.romaji)
                .unwrap_or_else(|| "Untitled".to_string()),
            score: anilist.average_score,
            season: standardize_season(anilist.season),
            year: anilist.season_year,
            synopsis: sanitize_synopsis(anilist.description),
            site_url: anilist.site_url,
            source: MetadataSource::AniList,
        }
    }

    fn from_kitsu(kitsu: KitsuTrendingNode) -> Self {
        let title = kitsu
            .attributes
            .canonical_title
            .clone()
            .or_else(|| kitsu.attributes.slug.clone())
            .unwrap_or_else(|| kitsu.id.clone());
        let score = kitsu
            .attributes
            .average_rating
            .as_deref()
            .and_then(|raw| raw.parse::<f32>().ok());
        let year = kitsu.attributes.start_date.as_deref().and_then(parse_year);
        let site_url = kitsu
            .attributes
            .slug
            .as_deref()
            .map(|slug| format!("{KITSU_ANIME_BASE_URL}{slug}"));

        Self {
            id: kitsu.id,
            title,
            score,
            season: None,
            year,
            synopsis: sanitize_synopsis(kitsu.attributes.synopsis),
            site_url,
            source: MetadataSource::Kitsu,
        }
    }
}

fn sanitize_synopsis(value: Option<String>) -> Option<String> {
    let description = value?;
    let stripped = strip_html_tags(&description);
    let normalized = normalize_whitespace(&stripped);
    if normalized.is_empty() {
        return None;
    }
    Some(truncate_snippet(&normalized, SYNOPSIS_LIMIT))
}

fn strip_html_tags(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut in_tag = false;
    for c in input.chars() {
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
            }
            _ if in_tag => {}
            c => result.push(c),
        }
    }
    result
}

fn normalize_whitespace(input: &str) -> String {
    let mut normalized = String::with_capacity(input.len());
    let mut last_was_space = false;
    for c in input.chars() {
        if c.is_whitespace() {
            if !last_was_space {
                normalized.push(' ');
                last_was_space = true;
            }
        } else {
            normalized.push(c);
            last_was_space = false;
        }
    }
    normalized.trim().to_string()
}

fn truncate_snippet(text: &str, limit: usize) -> String {
    let mut iter = text.chars();
    let truncated: String = iter.by_ref().take(limit).collect();
    if iter.next().is_none() {
        truncated
    } else {
        let mut trimmed = truncated.trim_end().to_string();
        if !trimmed.ends_with('…') {
            trimmed.push('…');
        }
        trimmed
    }
}

fn parse_year(value: &str) -> Option<u16> {
    value.split('-').next()?.parse::<u16>().ok()
}

fn standardize_season(value: Option<String>) -> Option<String> {
    value.map(|season| {
        let lower = season.to_lowercase();
        let mut chars = lower.chars();
        if let Some(first) = chars.next() {
            let rest: String = chars.collect();
            let mut head = first.to_uppercase().collect::<String>();
            head.push_str(&rest);
            head
        } else {
            String::new()
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_synopsis_removes_html() {
        let raw = "<p>Bold <strong>text</strong> and &amp; extras.</p>".to_string();
        let snippet = sanitize_synopsis(Some(raw)).unwrap();
        assert!(snippet.contains("Bold"));
        assert!(snippet.contains("text"));
        assert!(!snippet.contains('<'));
    }

    #[test]
    fn sanitize_synopsis_truncates_long_text() {
        let long_text = "a".repeat(SYNOPSIS_LIMIT + 5);
        let snippet = sanitize_synopsis(Some(long_text.clone())).unwrap();
        assert!(snippet.ends_with('…'));
        assert!(snippet.chars().count() <= SYNOPSIS_LIMIT + 1);
    }

    #[test]
    fn standardize_season_formats_properly() {
        assert_eq!(
            standardize_season(Some("SPRING".to_string())),
            Some("Spring".to_string())
        );
        assert_eq!(
            standardize_season(Some("winter".to_string())),
            Some("Winter".to_string())
        );
    }

    #[test]
    fn snapshot_is_fresh_immediately() {
        let snapshot = TrendingSnapshot::new(Vec::new());
        assert!(snapshot.is_fresh());
    }
}

fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[derive(Deserialize)]
struct AniListTrendingResponse {
    data: AniListPageContainer,
}

#[derive(Deserialize)]
struct AniListPageContainer {
    #[serde(rename = "Page")]
    page: AniListPage,
}

#[derive(Deserialize)]
struct AniListPage {
    media: Vec<AniListMedia>,
}

#[derive(Deserialize)]
struct AniListMedia {
    id: i64,
    #[serde(rename = "siteUrl")]
    site_url: Option<String>,
    title: AniListTitle,
    #[serde(rename = "averageScore")]
    average_score: Option<f32>,
    season: Option<String>,
    #[serde(rename = "seasonYear")]
    season_year: Option<u16>,
    description: Option<String>,
}

#[derive(Deserialize)]
struct AniListTitle {
    #[serde(rename = "userPreferred")]
    user_preferred: Option<String>,
    english: Option<String>,
    romaji: Option<String>,
}

#[derive(Deserialize)]
struct KitsuTrendingResponse {
    data: Vec<KitsuTrendingNode>,
}

#[derive(Deserialize)]
struct KitsuTrendingNode {
    id: String,
    attributes: KitsuAttributes,
}

#[derive(Deserialize)]
struct KitsuAttributes {
    #[serde(rename = "canonicalTitle")]
    canonical_title: Option<String>,
    #[serde(rename = "averageRating")]
    average_rating: Option<String>,
    #[serde(rename = "startDate")]
    start_date: Option<String>,
    synopsis: Option<String>,
    slug: Option<String>,
}
