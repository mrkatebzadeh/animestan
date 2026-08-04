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
use std::collections::HashSet;

use crate::{
    CoreResult,
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
    let response: EpisodesResponse =
        serde_json::from_str(body).map_err(|source| Error::ResponseParse {
            url: format!("{BASE_URL}/api/frontend/anime/{anime_id}/episodes"),
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

#[cfg(test)]
mod tests {
    use super::{anime_numeric_id, parse_episodes, parse_search};

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
}
