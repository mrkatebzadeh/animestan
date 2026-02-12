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

#[cfg(test)]
mod tests {
    use assert_cmd::assert::OutputAssertExt;
    use predicates::prelude::PredicateBooleanExt;
    use predicates::str::contains;
    use std::process::Command;

    fn cli_command() -> Command {
        let mut command = Command::new("cargo");
        command.args(["run", "-q", "-p", "animestan-cli", "--"]);
        command.env("ANIMESTAN_USE_FIXTURES", "1");
        command
    }

    #[test]
    fn search_command_outputs_naruto_entries() {
        cli_command()
            .args(["search", "naruto"])
            .assert()
            .success()
            .stdout(contains("naruto").and(contains("Naruto")));
    }

    #[test]
    fn episodes_command_outputs_episode_listing() {
        cli_command()
            .args(["episodes", "naruto"])
            .assert()
            .success()
            .stdout(contains("naruto-1").and(contains("Enter: Naruto")));
    }

    #[test]
    fn url_command_outputs_stream_url() {
        cli_command()
            .args(["url", "naruto-1"])
            .assert()
            .success()
            .stdout(contains("https://stream.example/naruto-1.m3u8"));
    }
}
