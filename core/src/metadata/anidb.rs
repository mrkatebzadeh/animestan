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

use std::collections::HashSet;

use reqwest::blocking::Client;
use reqwest::header::{REFERER, USER_AGENT};
use scraper::{Html, Selector};
use serde::Deserialize;
use url::Url;

use crate::{AppConfig, client::anidb as anidb_client, error::Error};

use super::{AnimeMetadata, MetadataCache, MetadataProvider, MetadataSource, normalize_query};

#[derive(Debug, Deserialize)]
struct JsonLd {
    #[serde(default)]
    name: String,
    description: Option<String>,
    #[serde(alias = "thumbnail")]
    image: Option<String>,
    #[serde(default, alias = "genres")]
    genre: Vec<String>,
}

pub(super) fn parse_detail(html: &str, anime_id: &str) -> Result<AnimeMetadata, Error> {
    let document = Html::parse_document(html);
    let canonical_selector = Selector::parse(r#"link[rel="canonical"]"#).expect("valid selector");
    let json_ld_selector =
        Selector::parse(r#"script[type="application/ld+json"]"#).expect("valid selector");
    let status_selector = Selector::parse(r#"a[href^="/browse?status="]"#).expect("valid selector");
    let score_selector = Selector::parse("span.badge-gray").expect("valid selector");
    let season_selector = Selector::parse(r#"a[href^="/browse?season="]"#).expect("valid selector");
    let studio_selector = Selector::parse(r#"a[href^="/studios/"]"#).expect("valid selector");
    let trailer_selector =
        Selector::parse(r#"a[href*="youtube.com/watch"]"#).expect("valid selector");

    let source_url = document.select(&canonical_selector).find_map(|link| {
        let href = link.value().attr("href")?;
        let url = Url::parse(href).ok()?;
        if url.host_str() != Some("anidb.app") {
            return None;
        }
        let canonical_id = url.path().strip_prefix("/anime/")?;
        let (slug, numeric_id) = canonical_id.rsplit_once('-')?;
        if slug.is_empty()
            || numeric_id.is_empty()
            || !numeric_id
                .chars()
                .all(|character| character.is_ascii_digit())
            || canonical_id != anime_id
        {
            return None;
        }
        Some(href.to_owned())
    });
    let json_ld = document.select(&json_ld_selector).find_map(|script| {
        serde_json::from_str::<JsonLd>(&script.inner_html())
            .ok()
            .filter(|json_ld| !json_ld.name.trim().is_empty())
    });

    let (Some(source_url), Some(json_ld)) = (source_url, json_ld) else {
        return Err(Error::MetadataNotFound {
            query: anime_id.to_string(),
        });
    };

    let status = document
        .select(&status_selector)
        .map(|element| element.text().collect::<String>().trim().to_string())
        .find(|status| !status.is_empty());
    let score = document.select(&score_selector).find_map(|element| {
        element
            .text()
            .collect::<String>()
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|score| (0.0..=10.0).contains(score))
    });
    let (season, year) = document
        .select(&season_selector)
        .map(|element| element.text().collect::<String>().trim().to_string())
        .find(|text| !text.is_empty())
        .map_or((None, None), |text| parse_season_year(&text));

    let mut studios = Vec::new();
    let mut seen_studios = HashSet::new();
    for studio in document
        .select(&studio_selector)
        .map(|element| element.text().collect::<String>().trim().to_string())
        .filter(|studio| !studio.is_empty())
    {
        if seen_studios.insert(studio.clone()) {
            studios.push(studio);
        }
    }

    let trailer_url = document
        .select(&trailer_selector)
        .find_map(|link| link.value().attr("href").map(str::to_owned));

    Ok(AnimeMetadata {
        title: json_ld.name,
        synopsis: json_ld.description,
        score,
        genres: json_ld.genre,
        studios,
        status,
        season,
        year,
        trailer_url,
        image_url: json_ld.image,
        source_url,
        source: MetadataSource::AniDb,
    })
}

fn parse_season_year(text: &str) -> (Option<String>, Option<u16>) {
    let mut words = text.split_whitespace();
    let season = words.next().map(str::to_owned);
    let year = words.find_map(|word| word.parse::<u16>().ok());
    (season, year)
}

pub struct AniDbMetadataProvider {
    client: Client,
    cache: MetadataCache,
}

impl MetadataProvider for AniDbMetadataProvider {
    fn fetch_by_query(&self, query: &str) -> Result<AnimeMetadata, Error> {
        let key = format!("anidb:query:{}", normalize_query(query));
        if let Some(metadata) = self.cache.get(&key)? {
            return Ok(metadata);
        }
        let metadata = self.fetch_by_query_uncached(query)?;
        self.cache.insert(key, metadata.clone())?;
        Ok(metadata)
    }
}

impl AniDbMetadataProvider {
    #[must_use]
    pub fn new(client: Client) -> Self {
        Self::with_cache(
            client,
            MetadataCache::new(AppConfig::default().metadata_cache_path()),
        )
    }

    pub(super) fn with_cache(client: Client, cache: MetadataCache) -> Self {
        Self { client, cache }
    }

    /// # Errors
    ///
    /// Returns an error if the `AniDB` request or detail page cannot be resolved.
    pub fn fetch_by_id(&self, id: &str, query: &str) -> Result<AnimeMetadata, Error> {
        let key = format!("anidb:id:{id}");
        if let Some(metadata) = self.cache.get(&key)? {
            return Ok(metadata);
        }
        let metadata = self.fetch_detail(id, query)?;
        self.cache.insert(key, metadata.clone())?;
        Ok(metadata)
    }

    pub(super) fn refresh_by_query(&self, query: &str) -> Result<AnimeMetadata, Error> {
        let key = format!("anidb:query:{}", normalize_query(query));
        let metadata = self.fetch_by_query_uncached(query)?;
        self.cache.insert(key, metadata.clone())?;
        Ok(metadata)
    }

    /// # Errors
    ///
    /// Returns an error if the `AniDB` request or detail page cannot be resolved.
    pub fn refresh_by_id(&self, id: &str, query: &str) -> Result<AnimeMetadata, Error> {
        let key = format!("anidb:id:{id}");
        let metadata = self.fetch_detail(id, query)?;
        self.cache.insert(key, metadata.clone())?;
        Ok(metadata)
    }

    fn fetch_by_query_uncached(&self, query: &str) -> Result<AnimeMetadata, Error> {
        let search_url = search_url(query)?;
        let body = self.fetch_url(&search_url)?;
        let entries = anidb_client::parse_search(&body, "anidb").map_err(|error| {
            error
                .downcast::<Error>()
                .unwrap_or_else(|_| Error::MetadataNotFound {
                    query: query.to_string(),
                })
        })?;
        let entry = entries
            .into_iter()
            .next()
            .ok_or_else(|| Error::MetadataNotFound {
                query: query.to_string(),
            })?;
        self.fetch_detail(&entry.id, query)
    }

    fn fetch_detail(&self, id: &str, query: &str) -> Result<AnimeMetadata, Error> {
        let url = detail_url(id)?;
        let body = self.fetch_url(&url)?;
        parse_detail(&body, id).map_err(|error| match error {
            Error::MetadataNotFound { .. } => Error::MetadataNotFound {
                query: query.to_string(),
            },
            error => error,
        })
    }

    fn fetch_url(&self, url: &Url) -> Result<String, Error> {
        let response = self
            .client
            .get(url.clone())
            .header(USER_AGENT, anidb_client::USER_AGENT_VALUE)
            .header(REFERER, anidb_client::BASE_URL)
            .send()
            .map_err(|source| Error::HttpRequest {
                url: url.to_string(),
                source,
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(Error::HttpStatus {
                url: url.to_string(),
                status: status.as_u16(),
            });
        }
        response.text().map_err(|source| Error::HttpBodyParse {
            url: url.to_string(),
            source,
        })
    }
}

fn search_url(query: &str) -> Result<Url, Error> {
    let mut url = Url::parse(anidb_client::BASE_URL).map_err(|source| Error::InvalidUrl {
        template: anidb_client::BASE_URL.to_string(),
        source,
    })?;
    url.set_path("/browse");
    url.query_pairs_mut().append_pair("q", query);
    Ok(url)
}

fn detail_url(id: &str) -> Result<Url, Error> {
    let mut url = Url::parse(anidb_client::BASE_URL).map_err(|source| Error::InvalidUrl {
        template: anidb_client::BASE_URL.to_string(),
        source,
    })?;
    url.set_path(&format!("/anime/{id}"));
    Ok(url)
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    use reqwest::blocking::Client;
    use reqwest::header::{HeaderMap, HeaderValue};
    use url::Url;

    use super::{AniDbMetadataProvider, parse_detail};
    use crate::metadata::{MetadataCache, MetadataSource};

    const DETAIL_HTML: &str = r#"
<html><head>
<link rel="canonical" href="https://anidb.app/anime/ippon-again-20">
<script type="application/ld+json">{
  "@type":"TVSeries","name":"Ippon again!","description":"Judo club story.",
  "image":"https://cdn.example/20.jpg","genre":["Sports"]
}</script></head><body>
<a href="/browse?status=Finished+Airing">Finished Airing</a>
<span class="badge badge-gray">7.1</span>
<a href="/browse?season=winter&year=2023">Winter 2023</a>
<a href="/studios/8">Bakken Record</a>
<a href="https://www.youtube.com/watch?v=trailer">Trailer</a>
</body></html>
"#;

    #[test]
    fn parses_rich_anidb_metadata() {
        let metadata = parse_detail(DETAIL_HTML, "ippon-again-20").expect("metadata");
        assert_eq!(metadata.title, "Ippon again!");
        assert_eq!(metadata.synopsis.as_deref(), Some("Judo club story."));
        assert_eq!(metadata.score, Some(7.1));
        assert_eq!(metadata.genres, ["Sports"]);
        assert_eq!(metadata.studios, ["Bakken Record"]);
        assert_eq!(metadata.status.as_deref(), Some("Finished Airing"));
        assert_eq!(metadata.season.as_deref(), Some("Winter"));
        assert_eq!(metadata.year, Some(2023));
        assert_eq!(
            metadata.trailer_url.as_deref(),
            Some("https://www.youtube.com/watch?v=trailer")
        );
        assert_eq!(
            metadata.image_url.as_deref(),
            Some("https://cdn.example/20.jpg")
        );
        assert_eq!(
            metadata.source_url,
            "https://anidb.app/anime/ippon-again-20"
        );
        assert_eq!(metadata.source, MetadataSource::AniDb);
    }

    #[test]
    fn rejects_non_anime_detail_html() {
        assert!(parse_detail("<html><title>Not found</title></html>", "missing-1").is_err());
    }

    #[test]
    fn ignores_invalid_scores_and_deduplicates_studios() {
        let html = r#"
            <link rel="canonical" href="https://anidb.app/anime/example-1">
            <script type="application/ld+json">{"name":"Example"}</script>
            <span class="badge-gray">11.0</span>
            <span class="badge-gray">0.0</span>
            <a href="/studios/1">Studio</a>
            <a href="/studios/2">Studio</a>
        "#;

        let metadata = parse_detail(html, "example-1").expect("metadata");

        assert_eq!(metadata.score, Some(0.0));
        assert_eq!(metadata.studios, ["Studio"]);
    }

    #[test]
    fn rejects_detail_without_canonical_url() {
        let html = r#"<script type="application/ld+json">{"name":"Example"}</script>"#;
        let error = parse_detail(html, "missing-1").expect_err("missing canonical URL");

        assert!(matches!(
            error,
            crate::error::Error::MetadataNotFound { query } if query == "missing-1"
        ));
    }

    #[test]
    fn rejects_invalid_canonical_urls() {
        for canonical in [
            "/anime/example-1",
            "https://example.com/anime/example-1",
            "https://anidb.app/anime/other-1",
            "https://anidb.app/anime/example-no-id",
            "https://anidb.app/browse/example-1",
        ] {
            let html = format!(
                r#"<link rel="canonical" href="{canonical}">
                <script type="application/ld+json">{{"name":"Example"}}</script>"#
            );
            let error = parse_detail(&html, "example-1").expect_err(canonical);

            assert!(matches!(
                error,
                crate::error::Error::MetadataNotFound { query } if query == "example-1"
            ));
        }
    }

    #[test]
    fn preserves_injected_client_defaults() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let address = listener.local_addr().expect("test server address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut request = Vec::new();
            let mut buffer = [0; 1024];
            loop {
                let read = stream.read(&mut buffer).expect("read request");
                request.extend_from_slice(&buffer[..read]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .expect("write response");
            String::from_utf8(request)
                .expect("request is UTF-8")
                .to_lowercase()
        });

        let mut headers = HeaderMap::new();
        headers.insert("x-client-setting", HeaderValue::from_static("preserved"));
        let client = Client::builder()
            .default_headers(headers)
            .build()
            .expect("build client");
        let provider = AniDbMetadataProvider::with_cache(
            client,
            MetadataCache::new("unused-cache.json".into()),
        );
        let url = Url::parse(&format!("http://{address}/")).expect("test URL");

        assert_eq!(provider.fetch_url(&url).expect("request succeeds"), "ok");
        let request = server.join().expect("server thread");
        assert!(request.contains("x-client-setting: preserved"));
        assert!(request.contains("user-agent: mozilla/5.0"));
        assert!(request.contains("referer: https://anidb.app"));
    }
}
