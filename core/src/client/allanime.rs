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

use aes::Aes256;
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::{Engine as _, engine::general_purpose};
use ctr::cipher::{KeyIvInit, StreamCipher};
use sha2::{Digest, Sha256};
use std::str;

use serde::Deserialize;
use serde_json::Value;
use url::Url;

use crate::{CoreResult, error::Error, source::ALLANIME_API_ENDPOINT};

pub(crate) const ALLANIME_TRANSLATION: &str = "sub";
pub(crate) const ALLANIME_REFERER: &str = "https://allmanga.to";
pub(crate) const ALLANIME_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:109.0) Gecko/20100101 Firefox/121.0";

// AllAnime's API currently requires requests to look like they came from a valid frontend.
// ani-cli uses https://youtu-chan.com for this purpose.
pub(crate) const ALLANIME_API_ORIGIN: &str = "https://youtu-chan.com";

// Persisted query hash for `ALLANIME_EPISODE_EMBED_GQL`.
// See: https://github.com/pystardust/ani-cli/commit/6803b8a15faafa41cb79271e9a4f7f9c70a53651
pub(crate) const ALLANIME_EPISODE_EMBED_PERSISTED_HASH: &str =
    "d405d0edd690624b66baba3068e0edc3ac90f1597d898a1ec8db4e5c43c00fec";
const ALLANIME_EMBED_HOST: &str = "https://allanime.day";
pub(crate) const ALLANIME_SEARCH_GQL: &str = "query ($search: SearchInput $limit: Int $page: Int $translationType: VaildTranslationTypeEnumType $countryOrigin: VaildCountryOriginEnumType ) { shows( search: $search limit: $limit page: $page translationType: $translationType countryOrigin: $countryOrigin ) { edges { _id name availableEpisodes __typename } }}";
pub(crate) const ALLANIME_EPISODES_GQL: &str =
    "query ($showId: String!) { show( _id: $showId ) { _id availableEpisodesDetail }}";
pub(crate) const ALLANIME_EPISODE_EMBED_GQL: &str = "query ($showId: String!, $translationType: VaildTranslationTypeEnumType!, $episodeString: String!) { episode( showId: $showId translationType: $translationType episodeString: $episodeString ) { episodeString sourceUrls }}";

#[derive(Debug, Deserialize, Default)]
pub(crate) struct AllAnimeSearchResponse {
    #[serde(default)]
    pub(crate) data: AllAnimeSearchData,
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct AllAnimeSearchData {
    #[serde(default)]
    pub(crate) shows: AllAnimeShows,
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct AllAnimeShows {
    #[serde(default)]
    pub(crate) edges: Vec<AllAnimeShowEdge>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AllAnimeShowEdge {
    #[serde(rename = "_id")]
    pub(crate) id: String,
    pub(crate) name: String,
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct AllAnimeEpisodesResponse {
    #[serde(default)]
    pub(crate) data: AllAnimeEpisodesData,
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct AllAnimeEpisodesData {
    pub(crate) show: Option<AllAnimeShowDetail>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AllAnimeShowDetail {
    #[serde(rename = "_id")]
    pub(crate) id: String,
    #[serde(rename = "availableEpisodesDetail", default)]
    pub(crate) available_episodes_detail: AllAnimeEpisodeDetail,
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct AllAnimeEpisodeDetail {
    #[serde(default)]
    pub(crate) sub: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct AllAnimeEpisodeEmbedResponse {
    #[serde(default)]
    pub(crate) data: AllAnimeEpisodeData,
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct AllAnimeEpisodeData {
    pub(crate) episode: Option<AllAnimeEpisodeInfo>,
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct AllAnimeEpisodeInfo {
    #[serde(rename = "sourceUrls", default)]
    pub(crate) source_urls: Vec<AllAnimeSourceUrl>,
}

#[derive(Debug, Deserialize, Clone)]
pub(crate) struct AllAnimeSourceUrl {
    #[serde(rename = "sourceUrl")]
    pub(crate) source_url: String,
    #[serde(rename = "sourceName")]
    pub(crate) source_name: Option<String>,
}

#[derive(Debug, Clone)]
struct StreamCandidate {
    url: String,
    resolution: Option<u32>,
}

pub(crate) fn build_graphql_url(_query: &str, _variables: &Value) -> Url {
    Url::parse(ALLANIME_API_ENDPOINT).expect("valid AllAnime API endpoint")
}

pub(crate) fn maybe_decrypt_response_data(mut response: Value) -> CoreResult<Value> {
    // Newer payload format (used by ani-cli):
    // 1 byte prefix + 12 byte IV + ciphertext + 16 byte trailer.
    const MIN_CTR_LEN: usize = 1 + 12 + 16;
    // Older payload format we previously supported (AES-GCM):
    const MIN_GCM_LEN: usize = 12 + 16;

    let Some(data_value) = response.get_mut("data") else {
        return Ok(response);
    };

    let Some(tobeparsed) = data_value
        .get("tobeparsed")
        .and_then(Value::as_str)
        .map(str::to_owned)
    else {
        return Ok(response);
    };

    let decoded =
        general_purpose::STANDARD
            .decode(&tobeparsed)
            .map_err(|_| Error::StreamResolution {
                message: "failed to base64 decode AllAnime payload".to_string(),
            })?;

    // Try the ani-cli format first (AES-256-CTR with a 12-byte IV + fixed counter suffix).
    if decoded.len() >= MIN_CTR_LEN {
        type Aes256Ctr = ctr::Ctr128BE<Aes256>;

        let key_bytes = Sha256::digest(b"Xot36i3lK3:v1");
        let mut iv = [0u8; 16];
        iv[..12].copy_from_slice(&decoded[1..13]);
        iv[12..].copy_from_slice(&[0, 0, 0, 2]);

        let mut plaintext = decoded[13..decoded.len().saturating_sub(16)].to_vec();
        let mut cipher = Aes256Ctr::new(key_bytes.as_slice().into(), (&iv).into());
        cipher.apply_keystream(&mut plaintext);

        if let Ok(decrypted_json) = serde_json::from_slice::<Value>(&plaintext) {
            *data_value = decrypted_json;
            return Ok(response);
        }
    }

    // Fall back to the older AES-GCM approach (kept for compatibility with any older responses).
    if decoded.len() < MIN_GCM_LEN {
        return Err(Error::StreamResolution {
            message: format!(
                "encrypted AllAnime payload too short ({decoded_len} bytes)",
                decoded_len = decoded.len()
            ),
        }
        .into());
    }

    let (iv_bytes, ciphertext) = decoded.split_at(12);
    let key_bytes = Sha256::digest(b"SimtVuagFbGR2K7P");
    let cipher = Aes256Gcm::new_from_slice(&key_bytes).map_err(|_| Error::StreamResolution {
        message: "invalid AES key length for AllAnime payload".to_string(),
    })?;
    let nonce = Nonce::from_slice(iv_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| Error::StreamResolution {
            message: "failed to decrypt AllAnime payload".to_string(),
        })?;

    let decrypted_json =
        serde_json::from_slice::<Value>(&plaintext).map_err(|_| Error::StreamResolution {
            message: "failed to parse decrypted AllAnime payload".to_string(),
        })?;

    *data_value = decrypted_json;
    Ok(response)
}

pub(crate) fn build_embed_url(decoded: &str) -> CoreResult<Url> {
    let trimmed = decoded.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        let url = Url::parse(trimmed).map_err(|source| Error::InvalidUrl {
            template: trimmed.to_string(),
            source,
        })?;
        Ok(url)
    } else {
        let full = format!("{ALLANIME_EMBED_HOST}{trimmed}");
        let url = Url::parse(&full).map_err(|source| Error::InvalidUrl {
            template: full,
            source,
        })?;
        Ok(url)
    }
}

pub(crate) fn select_source_url(sources: &[AllAnimeSourceUrl]) -> Option<&AllAnimeSourceUrl> {
    let mut first_encoded = None;
    let mut encoded_default = None;

    for source in sources {
        if source.source_url.trim_start().starts_with('-') {
            if first_encoded.is_none() {
                first_encoded = Some(source);
            }

            if source.source_name.as_deref() == Some("Default") {
                encoded_default = Some(source);
                break;
            }
        }
    }

    encoded_default
        .or(first_encoded)
        .or_else(|| {
            sources
                .iter()
                .find(|source| source.source_name.as_deref() == Some("Default"))
        })
        .or_else(|| sources.first())
}

pub(crate) fn ordered_source_urls(sources: &[AllAnimeSourceUrl]) -> Vec<&AllAnimeSourceUrl> {
    if sources.is_empty() {
        return Vec::new();
    }

    let mut ordered = Vec::with_capacity(sources.len());
    if let Some(preferred) = select_source_url(sources) {
        ordered.push(preferred);
        let preferred_ptr = std::ptr::from_ref(preferred);
        for source in sources {
            if !std::ptr::eq(source, preferred_ptr) {
                ordered.push(source);
            }
        }
    } else {
        ordered.extend(sources.iter());
    }

    ordered
}

pub(crate) fn split_episode_id(episode_id: &str) -> CoreResult<(String, String)> {
    let mut parts = episode_id.splitn(2, ':');
    let show_id = parts.next();
    let episode = parts.next();

    match (show_id, episode) {
        (Some(show), Some(ep)) if !show.is_empty() && !ep.is_empty() => {
            Ok((show.to_string(), ep.to_string()))
        }
        _ => Err(Error::EpisodeIdParse {
            episode_id: episode_id.to_string(),
        }
        .into()),
    }
}

pub(crate) fn decode_source_url(encoded: &str) -> CoreResult<String> {
    let normalize_clock = |value: &str| value.replace("/clock", "/clock.json");

    if encoded.starts_with("http://") || encoded.starts_with("https://") {
        return Ok(normalize_clock(encoded));
    }

    if encoded.starts_with('/') {
        return Ok(normalize_clock(encoded));
    }

    let stripped = if let Some(rest) = encoded.strip_prefix("--") {
        rest
    } else if let Some(rest) = encoded.strip_prefix('-') {
        rest
    } else {
        return Err(Error::StreamResolution {
            message: "unsupported source URL format".to_string(),
        }
        .into());
    };

    if stripped.len() % 2 != 0 {
        return Err(Error::StreamResolution {
            message: "invalid encoded source length".to_string(),
        }
        .into());
    }

    let mut decoded = String::with_capacity(stripped.len() / 2);
    for chunk in stripped.as_bytes().chunks(2) {
        let pair = str::from_utf8(chunk).map_err(|_| Error::StreamResolution {
            message: "encoded source not utf-8".to_string(),
        })?;
        let pair_lower = pair.to_ascii_lowercase();
        let ch = decode_pair(&pair_lower).ok_or_else(|| Error::StreamResolution {
            message: format!("unknown source code {pair}"),
        })?;
        decoded.push(ch);
    }

    Ok(normalize_clock(&decoded))
}

#[allow(clippy::too_many_lines)]
fn decode_pair(pair: &str) -> Option<char> {
    match pair {
        "79" => Some('A'),
        "7a" => Some('B'),
        "7b" => Some('C'),
        "7c" => Some('D'),
        "7d" => Some('E'),
        "7e" => Some('F'),
        "7f" => Some('G'),
        "70" => Some('H'),
        "71" => Some('I'),
        "72" => Some('J'),
        "73" => Some('K'),
        "74" => Some('L'),
        "75" => Some('M'),
        "76" => Some('N'),
        "77" => Some('O'),
        "68" => Some('P'),
        "69" => Some('Q'),
        "6a" => Some('R'),
        "6b" => Some('S'),
        "6c" => Some('T'),
        "6d" => Some('U'),
        "6e" => Some('V'),
        "6f" => Some('W'),
        "60" => Some('X'),
        "61" => Some('Y'),
        "62" => Some('Z'),
        "59" => Some('a'),
        "5a" => Some('b'),
        "5b" => Some('c'),
        "5c" => Some('d'),
        "5d" => Some('e'),
        "5e" => Some('f'),
        "5f" => Some('g'),
        "50" => Some('h'),
        "51" => Some('i'),
        "52" => Some('j'),
        "53" => Some('k'),
        "54" => Some('l'),
        "55" => Some('m'),
        "56" => Some('n'),
        "57" => Some('o'),
        "48" => Some('p'),
        "49" => Some('q'),
        "4a" => Some('r'),
        "4b" => Some('s'),
        "4c" => Some('t'),
        "4d" => Some('u'),
        "4e" => Some('v'),
        "4f" => Some('w'),
        "40" => Some('x'),
        "41" => Some('y'),
        "42" => Some('z'),
        "08" => Some('0'),
        "09" => Some('1'),
        "0a" => Some('2'),
        "0b" => Some('3'),
        "0c" => Some('4'),
        "0d" => Some('5'),
        "0e" => Some('6'),
        "0f" => Some('7'),
        "00" => Some('8'),
        "01" => Some('9'),
        "15" => Some('-'),
        "16" => Some('.'),
        "67" => Some('_'),
        "46" => Some('~'),
        "02" => Some(':'),
        "17" => Some('/'),
        "07" => Some('?'),
        "1b" => Some('#'),
        "63" => Some('['),
        "65" => Some(']'),
        "78" => Some('@'),
        "19" => Some('!'),
        "1c" => Some('$'),
        "1e" => Some('&'),
        "10" => Some('('),
        "11" => Some(')'),
        "12" => Some('*'),
        "13" => Some('+'),
        "14" => Some(','),
        "03" => Some(';'),
        "05" => Some('='),
        "1d" => Some('%'),
        _ => None,
    }
}

pub(crate) fn select_stream_url(value: &Value) -> CoreResult<String> {
    let mut candidates = Vec::new();
    collect_stream_candidates(value, &mut candidates);
    if candidates.is_empty() {
        return Err(Error::StreamResolution {
            message: "no playable links found".to_string(),
        }
        .into());
    }

    let mut best = candidates.remove(0);
    for candidate in candidates {
        if prefers_candidate(&candidate, &best) {
            best = candidate;
        }
    }

    Ok(best.url)
}

fn collect_stream_candidates(value: &Value, candidates: &mut Vec<StreamCandidate>) {
    match value {
        Value::Object(map) => {
            if let (Some(link), Some(resolution)) = (map.get("link"), map.get("resolutionStr")) {
                if let (Some(link), Some(resolution)) = (link.as_str(), resolution.as_str()) {
                    candidates.push(StreamCandidate {
                        url: link.to_string(),
                        resolution: parse_resolution(resolution),
                    });
                }
            } else if let Some(url) = map.get("url").and_then(Value::as_str) {
                if map
                    .get("hardsub_lang")
                    .and_then(Value::as_str)
                    .is_some_and(|lang| lang.eq_ignore_ascii_case("en-US"))
                {
                    candidates.push(StreamCandidate {
                        url: url.to_string(),
                        resolution: None,
                    });
                }
            }

            for value in map.values() {
                collect_stream_candidates(value, candidates);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_stream_candidates(item, candidates);
            }
        }
        _ => {}
    }
}

fn prefers_candidate(candidate: &StreamCandidate, current: &StreamCandidate) -> bool {
    match (candidate.resolution, current.resolution) {
        (Some(candidate_res), Some(current_res)) => candidate_res > current_res,
        (Some(_), None) => true,
        (None, _) => false,
    }
}

fn parse_resolution(text: &str) -> Option<u32> {
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

pub(crate) fn parse_episode_number(label: &str) -> u32 {
    let segment = label.split('.').next().unwrap_or(label);
    segment.parse().unwrap_or(0)
}
