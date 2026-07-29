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

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::{Engine as _, engine::general_purpose};
use reqwest::blocking::Client as BlockingHttpClient;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ORIGIN, REFERER, USER_AGENT};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::str;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use serde_json::Value;
use url::Url;

use crate::{CoreResult, error::Error, source::ALLANIME_API_ENDPOINT};

pub(crate) const ALLANIME_TRANSLATION: &str = "sub";
pub(crate) const ALLANIME_REFERER: &str = "https://mkissa.to";
pub(crate) const ALLANIME_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:109.0) Gecko/20100101 Firefox/150.0";

// AllAnime's bootstrap endpoint currently requires requests to look like they came from the mkissa frontend.
pub(crate) const ALLANIME_API_ORIGIN: &str = "https://mkissa.to";

// Persisted query hash for `ALLANIME_EPISODE_EMBED_GQL`.
// ani-cli master currently uses this hash for the episode source query.
pub(crate) const ALLANIME_EPISODE_EMBED_PERSISTED_HASH: &str =
    "f4662f4b7510b26795dd53ef824a0bf1740fbbc5d1273fab18222ac831bca8d0";
const ALLANIME_BUILD_MASK_BLOBS: [&str; 4] = [
    "12eJyE2wzfY=",
    "nWIlTqF9f5E=",
    "7f6CmXtAgpY=",
    "oR/792BJ+Sc=",
];
const ALLANIME_BOOTSTRAP_BUCKET_MS: u64 = 3 * 24 * 60 * 60 * 1000;
const ALLANIME_BOOTSTRAP_GRACE_MS: u64 = 24 * 60 * 60 * 1000;
const ALLANIME_BOOTSTRAP_URL: &str = "https://api.mkissa.net/client-crypto/v1/bootstrap";
const ALLANIME_EMBED_HOST: &str = "https://allanime.day";
pub(crate) const ALLANIME_SEARCH_GQL: &str = "query ($search: SearchInput $limit: Int $page: Int $translationType: VaildTranslationTypeEnumType $countryOrigin: VaildCountryOriginEnumType ) { shows( search: $search limit: $limit page: $page translationType: $translationType countryOrigin: $countryOrigin ) { edges { _id name availableEpisodes __typename } }}";
pub(crate) const ALLANIME_EPISODES_GQL: &str =
    "query ($showId: String!) { show( _id: $showId ) { _id availableEpisodesDetail }}";
pub(crate) const ALLANIME_EPISODE_EMBED_GQL: &str = "query ($showId: String!, $translationType: VaildTranslationTypeEnumType!, $episodeString: String!) { episode( showId: $showId translationType: $translationType episodeString: $episodeString ) { episodeString sourceUrls }}";

#[derive(Debug, Clone)]
pub(crate) struct AllAnimeKeyMaterial {
    pub(crate) build_id: String,
    pub(crate) content_lane: String,
    pub(crate) epoch: u64,
    pub(crate) key: [u8; 32],
}

#[derive(Debug, Deserialize)]
struct AllAnimeBootstrapResponse {
    epoch: u64,
    #[serde(rename = "partB")]
    part_b: String,
    #[serde(default)]
    k: Option<String>,
}

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

pub(crate) fn build_aa_req(
    query_hash: &str,
    build_id: &str,
    epoch: u64,
    content_lane: &str,
    key: &[u8; 32],
    now_ms: u64,
) -> CoreResult<String> {
    let ts = (now_ms / 300_000) * 300_000;
    let iv_digest = Sha256::digest(format!("{epoch}:{query_hash}:{ts}").as_bytes());
    let plaintext = format!(
        "{{\"v\":1,\"ts\":{ts},\"epoch\":{epoch},\"buildId\":\"{build_id}\",\"qh\":\"{query_hash}\",\"k\":\"{content_lane}\"}}"
    );
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| Error::StreamResolution {
        message: "invalid AllAnime request key length".to_string(),
    })?;
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&iv_digest[..12]), plaintext.as_bytes())
        .map_err(|_| Error::StreamResolution {
            message: "failed to encrypt AllAnime aaReq".to_string(),
        })?;

    let mut payload = Vec::with_capacity(1 + 12 + ciphertext.len());
    payload.push(1);
    payload.extend_from_slice(&iv_digest[..12]);
    payload.extend_from_slice(&ciphertext);
    Ok(general_purpose::STANDARD.encode(payload))
}

pub(crate) fn fetch_allanime_key_material() -> CoreResult<AllAnimeKeyMaterial> {
    let client = allanime_http_client()?;
    // ponytail: fixed to the current mkissa build tuple; refresh when the site rotates it.
    let build_id = "75".to_string();
    let content_lane = "k7".to_string();
    let build_key = generate_allanime_build_key(&build_id)?;

    let bootstrap_response =
        fetch_allanime_bootstrap(&client, &build_id, &content_lane, &build_key)?;
    let key = derive_allanime_key(&bootstrap_response.part_b, &build_key)?;

    Ok(AllAnimeKeyMaterial {
        build_id,
        content_lane,
        epoch: bootstrap_response.epoch,
        key,
    })
}

fn allanime_http_client() -> CoreResult<BlockingHttpClient> {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static(ALLANIME_USER_AGENT));
    headers.insert(REFERER, HeaderValue::from_static(ALLANIME_REFERER));
    Ok(BlockingHttpClient::builder()
        .default_headers(headers)
        .build()
        .map_err(Error::HttpClient)?)
}

fn generate_allanime_build_key(build_id: &str) -> CoreResult<[u8; 32]> {
    if build_id.is_empty() {
        return Err(Error::StreamResolution {
            message: "empty AllAnime build ID".to_string(),
        }
        .into());
    }

    let mut out = [0u8; 32];
    let mut cursor = 0usize;

    for (block, b64) in ALLANIME_BUILD_MASK_BLOBS.iter().enumerate() {
        let decoded =
            general_purpose::STANDARD
                .decode(b64)
                .map_err(|_| Error::StreamResolution {
                    message: "failed to decode AllAnime build mask blob".to_string(),
                })?;

        if decoded.len() != 8 {
            return Err(Error::StreamResolution {
                message: format!(
                    "unexpected AllAnime build mask blob length ({})",
                    decoded.len()
                ),
            }
            .into());
        }

        for (byte, embedded) in decoded.iter().enumerate() {
            let index = block * 8 + byte;
            let build_byte = build_id.as_bytes()[index % build_id.len()];
            let build_mask_byte =
                build_byte ^ u8::try_from(((index * 17) + 31) & 0xff).expect("masked build byte");
            let tweak =
                u8::try_from(((block * 41) + (byte * 7)) & 0xff).expect("masked tweak byte");
            out[cursor] = embedded ^ build_mask_byte ^ tweak;
            cursor += 1;
        }
    }

    Ok(out)
}

#[allow(clippy::too_many_lines)]
fn fetch_allanime_bootstrap(
    client: &BlockingHttpClient,
    build_id: &str,
    content_lane: &str,
    build_key: &[u8; 32],
) -> CoreResult<AllAnimeBootstrapResponse> {
    let referer_host = Url::parse(ALLANIME_REFERER)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .unwrap_or_else(|| "mkissa.to".to_string());
    let key_group = allanime_key_group(&referer_host);
    let now_ms = u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|source| Error::StreamResolution {
                message: format!("system clock before unix epoch: {source}"),
            })?
            .as_millis(),
    )
    .map_err(|_| Error::StreamResolution {
        message: "system clock too far in the future".to_string(),
    })?;

    let mut bootstrap_url =
        Url::parse(ALLANIME_BOOTSTRAP_URL).map_err(|source| Error::InvalidUrl {
            template: ALLANIME_BOOTSTRAP_URL.to_string(),
            source,
        })?;
    {
        let mut pairs = bootstrap_url.query_pairs_mut();
        pairs.append_pair("buildId", build_id);
        pairs.append_pair("k", content_lane);
    }

    let mut last_error = None;
    for epoch in bootstrap_epoch_candidates(now_ms) {
        let x_aa_boot = build_allanime_boot_token(
            build_key,
            build_id,
            epoch,
            key_group,
            &referer_host,
            content_lane,
        );

        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("x-build-id"),
            HeaderValue::from_str(build_id).map_err(|source| Error::StreamResolution {
                message: format!("invalid AllAnime build ID: {source}"),
            })?,
        );
        headers.insert(
            HeaderName::from_static("x-aa-boot"),
            HeaderValue::from_str(&x_aa_boot).map_err(|source| Error::StreamResolution {
                message: format!("invalid AllAnime bootstrap token: {source}"),
            })?,
        );
        headers.insert(REFERER, HeaderValue::from_static(ALLANIME_REFERER));
        headers.insert(ORIGIN, HeaderValue::from_static(ALLANIME_API_ORIGIN));

        let response = client.get(bootstrap_url.clone()).headers(headers).send();
        let response = match response {
            Ok(response) if response.status().is_success() => response,
            Ok(response) => {
                last_error = Some(
                    Error::HttpStatus {
                        url: bootstrap_url.to_string(),
                        status: response.status().as_u16(),
                    }
                    .into(),
                );
                continue;
            }
            Err(source) => {
                last_error = Some(
                    Error::HttpRequest {
                        url: bootstrap_url.to_string(),
                        source,
                    }
                    .into(),
                );
                continue;
            }
        };

        let value = response
            .json::<Value>()
            .map_err(|source| Error::HttpBodyParse {
                url: bootstrap_url.to_string(),
                source,
            })?;

        let bootstrap_response = serde_json::from_value::<AllAnimeBootstrapResponse>(value)
            .map_err(|source| Error::ResponseParse {
                url: bootstrap_url.to_string(),
                source,
            })?;

        if let Some(lane) = bootstrap_response.k.as_deref()
            && lane != content_lane
        {
            last_error = Some(
                Error::StreamResolution {
                    message: format!(
                        "AllAnime bootstrap lane mismatch: expected {content_lane}, got {lane}"
                    ),
                }
                .into(),
            );
            continue;
        }

        return Ok(bootstrap_response);
    }

    Err(last_error.unwrap_or_else(|| {
        Error::StreamResolution {
            message: "epoch bootstrap unavailable".to_string(),
        }
        .into()
    }))
}

fn bootstrap_epoch_candidates(now_ms: u64) -> Vec<u64> {
    let qh = now_ms / ALLANIME_BOOTSTRAP_BUCKET_MS;
    let bs = if now_ms % ALLANIME_BOOTSTRAP_BUCKET_MS < ALLANIME_BOOTSTRAP_GRACE_MS && qh > 0 {
        qh - 1
    } else {
        qh
    };

    if bs == qh { vec![qh] } else { vec![bs, qh] }
}

fn allanime_key_group(host: &str) -> &'static str {
    let host = host.trim().trim_start_matches("www.").to_ascii_lowercase();
    if host == "mkissa.to" || host == "localhost" || host == "127.0.0.1" {
        "mkissa"
    } else if host.contains("youtu-chan.com") {
        "youtu-chan.com"
    } else {
        "mirror"
    }
}

fn build_allanime_boot_token(
    build_key: &[u8; 32],
    build_id: &str,
    epoch: u64,
    key_group: &str,
    referer_host: &str,
    content_lane: &str,
) -> String {
    let first = hmac_sha256(build_key, format!("aa-boot:{build_id}").as_bytes());
    let payload = format!("{build_id}:{key_group}:{referer_host}:{epoch}:{content_lane}");
    bytes_to_hex(&hmac_sha256(&first, payload.as_bytes()))
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    const BLOCK_SIZE: usize = 64;
    let mut block = [0u8; BLOCK_SIZE];
    if key.len() > BLOCK_SIZE {
        let digest = Sha256::digest(key);
        block[..digest.len()].copy_from_slice(&digest);
    } else {
        block[..key.len()].copy_from_slice(key);
    }

    let mut inner_pad = [0x36u8; BLOCK_SIZE];
    let mut outer_pad = [0x5cu8; BLOCK_SIZE];
    for idx in 0..BLOCK_SIZE {
        inner_pad[idx] ^= block[idx];
        outer_pad[idx] ^= block[idx];
    }

    let mut inner = Sha256::new();
    inner.update(inner_pad);
    inner.update(data);
    let inner = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(outer_pad);
    outer.update(inner);
    let output = outer.finalize();

    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&output);
    bytes
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut out, "{byte:02x}").expect("write to string");
    }
    out
}

fn derive_allanime_key(part_b: &str, build_key: &[u8; 32]) -> CoreResult<[u8; 32]> {
    let decoded =
        general_purpose::STANDARD
            .decode(part_b)
            .map_err(|_| Error::StreamResolution {
                message: "failed to base64 decode AllAnime partB".to_string(),
            })?;

    if decoded.len() < build_key.len() {
        return Err(Error::StreamResolution {
            message: format!("invalid AllAnime partB length ({} bytes)", decoded.len()),
        }
        .into());
    }

    let mut key = [0u8; 32];
    for i in 0..32 {
        key[i] = build_key[i] ^ decoded[i];
    }
    Ok(key)
}

#[cfg(test)]
fn hex_to_bytes(hex: &str) -> CoreResult<[u8; 32]> {
    if hex.len() != 64 {
        return Err(Error::StreamResolution {
            message: format!("invalid AllAnime mask length ({})", hex.len()),
        }
        .into());
    }

    let mut out = [0u8; 32];
    for (slot, chunk) in out.iter_mut().zip(hex.as_bytes().chunks(2)) {
        let pair = std::str::from_utf8(chunk).map_err(|_| Error::StreamResolution {
            message: "AllAnime mask is not valid UTF-8".to_string(),
        })?;
        *slot = u8::from_str_radix(pair, 16).map_err(|_| Error::StreamResolution {
            message: format!("invalid AllAnime mask byte {pair}"),
        })?;
    }
    Ok(out)
}

pub(crate) fn maybe_decrypt_response_data(
    mut response: Value,
    key: &[u8; 32],
) -> CoreResult<Value> {
    const MIN_LEN: usize = 1 + 12 + 16;

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

    if decoded.len() < MIN_LEN {
        return Err(Error::StreamResolution {
            message: format!(
                "encrypted AllAnime payload too short ({decoded_len} bytes)",
                decoded_len = decoded.len()
            ),
        }
        .into());
    }

    let version = decoded[0];
    if version != 1 {
        return Err(Error::StreamResolution {
            message: format!("unsupported AllAnime encryption version {version}"),
        }
        .into());
    }

    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| Error::StreamResolution {
        message: "invalid AES key length for AllAnime payload".to_string(),
    })?;
    let nonce = Nonce::from_slice(&decoded[1..13]);
    let plaintext = cipher
        .decrypt(nonce, &decoded[13..])
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn build_allanime_boot_token_matches_live_mkissa_response() {
        let build_key = generate_allanime_build_key("74").expect("build key");

        let token = build_allanime_boot_token(&build_key, "74", 6887, "mkissa", "mkissa.to", "k7");

        assert_eq!(
            token,
            "e2689d1ad932ab40b7b2f1adabc3e3c858d4907528021d2d6ae9dad8089d6533"
        );
    }

    #[test]
    fn build_aa_req_round_trips_the_payload() {
        let key = hex_to_bytes("c9df59c795466fc271f8e48af65e7390860ac465acf6d2cb6a17670c8e5505b0")
            .expect("key hex");
        let aa_req = build_aa_req(
            ALLANIME_EPISODE_EMBED_PERSISTED_HASH,
            "74",
            6887,
            "k7",
            &key,
            1_800_000_000_000,
        )
        .expect("aaReq");

        let decoded = general_purpose::STANDARD.decode(aa_req).expect("base64");
        assert_eq!(decoded[0], 1);
        let expected_iv = Sha256::digest(
            format!(
                "{}:{}:{}",
                6887, ALLANIME_EPISODE_EMBED_PERSISTED_HASH, 1_800_000_000_000u64
            )
            .as_bytes(),
        );
        assert_eq!(&decoded[1..13], &expected_iv[..12]);

        let cipher = Aes256Gcm::new_from_slice(&key).expect("cipher");
        let plaintext = cipher
            .decrypt(Nonce::from_slice(&decoded[1..13]), &decoded[13..])
            .expect("plaintext");
        assert_eq!(
            String::from_utf8(plaintext).expect("utf8"),
            r#"{"v":1,"ts":1800000000000,"epoch":6887,"buildId":"74","qh":"f4662f4b7510b26795dd53ef824a0bf1740fbbc5d1273fab18222ac831bca8d0","k":"k7"}"#
        );
    }

    #[test]
    fn maybe_decrypt_response_data_uses_derived_key() {
        let key = hex_to_bytes("c9df59c795466fc271f8e48af65e7390860ac465acf6d2cb6a17670c8e5505b0")
            .expect("key hex");
        let nonce = *b"0123456789ab";
        let plaintext = json!({
            "episode": {
                "sourceUrls": [
                    {
                        "sourceUrl": "https://stream.example/test.m3u8",
                        "sourceName": "Default"
                    }
                ]
            }
        });

        let cipher = Aes256Gcm::new_from_slice(&key).expect("cipher");
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce), plaintext.to_string().as_bytes())
            .expect("ciphertext");

        let mut payload = Vec::with_capacity(1 + 12 + ciphertext.len());
        payload.push(1);
        payload.extend_from_slice(&nonce);
        payload.extend_from_slice(&ciphertext);

        let response = json!({
            "data": {
                "tobeparsed": general_purpose::STANDARD.encode(payload)
            }
        });

        let decrypted = maybe_decrypt_response_data(response, &key).expect("decrypt");
        assert_eq!(
            decrypted["data"]["episode"]["sourceUrls"][0]["sourceUrl"],
            "https://stream.example/test.m3u8"
        );
    }
}
