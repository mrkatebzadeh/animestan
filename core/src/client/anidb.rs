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

use scraper::{Html, Selector};
use serde::Deserialize;
use spdlog::prelude::*;
use std::{cmp::Reverse, collections::HashSet};
use url::Url;

use crate::{
    CoreResult,
    config::{QualityPreference, StreamingMode},
    error::Error,
    models::{AnimeEntry, Episode},
};

pub(crate) const BASE_URL: &str = "https://anidb.app";
pub(crate) const USER_AGENT_VALUE: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";

#[derive(Deserialize)]
struct EpisodesResponse {
    episodes: Vec<EpisodeRecord>,
}

#[derive(Deserialize)]
struct EpisodeRecord {
    id: u64,
    number: u32,
    #[serde(rename = "number2")]
    _number2: Option<u32>,
    #[serde(rename = "filler")]
    _filler: bool,
}

#[derive(Deserialize)]
struct LanguagesResponse {
    languages: Vec<LanguageRecord>,
}

#[derive(Deserialize)]
struct LanguageRecord {
    code: String,
    embed_url: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HlsVariant {
    pub(crate) height: u16,
    pub(crate) url: Url,
}

pub(crate) fn anime_numeric_id(anime_id: &str) -> CoreResult<&str> {
    let suffix = anime_id.rsplit_once('-').map_or("", |(_, suffix)| suffix);
    if suffix.is_empty() || !suffix.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(Error::EpisodeIdParse {
            episode_id: anime_id.to_string(),
        }
        .into());
    }
    Ok(suffix)
}

pub(crate) fn parse_search(html: &str, source_id: &str) -> CoreResult<Vec<AnimeEntry>> {
    if html.contains("Just a moment") {
        return Err(Error::ProviderBlocked {
            url: BASE_URL.to_string(),
        }
        .into());
    }

    let document = Html::parse_document(html);
    let link_selector = Selector::parse(r#"a[href*="/anime/"]"#).expect("valid AniDB selector");
    let image_selector = Selector::parse("img[alt]").expect("valid image selector");
    let mut seen = HashSet::new();
    let mut entries = Vec::new();

    for link in document.select(&link_selector) {
        let Some(href) = link.value().attr("href") else {
            continue;
        };
        let Some(anime_id) = anime_path_segment(href) else {
            continue;
        };
        let title = link
            .value()
            .attr("title")
            .filter(|title| !title.is_empty())
            .or_else(|| {
                link.select(&image_selector)
                    .find_map(|image| image.value().attr("alt"))
                    .filter(|title| !title.is_empty())
            });
        let Some(title) = title else {
            continue;
        };
        if !seen.insert(anime_id.to_string()) {
            continue;
        }

        entries.push(AnimeEntry {
            id: anime_id.to_string(),
            title: title.to_string(),
            source_id: source_id.to_string(),
        });
    }

    Ok(entries)
}

pub(crate) fn parse_episodes(
    body: &str,
    anime_id: &str,
    source_id: &str,
) -> CoreResult<Vec<Episode>> {
    let numeric_id = anime_numeric_id(anime_id)?;
    let response: EpisodesResponse =
        serde_json::from_str(body).map_err(|source| Error::ResponseParse {
            url: format!("{BASE_URL}/api/frontend/anime/{numeric_id}/episodes"),
            source,
        })?;

    let mut episodes: Vec<Episode> = response
        .episodes
        .into_iter()
        .map(|episode| Episode {
            id: episode.id.to_string(),
            number: episode.number,
            title: format!("Episode {}", episode.number),
            anime_id: anime_id.to_string(),
            source_id: source_id.to_string(),
            synopsis: None,
            duration_secs: None,
            air_date: None,
        })
        .collect();
    episodes.sort_by_key(|episode| episode.number);
    Ok(episodes)
}

fn anime_path_segment(href: &str) -> Option<&str> {
    let path = href.split(['?', '#']).next()?;
    let mut segments = path.split('/');
    while let Some(segment) = segments.next() {
        if segment != "anime" {
            continue;
        }

        let anime_id = segments.next()?;
        anime_numeric_id(anime_id).ok()?;
        return Some(anime_id);
    }
    None
}

pub(crate) fn parse_languages(body: &str, mode: StreamingMode) -> CoreResult<Url> {
    let response: LanguagesResponse =
        serde_json::from_str(body).map_err(|source| Error::ResponseParse {
            url: format!("{BASE_URL}/api/frontend/episode/languages"),
            source,
        })?;
    let language = response
        .languages
        .into_iter()
        .find(|language| language.code == mode.code())
        .ok_or_else(|| Error::StreamResolution {
            message: format!("no {} language stream was returned", mode.code()),
        })?;
    Url::parse(&language.embed_url)
        .map_err(|source| Error::StreamUrlParse {
            url: language.embed_url,
            source,
        })
        .map_err(Into::into)
}

pub(crate) fn extract_master_url(embed: &str) -> CoreResult<Url> {
    let marker = "file: '";
    let start = embed.find(marker).ok_or_else(|| Error::StreamResolution {
        message: "embed response did not contain a master playlist".to_string(),
    })? + marker.len();
    let end = embed[start..]
        .find('\'')
        .map_or(embed.len(), |offset| start + offset);
    if end == embed.len() {
        return Err(Error::StreamResolution {
            message: "embed response contained an unterminated master playlist".to_string(),
        }
        .into());
    }
    let url = &embed[start..end];
    Url::parse(url)
        .map_err(|source| Error::StreamUrlParse {
            url: url.to_string(),
            source,
        })
        .map_err(Into::into)
}

pub(crate) fn parse_master_playlist(body: &str, master_url: &Url) -> CoreResult<Vec<HlsVariant>> {
    let mut variants = Vec::new();
    let mut lines = body.lines();

    while let Some(line) = lines.next() {
        let Some(height) = stream_height(line) else {
            continue;
        };
        let Some(uri) = lines
            .by_ref()
            .find(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
        else {
            break;
        };
        let uri = uri.trim();
        let url = master_url
            .join(uri)
            .map_err(|source| Error::StreamUrlParse {
                url: uri.to_string(),
                source,
            })?;
        variants.push(HlsVariant { height, url });
    }

    if variants.is_empty() {
        return Err(Error::StreamResolution {
            message: "master playlist did not contain any variants".to_string(),
        }
        .into());
    }
    variants.sort_by_key(|variant| Reverse(variant.height));
    Ok(variants)
}

fn stream_height(line: &str) -> Option<u16> {
    let resolution = line.split_once("RESOLUTION=")?.1;
    let (_, height) = resolution.split_once('x')?;
    height.split(',').next()?.parse().ok()
}

pub(crate) fn select_variant(
    variants: &[HlsVariant],
    quality: QualityPreference,
) -> CoreResult<&HlsVariant> {
    let best = variants.first().ok_or_else(|| Error::StreamResolution {
        message: "no HLS variants available".to_string(),
    })?;
    match quality {
        QualityPreference::Best => Ok(best),
        QualityPreference::Worst => Ok(variants.last().unwrap_or(best)),
        QualityPreference::Height(height) => {
            if let Some(variant) = variants.iter().find(|variant| variant.height == height) {
                Ok(variant)
            } else {
                warn!("requested {height}p variant was not found; using best available variant");
                Ok(best)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        anime_numeric_id, extract_master_url, parse_episodes, parse_languages,
        parse_master_playlist, parse_search, select_variant,
    };
    use crate::config::{QualityPreference, StreamingMode};
    use url::Url;

    #[test]
    fn parses_and_deduplicates_browse_cards() {
        let html = r#"
            <a href="https://anidb.app/anime/naruto-3686" title="Naruto">
              <img alt="Naruto" src="poster.jpg">
            </a>
            <a href="/anime/naruto-3686" title="Naruto"><img alt="Naruto"></a>
            <a href="/anime/boruto-naruto-next-generations-4647" title="Boruto &amp; Naruto">
              <img alt="Boruto &amp; Naruto">
            </a>
        "#;
        let entries = parse_search(html, "anidb").expect("search entries");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, "naruto-3686");
        assert_eq!(entries[1].title, "Boruto & Naruto");
    }

    #[test]
    fn rejects_cloudflare_block_page() {
        let error =
            parse_search("<title>Just a moment...</title>", "anidb").expect_err("blocked response");
        assert!(error.to_string().contains("Cloudflare"));
    }

    #[test]
    fn maps_episode_api_ids_and_sorts_numbers() {
        let body = r#"{"episodes":[{"id":6090,"number":4,"number2":null,"filler":false},{"id":6087,"number":1,"number2":null,"filler":false}]}"#;
        let episodes = parse_episodes(body, "ippon-again-20", "anidb").expect("episodes");
        assert_eq!(episodes[0].id, "6087");
        assert_eq!(episodes[0].anime_id, "ippon-again-20");
        assert_eq!(episodes[1].number, 4);
    }

    #[test]
    fn extracts_numeric_anime_suffix() {
        assert_eq!(anime_numeric_id("naruto-3686").unwrap(), "3686");
        assert!(anime_numeric_id("naruto").is_err());
    }

    #[test]
    fn malformed_episode_response_reports_numeric_endpoint() {
        let error =
            parse_episodes("not json", "naruto-3686", "anidb").expect_err("malformed response");
        let message = error.to_string();
        assert!(message.contains("/api/frontend/anime/3686/episodes"));
        assert!(!message.contains("/api/frontend/anime/naruto-3686/episodes"));
    }

    #[test]
    fn selects_requested_language_without_fallback() {
        let body = r#"{"languages":[{"code":"jpn","name":"Japanese","embed_url":"https://anidb.app/embed/sub"},{"code":"eng","name":"English","embed_url":"https://anidb.app/embed/dub"}]}"#;
        assert_eq!(
            parse_languages(body, StreamingMode::Dub).unwrap().as_str(),
            "https://anidb.app/embed/dub"
        );
        assert!(parse_languages(body, StreamingMode::Sub).is_ok());
        assert!(
            parse_languages(
                r#"{"languages":[{"code":"jpn","embed_url":"https://anidb.app/embed/sub"}]}"#,
                StreamingMode::Dub
            )
            .is_err()
        );
        assert!(parse_languages(r#"{"languages":[]}"#, StreamingMode::Sub).is_err());
    }

    #[test]
    fn resolves_relative_hls_variants_and_quality() {
        let master = Url::parse("https://cdn.example/path/master.m3u8").unwrap();
        let body = "#EXTM3U\n#EXT-X-STREAM-INF:RESOLUTION=1280x720\n720/index.m3u8\n#EXT-X-STREAM-INF:RESOLUTION=1920x1080\nhttps://video.example/1080.m3u8\n";
        let variants = parse_master_playlist(body, &master).unwrap();
        assert_eq!(
            select_variant(&variants, QualityPreference::Best)
                .unwrap()
                .height,
            1080
        );
        assert_eq!(
            select_variant(&variants, QualityPreference::Worst)
                .unwrap()
                .height,
            720
        );
        assert_eq!(
            select_variant(&variants, QualityPreference::Height(720))
                .unwrap()
                .url
                .as_str(),
            "https://cdn.example/path/720/index.m3u8"
        );
        assert_eq!(
            select_variant(&variants, QualityPreference::Height(480))
                .unwrap()
                .height,
            1080
        );
    }

    #[test]
    fn extracts_first_master_playlist() {
        let embed = "<script>player.setup({file: 'https://cdn.example/master.m3u8'});</script>";
        assert_eq!(
            extract_master_url(embed).unwrap().as_str(),
            "https://cdn.example/master.m3u8"
        );
    }
}
