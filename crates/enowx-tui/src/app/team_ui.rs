//! Settings > Team: agents working together. One switch turns it on; then
//! messages between agents at work, a board each run shares, and
//! cross-review of delegated work (how many correction rounds, and which
//! agent reviews) can each be set. Saved to `config.toml` under
//! `[agent.comms]`.

use super::*;

impl App {
    /// Open the Team section, filled from the config.
    pub(crate) fn open_team(&mut self) {
        let comms = &self.config.agent.comms;
        let on = |b: bool| if b { "on" } else { "off" }.to_owned();
        // Any agent can be the reviewer; the review agents first.
        let mut reviewers: Vec<String> = self
            .discovery
            .agents
            .iter()
            .filter(|a| a.name != "compactor")
            .map(|a| a.name.clone())
            .collect();
        reviewers.sort_by_key(|name| {
            (
                !matches!(name.as_str(), "review" | "test" | "perf"),
                name.clone(),
            )
        });
        self.settings = crate::modal::SettingsDraft {
            provider_id: "team".into(),
            name: "team".into(),
            team_enabled: on(comms.enabled),
            team_messages: on(comms.messages),
            team_board: on(comms.board),
            team_review: on(comms.review),
            review_rounds: comms.review_rounds.clamp(1, 5).to_string(),
            reviewer: comms.reviewer.clone(),
            reviewers,
            ..Default::default()
        };
        self.modal_cursor = 0;
        self.field_cursor = 0;
        self.modal_error.clear();
        self.modal = Modal::Team;
    }

    /// Enter in the Team section: save and use it from the next step. The
    /// section stays open.
    pub(crate) fn save_team(&mut self) -> Result<()> {
        let draft = &self.settings;
        let mut config = self.config.clone();
        let comms = &mut config.agent.comms;
        comms.enabled = draft.team_enabled == "on";
        comms.messages = draft.team_messages == "on";
        comms.board = draft.team_board == "on";
        comms.review = draft.team_review == "on";
        comms.review_rounds = draft.review_rounds.parse().unwrap_or(2).clamp(1, 5);
        if !draft.reviewer.trim().is_empty() {
            comms.reviewer = draft.reviewer.trim().to_owned();
        }
        let summary = if comms.enabled {
            let mut parts = Vec::new();
            if comms.messages {
                parts.push("messages".to_owned());
            }
            if comms.board {
                parts.push("board".to_owned());
            }
            if comms.review {
                parts.push(format!(
                    "review by {} ({} round(s))",
                    comms.reviewer, comms.review_rounds
                ));
            }
            if parts.is_empty() {
                "team on, every part off".to_owned()
            } else {
                format!("team on: {}", parts.join(", "))
            }
        } else {
            "team off".to_owned()
        };
        if let Err(error) = config.save() {
            self.modal_error = format!("{error:#}");
            return Ok(());
        }
        self.adopt(config);
        self.modal = Modal::Team;
        self.status = summary;
        Ok(())
    }
}
