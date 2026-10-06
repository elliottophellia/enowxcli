use super::{
    pages::{Page, Tab, SECTIONS},
    App, SettingEdit, SettingsChoiceState, SettingsReturnContext,
};
use crate::{
    app::settings_catalog::{SettingAction, SettingRow, SettingRowKind},
    modal::Modal,
};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

impl App {
    pub(crate) fn open_settings(&mut self) -> Result<()> {
        if self.modal == Modal::Themes {
            self.theme = crate::theme::Theme::find(&self.config.ui.theme);
        }
        self.active_tab = Tab::Settings;
        self.modal = Modal::None;
        self.settings_nav = false;
        self.settings_query.clear();
        self.setting_edit = None;
        self.settings_choice = None;
        self.settings_return = None;
        self.modal_error.clear();
        if let Some(context) = self.chat_return.take() {
            self.page_index = SECTIONS
                .iter()
                .position(|page| *page == context.category)
                .unwrap_or(0);
            self.settings_row_id = context.row_id;
        }
        self.ensure_settings_selection();
        Ok(())
    }

    pub(crate) fn settings_matches(&self) -> Vec<SettingRow> {
        let mut rows = self.settings_rows();
        if self.settings_query.is_empty() {
            return rows;
        }
        let query = self.settings_query.to_lowercase();
        rows.retain(|row| {
            [row.category.label(), row.label.as_str(), row.id.as_str(), row.description.as_str(), row.value.as_str()]
                .iter().any(|text| text.to_lowercase().contains(&query))
                || matches!(&row.kind, SettingRowKind::Cycle { choices, .. } if choices.iter().any(|(value, label)| value.to_lowercase().contains(&query) || label.to_lowercase().contains(&query)))
        });
        rows.sort_by_key(|row| {
            let label = row.label.to_lowercase();
            let id = row.id.to_lowercase();
            let category = row.category.label().to_lowercase();
            if label.starts_with(&query) || id.starts_with(&query) {
                0
            } else if label.contains(&query) || id.contains(&query) || category.contains(&query) {
                1
            } else {
                2
            }
        });
        rows
    }

    pub(crate) fn ensure_settings_selection(&mut self) {
        let rows = self.settings_matches();
        if !rows.iter().any(|row| row.id == self.settings_row_id) {
            let category = SECTIONS
                .get(self.page_index)
                .copied()
                .unwrap_or(Page::General);
            let actionable = |row: &&SettingRow| !matches!(&row.kind, SettingRowKind::ReadOnly);
            let same_category = rows
                .iter()
                .find(|row| row.category == category && actionable(row));
            let nearest = rows.iter().filter(|row| actionable(row)).min_by_key(|row| {
                let index = SECTIONS
                    .iter()
                    .position(|page| *page == row.category)
                    .unwrap_or(0);
                index.abs_diff(self.page_index)
            });
            self.settings_row_id = same_category
                .or(nearest)
                .or_else(|| rows.first())
                .map(|row| row.id.clone())
                .unwrap_or_default();
        }
        if let Some(row) = self
            .settings_matches()
            .into_iter()
            .find(|row| row.id == self.settings_row_id)
        {
            self.page_index = SECTIONS
                .iter()
                .position(|page| *page == row.category)
                .unwrap_or(self.page_index);
        }
    }

    pub(crate) fn move_settings_row(&mut self, delta: isize) {
        let rows = self.settings_matches();
        if rows.is_empty() {
            self.settings_row_id.clear();
            return;
        }
        let at = rows
            .iter()
            .position(|row| row.id == self.settings_row_id)
            .unwrap_or(0);
        let next = (at as isize + delta).clamp(0, rows.len() as isize - 1) as usize;
        self.settings_row_id = rows[next].id.clone();
        self.page_index = SECTIONS
            .iter()
            .position(|page| *page == rows[next].category)
            .unwrap_or(self.page_index);
    }

    pub(crate) fn move_settings_category(&mut self, delta: isize) {
        let rows = self.settings_matches();
        let current = self.page();
        let matches_category = |category: Page| rows.iter().any(|row| row.category == category);
        let visible: Vec<Page> = SECTIONS
            .iter()
            .copied()
            .filter(|page| matches_category(*page))
            .collect();
        if visible.is_empty() {
            return;
        }
        let at = visible
            .iter()
            .position(|page| *page == current)
            .unwrap_or(0);
        let next = (at as isize + delta).clamp(0, visible.len() as isize - 1) as usize;
        let category = visible[next];
        self.page_index = SECTIONS
            .iter()
            .position(|page| *page == category)
            .unwrap_or(0);
        if let Some(row) = rows.iter().find(|row| {
            row.category == category
                && matches!(
                    row.kind,
                    SettingRowKind::Action { .. }
                        | SettingRowKind::Toggle { .. }
                        | SettingRowKind::Cycle { .. }
                        | SettingRowKind::Text { .. }
                )
        }) {
            self.settings_row_id = row.id.clone();
        }
    }
    pub(crate) fn select_settings_category(&mut self, category: Page) {
        if let Some(index) = SECTIONS.iter().position(|page| *page == category) {
            self.page_index = index;
        }
        if let Some(row) = self
            .settings_matches()
            .into_iter()
            .find(|row| row.category == category)
        {
            self.settings_row_id = row.id;
        }
        self.settings_nav = false;
    }

    pub(crate) fn activate_settings_row(&mut self, id: &str) -> Result<()> {
        let Some(row) = self.settings_rows().into_iter().find(|row| row.id == id) else {
            return Ok(());
        };
        self.settings_row_id = row.id.clone();
        self.page_index = SECTIONS
            .iter()
            .position(|page| *page == row.category)
            .unwrap_or(self.page_index);
        match row.kind.clone() {
            SettingRowKind::Toggle { config_key } => self.save_settings_value(
                &row,
                &config_key,
                if row.value.eq_ignore_ascii_case("true")
                    || row.value.eq_ignore_ascii_case("on")
                    || row.value.eq_ignore_ascii_case("enabled")
                {
                    "false"
                } else {
                    "true"
                },
            ),
            SettingRowKind::Cycle {
                config_key,
                choices,
            } => {
                let index = choices
                    .iter()
                    .position(|(value, label)| value == &row.value || label == &row.value)
                    .unwrap_or(0);
                let next = choices
                    .get((index + 1) % choices.len().max(1))
                    .map(|(value, _)| value.clone())
                    .unwrap_or_default();
                self.save_settings_value(&row, &config_key, &next)
            }
            SettingRowKind::Text {
                config_key,
                placeholder,
            } => {
                let value = self.config.get(&config_key).unwrap_or_default();
                self.setting_edit = Some(SettingEdit {
                    row_id: row.id,
                    category: row.category,
                    config_key,
                    label: row.label,
                    description: row.description,
                    placeholder,
                    cursor: value.len(),
                    draft: value,
                    error: String::new(),
                });
                self.modal = Modal::SettingValue;
                Ok(())
            }
            SettingRowKind::Action { enter, .. } => self.run_settings_action(&row, enter),
            SettingRowKind::ReadOnly => Ok(()),
        }
    }

    pub(crate) fn settings_key(&mut self, key: &KeyEvent) -> Result<bool> {
        if self.active_tab != Tab::Settings {
            return Ok(false);
        }
        if self.modal == Modal::SettingValue {
            self.setting_value_key(key)?;
            return Ok(true);
        }
        if self.modal == Modal::SettingsChoice {
            self.settings_choice_key(key)?;
            return Ok(true);
        }
        if self.modal != Modal::None {
            return Ok(false);
        }
        if self.modal == Modal::Models && key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('f') => {
                    self.toggle_favorite_model()?;
                    return Ok(true);
                }
                KeyCode::Char('r') => {
                    self.refresh_models();
                    return Ok(true);
                }
                KeyCode::Char('n') => {
                    self.open_manual_model();
                    return Ok(true);
                }
                KeyCode::Char('e') => {
                    self.open_edit_model();
                    return Ok(true);
                }
                _ => {}
            }
        }
        match key.code {
            KeyCode::Up if self.settings_nav => self.move_settings_category(-1),
            KeyCode::Down if self.settings_nav => self.move_settings_category(1),
            KeyCode::Up => self.move_settings_row(-1),
            KeyCode::Down => self.move_settings_row(1),
            KeyCode::PageUp => self.move_settings_category(-1),
            KeyCode::PageDown => self.move_settings_category(1),
            KeyCode::Tab | KeyCode::BackTab => {
                let categories = self
                    .settings_matches()
                    .iter()
                    .map(|row| row.category)
                    .collect::<std::collections::HashSet<_>>();
                self.settings_nav = categories.len() >= 2 && !self.settings_nav;
            }
            KeyCode::Right | KeyCode::Enter if self.settings_nav => {
                self.settings_nav = false;
                self.ensure_settings_selection();
            }
            KeyCode::Right if !self.settings_nav => self.cycle_settings_choice(true)?,
            KeyCode::Left if !self.settings_nav => self.cycle_settings_choice(false)?,
            KeyCode::Char(' ')
                if !self.settings_nav
                    && (self.settings_query.is_empty()
                        || self.settings_row_id == self.settings_query) =>
            {
                self.activate_settings_row_with_space(&self.settings_row_id.clone())?
            }
            KeyCode::Esc if !self.settings_query.is_empty() => {
                self.settings_query.clear();
                self.ensure_settings_selection();
            }
            KeyCode::Esc if self.settings_nav => self.settings_nav = false,
            KeyCode::Esc => self.open_page(Page::Chat)?,
            KeyCode::Backspace if !self.settings_nav => {
                self.settings_query.pop();
                self.ensure_settings_selection();
            }
            KeyCode::Char(c) if !self.settings_nav && !c.is_control() => {
                if self.settings_query.is_empty() {
                    let selected = self
                        .settings_matches()
                        .into_iter()
                        .find(|row| row.id == self.settings_row_id);
                    if let Some(row) = selected {
                        if let SettingRowKind::Action { shortcuts, .. } = row.kind.clone() {
                            if let Some((_, action)) = shortcuts
                                .into_iter()
                                .find(|(code, _)| *code == KeyCode::Char(c))
                            {
                                self.run_settings_action(&row, action)?;
                                return Ok(true);
                            }
                        }
                    }
                }
                self.settings_query.push(c);
                self.ensure_settings_selection();
            }
            KeyCode::Enter if !self.settings_nav => {
                self.activate_settings_row(&self.settings_row_id.clone())?
            }
            _ => {}
        }
        Ok(true)
    }

    fn cycle_settings_choice(&mut self, forward: bool) -> Result<()> {
        let Some(row) = self
            .settings_matches()
            .into_iter()
            .find(|row| row.id == self.settings_row_id)
        else {
            return Ok(());
        };
        if let SettingRowKind::Cycle {
            config_key,
            choices,
        } = row.kind.clone()
        {
            if choices.is_empty() {
                return Ok(());
            }
            let index = choices
                .iter()
                .position(|(value, _)| value == &row.value)
                .unwrap_or(0);
            let next = if forward {
                (index + 1) % choices.len()
            } else {
                (index + choices.len() - 1) % choices.len()
            };
            self.save_settings_value(&row, &config_key, &choices[next].0)?;
        }
        Ok(())
    }

    fn save_settings_value(&mut self, row: &SettingRow, key: &str, value: &str) -> Result<()> {
        let mut next = self.config.clone();
        if let Err(error) = next.set(key, value).and_then(|()| next.save().map(|_| ())) {
            self.modal_error = format!("{}: {error:#}", row.label);
            self.status = self.modal_error.clone();
            return Ok(());
        }
        self.adopt(next);
        self.modal_error.clear();
        self.status = format!("{} changed; used on the next request", row.label);
        Ok(())
    }
    pub(crate) fn activate_settings_row_with_space(&mut self, id: &str) -> Result<()> {
        let Some(row) = self.settings_rows().into_iter().find(|row| row.id == id) else {
            return Ok(());
        };
        match &row.kind {
            SettingRowKind::Toggle { .. } | SettingRowKind::Cycle { .. } => {
                self.activate_settings_row(id)
            }
            SettingRowKind::Action {
                space: Some(action),
                ..
            } => self.run_settings_action(&row, action.clone()),
            _ => Ok(()),
        }
    }

    fn begin_settings_return(&mut self, row: &SettingRow) {
        self.settings_return = Some(SettingsReturnContext {
            category: row.category,
            row_id: row.id.clone(),
            query: self.settings_query.clone(),
        });
    }

    fn run_settings_action(&mut self, row: &SettingRow, action: SettingAction) -> Result<()> {
        use crate::app::settings_catalog::SettingAction as A;
        self.begin_settings_return(row);
        match action {
            A::ChooseModel => self.open_model_picker(None),
            A::SetEffort => self.open_effort()?,
            A::RefreshModels => {
                self.open_model_picker(None);
                self.refresh_models();
            }
            A::AddModel => {
                self.open_model_picker(None);
                self.open_manual_model();
            }
            A::OpenProvider(id) => {
                self.open_providers();
                self.modal_cursor = self
                    .provider_ids
                    .iter()
                    .position(|candidate| candidate == &id)
                    .unwrap_or(0);
                self.select_provider();
            }
            A::AddProvider => {
                self.open_providers();
                self.modal_cursor = self.provider_ids.len();
                self.select_provider();
            }
            A::DisconnectProvider(id) => {
                self.open_providers();
                self.modal_cursor = self
                    .provider_ids
                    .iter()
                    .position(|candidate| candidate == &id)
                    .unwrap_or(0);
                self.disconnect_provider()?;
                self.restore_settings_return();
            }
            A::SwitchAgent(name) => {
                self.force_agent(&name)?;
                self.restore_settings_return();
            }
            A::SetAgentModel(name) => {
                self.picking_for_agent = Some(name);
                self.open_model_picker(None);
            }
            A::ClearAgentModel(name) => {
                self.set_agent_model(&name, None)?;
                self.restore_settings_return();
            }
            A::ToggleMcp(name) => {
                self.open_mcp();
                self.modal_cursor = self.mcp_rows().iter().position(|entry| matches!(entry, super::mcp_ui::McpRow::Server { name: found, .. } if found == &name)).unwrap_or(0);
                self.toggle_selected_mcp()?;
                self.restore_settings_return();
            }
            A::ShowMcpTools(name) => {
                self.open_mcp();
                self.modal_cursor = self.mcp_rows().iter().position(|entry| matches!(entry, super::mcp_ui::McpRow::Server { name: found, .. } if found == &name)).unwrap_or(0);
                self.show_mcp_tools();
                self.restore_settings_return();
            }
            A::ConfigureMcp(name) => {
                if name == "rag" {
                    self.open_rag();
                } else {
                    self.open_mcp();
                    self.modal_cursor = self.mcp_rows().iter().position(|entry| matches!(entry, super::mcp_ui::McpRow::Server { name: found, .. } if found == &name)).unwrap_or(0);
                    self.config_selected_mcp();
                    if self.modal == Modal::Mcp {
                        self.restore_settings_return();
                    }
                }
            }
            A::AddMcp => self.open_mcp_form(),
            A::ConfigureRag => self.open_rag(),
            A::ToggleSkill(name) => {
                self.open_skills();
                self.modal_cursor = self
                    .skill_rows()
                    .iter()
                    .position(|row| row.name == name)
                    .unwrap_or(0);
                self.toggle_selected_skill()?;
                self.restore_settings_return();
            }
            A::ReadSkill(name) => {
                self.open_skills();
                self.modal_cursor = self
                    .skill_rows()
                    .iter()
                    .position(|row| row.name == name)
                    .unwrap_or(0);
                self.read_selected_skill()?;
                self.restore_settings_return();
            }
            A::SetTypeSafeKey => {
                self.settings.api_key.clear();
                self.open_form(Modal::TypeSafeKey);
            }
            A::ChooseTeamReviewer => self.open_settings_choice(
                row,
                "agent.comms.reviewer",
                self.discovery
                    .agents
                    .iter()
                    .filter(|agent| agent.is_routable())
                    .map(|agent| (agent.name.clone(), agent.name.clone()))
                    .collect(),
            ),
            A::ResumeSession(id) => {
                self.resume(&id)?;
                self.active_tab = Tab::Chat;
                self.modal = Modal::None;
                self.settings_return = None;
            }
            A::ChooseTheme => self.open_themes(),
            A::UpdateNow => {
                self.update_now();
                self.restore_settings_return();
            }
        }
        Ok(())
    }

    fn open_settings_choice(
        &mut self,
        row: &SettingRow,
        key: &str,
        choices: Vec<(String, String)>,
    ) {
        if choices.is_empty() {
            self.status = "no choices available".into();
            return;
        }
        let current = self.config.get(key).unwrap_or_default();
        let selected_index = choices
            .iter()
            .position(|(value, _)| value == &current)
            .unwrap_or(0);
        self.settings_choice = Some(SettingsChoiceState {
            row_id: row.id.clone(),
            category: row.category,
            config_key: key.to_owned(),
            original_value: current.clone(),
            preview_value: choices[selected_index].0.clone(),
            selected_index,
            choices,
        });
        self.modal = Modal::SettingsChoice;
    }

    fn settings_choice_key(&mut self, key: &KeyEvent) -> Result<()> {
        let Some(state) = self.settings_choice.as_mut() else {
            self.modal = Modal::None;
            return Ok(());
        };
        match key.code {
            KeyCode::Up => state.selected_index = state.selected_index.saturating_sub(1),
            KeyCode::Down => {
                state.selected_index =
                    (state.selected_index + 1).min(state.choices.len().saturating_sub(1))
            }
            KeyCode::Esc => {
                self.settings_choice = None;
                self.modal = Modal::None;
                self.restore_settings_return();
            }
            KeyCode::Enter => {
                let state = self.settings_choice.as_ref().unwrap().clone();
                let row = self
                    .settings_rows()
                    .into_iter()
                    .find(|row| row.id == state.row_id)
                    .unwrap();
                self.save_settings_value(&row, &state.config_key, &state.preview_value)?;
                if self.modal_error.is_empty() {
                    self.settings_choice = None;
                    self.modal = Modal::None;
                    self.restore_settings_return();
                }
            }
            _ => {}
        }
        if let Some(state) = self.settings_choice.as_mut() {
            if let Some((value, _)) = state.choices.get(state.selected_index) {
                state.preview_value = value.clone();
            }
        }
        Ok(())
    }
    fn setting_value_key(&mut self, key: &KeyEvent) -> Result<()> {
        let Some(edit) = self.setting_edit.as_mut() else {
            self.modal = Modal::None;
            return Ok(());
        };
        let cursor = edit
            .draft
            .char_indices()
            .map(|(index, _)| index)
            .chain(std::iter::once(edit.draft.len()))
            .collect::<Vec<_>>();
        let pos = cursor
            .iter()
            .position(|index| *index == edit.cursor)
            .unwrap_or(cursor.len().saturating_sub(1));
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            if key.code == KeyCode::Char('u') {
                edit.draft.clear();
                edit.cursor = 0;
                edit.error.clear();
            }
            return Ok(());
        }
        match key.code {
            KeyCode::Esc => {
                self.setting_edit = None;
                self.modal = Modal::None;
                self.restore_settings_return();
            }
            KeyCode::Enter => {
                let edit = self.setting_edit.as_ref().unwrap().clone();
                let row = self
                    .settings_rows()
                    .into_iter()
                    .find(|row| row.id == edit.row_id)
                    .unwrap();
                self.save_settings_value(&row, &edit.config_key, &edit.draft)?;
                if self.modal_error.is_empty() {
                    self.setting_edit = None;
                    self.modal = Modal::None;
                    self.restore_settings_return();
                } else if let Some(edit) = self.setting_edit.as_mut() {
                    edit.error = self.modal_error.clone();
                }
            }
            KeyCode::Left => {
                self.setting_edit.as_mut().unwrap().cursor = cursor[pos.saturating_sub(1)]
            }
            KeyCode::Right => {
                self.setting_edit.as_mut().unwrap().cursor = cursor[(pos + 1).min(cursor.len() - 1)]
            }
            KeyCode::Home => self.setting_edit.as_mut().unwrap().cursor = 0,
            KeyCode::End => {
                self.setting_edit.as_mut().unwrap().cursor =
                    self.setting_edit.as_ref().unwrap().draft.len()
            }
            KeyCode::Backspace if pos > 0 => {
                let start = cursor[pos - 1];
                let end = cursor[pos];
                let edit = self.setting_edit.as_mut().unwrap();
                edit.draft.replace_range(start..end, "");
                edit.cursor = start;
                edit.error.clear();
            }
            KeyCode::Char(ch) if !ch.is_control() => {
                let edit = self.setting_edit.as_mut().unwrap();
                edit.draft.insert(edit.cursor, ch);
                edit.cursor += ch.len_utf8();
                edit.error.clear();
            }
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn settings_paste(&mut self, text: &str) -> bool {
        if self.modal != Modal::SettingValue {
            return false;
        }
        if let Some(edit) = self.setting_edit.as_mut() {
            let clean = text
                .chars()
                .filter(|ch| !ch.is_control())
                .collect::<String>();
            edit.draft.insert_str(edit.cursor, &clean);
            edit.cursor += clean.len();
            edit.error.clear();
        }
        true
    }

    pub(crate) fn restore_settings_return(&mut self) {
        if let Some(ret) = self.settings_return.take() {
            self.active_tab = Tab::Settings;
            self.page_index = SECTIONS
                .iter()
                .position(|page| *page == ret.category)
                .unwrap_or(self.page_index);
            self.settings_query = ret.query;
            self.settings_row_id = ret.row_id;
            self.modal = Modal::None;
            self.ensure_settings_selection();
        }
    }
}
