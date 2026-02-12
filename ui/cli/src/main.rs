// Copyright (C) 2026 M.R. Siavash Katebzadeg <mr@katebzadeh.xyz>
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

use animestan_core::AnimeClient;
use clap::{Parser, Subcommand};

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), animestan_core::Error> {
    let cli = Cli::parse();
    let client = AnimeClient::with_fixtures()?;

    match cli.command {
        Commands::Search { query } => {
            let results = client.search(&query)?;
            for entry in results {
                println!("{}\t{}", entry.id, entry.title);
            }
        }
        Commands::Episodes { anime_id } => {
            let episodes = client.list_episodes(&anime_id)?;
            for episode in episodes {
                println!("{}\t{}\t{}", episode.id, episode.number, episode.title);
            }
        }
        Commands::Url { episode_id } => {
            let link = client.resolve_stream_url(&episode_id)?;
            println!("{}", link.url);
        }
    }

    Ok(())
}

#[derive(Parser)]
#[command(name = "animestan-cli", version, about = "Search demo fixtures", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Search catalog by query string
    Search { query: String },
    /// List episodes for a given anime id
    Episodes { anime_id: String },
    /// Resolve a stream URL for an episode
    Url { episode_id: String },
}
