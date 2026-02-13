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

use crate::{CoreResult, error::Error, models::SourceId};
use serde::Deserialize;
use url::Url;

pub const ALLANIME_API_ENDPOINT: &str = "https://api.allanime.day/api";

#[derive(Debug, Deserialize, Clone)]
pub struct SourceCatalog {
    pub sources: Vec<SourceDefinition>,
}

impl SourceCatalog {
    pub fn load_from_str(json: &str) -> CoreResult<Self> {
        let catalog = serde_json::from_str(json).map_err(Error::CatalogFixture)?;
        Ok(catalog)
    }

    pub fn default_source(&self) -> CoreResult<SourceDefinition> {
        self.sources
            .first()
            .cloned()
            .ok_or_else(|| Error::EmptyCatalog.into())
    }

    pub fn source_by_id(&self, source_id: &str) -> Option<SourceDefinition> {
        self.sources
            .iter()
            .find(|source| source.id == source_id)
            .cloned()
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct SourceDefinition {
    pub id: SourceId,
    pub name: String,
    pub search: EndpointTemplate,
    pub episodes: EndpointTemplate,
    pub stream: EndpointTemplate,
}

impl SourceDefinition {
    pub const ALLANIME_ID: &'static str = "allanime";

    pub fn allanime() -> Self {
        let endpoint = EndpointTemplate {
            url_template: ALLANIME_API_ENDPOINT.to_string(),
        };

        Self {
            id: Self::ALLANIME_ID.to_string(),
            name: "AllAnime".to_string(),
            search: endpoint.clone(),
            episodes: endpoint.clone(),
            stream: endpoint,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct EndpointTemplate {
    pub url_template: String,
}

impl EndpointTemplate {
    pub fn render(&self, params: &[(&str, &str)]) -> CoreResult<Url> {
        let mut rendered = self.url_template.clone();

        for (key, value) in params {
            let placeholder = format!("{{{key}}}");
            let encoded = urlencoding::encode(value);
            rendered = rendered.replace(&placeholder, encoded.as_ref());
        }

        let url = Url::parse(&rendered).map_err(|source| Error::InvalidUrl {
            template: rendered,
            source,
        })?;
        Ok(url)
    }
}
