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

use super::{
    App, FilterCandidate, Focus, QUICK_LAUNCH_HISTORY_SIZE, QUICK_LAUNCH_RECENT_PLAY_SIZE,
};

use nucleo::pattern::{CaseMatching, Normalization, Pattern};

#[derive(Clone)]
pub enum QuickLaunchAction {
    GoToSearch,
    OpenAnimePanel,
    OpenEpisodePanel,
    DownloadCurrentEpisode,
    OpenInfo,
    PlayLastEpisode { episode_id: String },
}

#[derive(Clone)]
pub struct QuickLaunchCandidate {
    pub label: String,
    pub action: QuickLaunchAction,
    pub score: i32,
}

#[derive(Clone)]
pub(super) struct LastPlayedEpisode {
    episode_id: String,
    title: Option<String>,
    anime_id: Option<String>,
}

#[derive(Clone)]
pub(super) struct PendingPlayback {
    episode_id: String,
    title: Option<String>,
    anime_id: Option<String>,
}

impl App {
    pub fn quick_launch_active(&self) -> bool {
        self.quick_launch_active
    }

    pub fn quick_launch_query(&self) -> &str {
        &self.quick_launch_query
    }

    pub fn quick_launch_selection(&self) -> usize {
        self.quick_launch_selection
    }

    pub fn quick_launch_items(&self) -> &[QuickLaunchCandidate] {
        &self.quick_launch_items
    }

    pub fn open_quick_launch(&mut self) {
        self.quick_launch_active = true;
        self.quick_launch_selection = 0;
        self.quick_launch_query.clear();
        self.set_details("Quick Launch: type to filter, Enter to run, Esc to close.");
        self.refresh_quick_launch_items();
    }

    pub fn close_quick_launch(&mut self) {
        self.quick_launch_active = false;
    }

    pub fn append_quick_launch_char(&mut self, ch: char) {
        self.quick_launch_query.push(ch);
        self.refresh_quick_launch_items();
    }

    pub fn pop_quick_launch_char(&mut self) {
        self.quick_launch_query.pop();
        self.refresh_quick_launch_items();
    }

    pub fn move_quick_launch_selection_up(&mut self) {
        if self.quick_launch_selection > 0 {
            self.quick_launch_selection -= 1;
        }
    }

    pub fn move_quick_launch_selection_down(&mut self, len: usize) {
        if len == 0 {
            self.quick_launch_selection = 0;
            return;
        }
        if self.quick_launch_selection + 1 < len {
            self.quick_launch_selection += 1;
        }
    }

    pub fn run_quick_launch_selection(&mut self) {
        if let Some(candidate) = self.quick_launch_items.get(self.quick_launch_selection) {
            let action = candidate.action.clone();
            match action {
                QuickLaunchAction::GoToSearch => {
                    self.enter_search_mode();
                    self.focus = Focus::Left;
                    self.set_details("Focus: search input");
                }
                QuickLaunchAction::OpenAnimePanel => {
                    self.focus = Focus::Left;
                    self.set_details("Open: anime panel");
                }
                QuickLaunchAction::OpenEpisodePanel => {
                    self.focus = Focus::Right;
                    self.set_details("Open: episode panel");
                }
                QuickLaunchAction::DownloadCurrentEpisode => {
                    self.request_download();
                }
                QuickLaunchAction::OpenInfo => {
                    self.open_info_modal();
                    self.set_details("Press Esc to close info modal.");
                }
                QuickLaunchAction::PlayLastEpisode { episode_id } => {
                    let title = self
                        .last_played_episode
                        .as_ref()
                        .and_then(|entry| entry.title.clone());
                    let anime_id = self
                        .last_played_episode
                        .as_ref()
                        .and_then(|entry| entry.anime_id.clone());
                    self.set_pending_playback_override(episode_id.clone(), title.clone(), anime_id);
                    self.request_play_async();
                    if let Some(title) = title {
                        self.set_details(format!("Quick Launch: playing {title}"));
                    } else {
                        self.set_details("Quick Launch: replaying last episode");
                    }
                }
            }
            self.refresh_quick_launch_items();
        }
        self.close_quick_launch();
    }

    pub(super) fn refresh_quick_launch_items(&mut self) {
        let candidates = self.build_quick_launch_candidates();
        self.quick_launch_items = self.rank_quick_launch_candidates(candidates);
        if self.quick_launch_selection >= self.quick_launch_items.len() {
            self.quick_launch_selection = self.quick_launch_items.len().saturating_sub(1);
        }
    }

    fn build_quick_launch_candidates(&self) -> Vec<QuickLaunchCandidate> {
        let mut candidates = Vec::new();
        candidates.push(QuickLaunchCandidate {
            label: "Go to search".to_string(),
            score: 40,
            action: QuickLaunchAction::GoToSearch,
        });

        if !matches!(self.focus, Focus::Left) {
            candidates.push(QuickLaunchCandidate {
                label: "Open anime".to_string(),
                score: 30,
                action: QuickLaunchAction::OpenAnimePanel,
            });
        }

        if !matches!(self.focus, Focus::Right) {
            candidates.push(QuickLaunchCandidate {
                label: "Open episodes".to_string(),
                score: 30,
                action: QuickLaunchAction::OpenEpisodePanel,
            });
        }

        if matches!(self.focus, Focus::Right) {
            candidates.push(QuickLaunchCandidate {
                label: "Download current episode".to_string(),
                score: 25,
                action: QuickLaunchAction::DownloadCurrentEpisode,
            });
        }

        if let Some(entry) = &self.last_played_episode {
            let label = if let Some(title) = &entry.title {
                format!("Play last episode: {title}")
            } else {
                "Play last episode".to_string()
            };
            candidates.push(QuickLaunchCandidate {
                label,
                score: 50,
                action: QuickLaunchAction::PlayLastEpisode {
                    episode_id: entry.episode_id.clone(),
                },
            });
        }

        candidates.push(QuickLaunchCandidate {
            label: "Open anime info".to_string(),
            score: 10,
            action: QuickLaunchAction::OpenInfo,
        });

        candidates
    }

    fn rank_quick_launch_candidates(
        &mut self,
        mut candidates: Vec<QuickLaunchCandidate>,
    ) -> Vec<QuickLaunchCandidate> {
        let query = self.quick_launch_query().trim();
        if query.is_empty() {
            candidates.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.label.cmp(&b.label)));
            return candidates;
        }

        let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
        let matcher_candidates: Vec<_> = candidates
            .iter()
            .enumerate()
            .map(|(index, candidate)| FilterCandidate {
                index,
                title: candidate.label.as_str(),
            })
            .collect();
        let mut ranked = pattern.match_list(matcher_candidates, &mut self.matcher);
        ranked.sort_by(|(left, left_score), (right, right_score)| {
            right_score
                .cmp(left_score)
                .then_with(|| {
                    candidates[right.index]
                        .score
                        .cmp(&candidates[left.index].score)
                })
                .then_with(|| left.index.cmp(&right.index))
        });

        ranked
            .into_iter()
            .map(|(candidate, _)| candidates[candidate.index].clone())
            .collect()
    }

    pub fn record_anime_history(&mut self, anime_id: &str) {
        if let Some(pos) = self
            .quick_launch_history
            .iter()
            .position(|id| id == anime_id)
        {
            self.quick_launch_history.remove(pos);
        }
        self.quick_launch_history.push_front(anime_id.to_string());
        if self.quick_launch_history.len() > QUICK_LAUNCH_HISTORY_SIZE {
            self.quick_launch_history.pop_back();
        }
    }

    pub fn record_played_episode(
        &mut self,
        episode_id: String,
        anime_id: Option<String>,
        title: Option<String>,
    ) {
        if let Some(anime_id) = anime_id.clone() {
            if let Some(pos) = self
                .quick_launch_recently_played
                .iter()
                .position(|id| id == &anime_id)
            {
                self.quick_launch_recently_played.remove(pos);
            }
            self.quick_launch_recently_played.push_front(anime_id);
            if self.quick_launch_recently_played.len() > QUICK_LAUNCH_RECENT_PLAY_SIZE {
                self.quick_launch_recently_played.pop_back();
            }
        }
        self.last_played_episode = Some(LastPlayedEpisode {
            episode_id,
            title,
            anime_id,
        });
        self.refresh_quick_launch_items();
    }

    pub fn set_pending_playback_override(
        &mut self,
        episode_id: String,
        title: Option<String>,
        anime_id: Option<String>,
    ) {
        self.pending_playback_override = Some(PendingPlayback {
            episode_id,
            title,
            anime_id,
        });
    }

    pub fn take_pending_playback_override(
        &mut self,
    ) -> Option<(String, Option<String>, Option<String>)> {
        self.pending_playback_override
            .take()
            .map(|pending| (pending.episode_id, pending.title, pending.anime_id))
    }
}
