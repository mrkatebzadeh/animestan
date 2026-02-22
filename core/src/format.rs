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

use crate::MetadataSource;

#[must_use]
pub fn format_status_score(status: Option<&str>, score: Option<f32>) -> String {
    match (status, score) {
        (Some(status), Some(score)) => format!("{status} / {score:.1}"),
        (Some(status), None) => status.to_string(),
        (None, Some(score)) => format!("Score {score:.1}"),
        _ => "N/A".to_string(),
    }
}

#[must_use]
pub fn format_list(items: &[String]) -> String {
    if items.is_empty() {
        "N/A".to_string()
    } else {
        items.join(", ")
    }
}

#[must_use]
pub fn format_season_year(season: Option<&str>, year: Option<u16>) -> String {
    match (season, year) {
        (Some(season), Some(year)) => format!("{season} {year}"),
        (Some(season), None) => season.to_string(),
        (None, Some(year)) => year.to_string(),
        _ => "N/A".to_string(),
    }
}

#[must_use]
pub fn metadata_source_label(source: MetadataSource) -> &'static str {
    match source {
        MetadataSource::AllManga => "AllManga",
        MetadataSource::AniList => "AniList",
        MetadataSource::Kitsu => "Kitsu",
    }
}
