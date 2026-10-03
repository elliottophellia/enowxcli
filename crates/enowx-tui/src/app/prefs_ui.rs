//! Settings > General and Settings > Display: config keys edited in place,
//! each field one dotted key that `Config::set` checks and writes.

use super::*;
use crate::modal::{CONF_FIELDS, DISPLAY_FIELDS, GENERAL_FIELDS};

impl App {
    /// Open General or Display, filled from the config.
    pub(crate) fn open_prefs(&mut self, modal: Modal) {
        let conf: Vec<String> = CONF_FIELDS
            .iter()
            .map(|(key, ..)| self.config.get(key).unwrap_or_default())
            .collect();
        self.settings = crate::modal::SettingsDraft {
            provider_id: "prefs".into(),
            name: "prefs".into(),
            conf,
            ..Default::default()
        };
        self.modal_cursor = 0;
        self.field_cursor = 0;
        self.modal_error.clear();
        self.modal = modal;
    }

    /// The fields of the open General or Display section.
    pub(crate) fn prefs_fields(&self) -> &'static [crate::modal::SettingsField] {
        if self.modal == Modal::Display {
            &DISPLAY_FIELDS
        } else {
            &GENERAL_FIELDS
        }
    }

    /// Enter: write each field of the section through `Config::set`, which
    /// parses and checks it; a bad value is said under the form and nothing
    /// is saved. The section stays open.
    pub(crate) fn save_prefs(&mut self) -> Result<()> {
        let mut config = self.config.clone();
        for field in self.prefs_fields() {
            let crate::modal::SettingsField::Conf(i) = *field else {
                continue;
            };
            let (key, label, ..) = CONF_FIELDS[i];
            let value = self.settings.value(*field).trim().to_owned();
            if let Err(error) = config.set(key, &value) {
                self.modal_error = format!("{label}: {error:#}");
                return Ok(());
            }
        }
        if let Err(error) = config.save() {
            self.modal_error = format!("{error:#}");
            return Ok(());
        }
        // What the sidebar setting changes shows at once.
        self.show_sidebar = config.ui.show_sidebar;
        let modal = self.modal;
        self.adopt(config);
        self.modal = modal;
        self.status = "settings saved".into();
        Ok(())
    }
}
