//! Updates from inside the interface: the check at start (Settings >
//! Updates), the note in the status bar when a release is out, `/update`,
//! and the Updates section itself.

use super::*;

/// Where the update check and install stand, shared with the task doing it.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub(crate) enum UpdateState {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available(String),
    Installing(String),
    Installed(String),
    Failed(String),
}

impl App {
    /// At start: look for a newer release in the background, and install it
    /// when Settings say so. Never waits, and says nothing if it fails.
    pub(crate) fn start_update_check(&mut self) {
        if !self.config.update.check_on_start || enowx_core::update::check_disabled_by_env() {
            return;
        }
        self.run_update(false, self.config.update.auto_install);
    }

    /// `/update`: install the latest release if there is a newer one.
    pub(crate) fn update_now(&mut self) {
        let state = self.update.lock().map(|s| s.clone()).unwrap_or_default();
        match state {
            UpdateState::Checking | UpdateState::Installing(_) => {
                self.status = "an update is already under way".into();
            }
            UpdateState::Installed(tag) => {
                self.status = format!("{tag} is installed; restart enx to use it");
            }
            _ => {
                self.status = "looking for a newer enx…".into();
                self.run_update(true, true);
            }
        }
    }

    /// Check, then install when `install`. `asked` says whether the user
    /// asked (and so wants to hear the outcome, failures included).
    fn run_update(&mut self, asked: bool, install: bool) {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let shared = self.update.clone();
        let set = move |state: UpdateState| {
            if let Ok(mut slot) = shared.lock() {
                *slot = state;
            }
        };
        set(UpdateState::Checking);
        self.update_heard = asked;
        runtime.spawn(async move {
            use enowx_core::update;
            match update::check().await {
                Ok(update::Check::UpToDate) => set(UpdateState::UpToDate),
                Ok(update::Check::Available(tag)) if install => {
                    set(UpdateState::Installing(tag.clone()));
                    match update::install(&tag).await {
                        Ok(_) => set(UpdateState::Installed(tag)),
                        Err(error) => set(UpdateState::Failed(format!("{error:#}"))),
                    }
                }
                Ok(update::Check::Available(tag)) => set(UpdateState::Available(tag)),
                Err(error) => set(UpdateState::Failed(format!("{error:#}"))),
            }
        });
    }

    /// Say what changed since the last frame, once.
    pub(crate) fn tick_update(&mut self) {
        let state = self.update.lock().map(|s| s.clone()).unwrap_or_default();
        if state == self.update_shown {
            return;
        }
        self.update_shown = state.clone();
        match state {
            UpdateState::Available(tag) => {
                self.status = format!("enx {tag} is out · /update installs it");
            }
            UpdateState::Installed(tag) => {
                self.status = format!("enx {tag} installed · restart enx to use it");
            }
            UpdateState::UpToDate if self.update_heard => {
                self.status = format!("enx {} is the latest", enowx_core::update::current());
            }
            UpdateState::Failed(why) if self.update_heard => {
                self.status = format!("update failed: {why}");
            }
            UpdateState::Installing(tag) if self.update_heard => {
                self.status = format!("installing enx {tag}…");
            }
            _ => {}
        }
    }

    /// The note the status bar keeps while there is something to do about
    /// an update.
    pub(crate) fn update_note(&self) -> Option<String> {
        match &self.update_shown {
            UpdateState::Available(tag) => Some(format!("{tag} available · /update")),
            UpdateState::Installing(tag) => Some(format!("installing {tag}…")),
            UpdateState::Installed(tag) => Some(format!("{tag} installed · restart")),
            _ => None,
        }
    }

    /// Settings > Updates, filled from the config.
    pub(crate) fn open_updates(&mut self) {
        let on = |b: bool| if b { "on" } else { "off" }.to_owned();
        self.settings = crate::modal::SettingsDraft {
            provider_id: "updates".into(),
            name: "updates".into(),
            update_check: on(self.config.update.check_on_start),
            update_auto: on(self.config.update.auto_install),
            ..Default::default()
        };
        self.modal_cursor = 0;
        self.field_cursor = 0;
        self.modal_error.clear();
        self.modal = Modal::Updates;
    }

    /// Enter in Settings > Updates: save. The section stays open.
    pub(crate) fn save_updates(&mut self) -> Result<()> {
        let mut config = self.config.clone();
        config.update.check_on_start = self.settings.update_check == "on";
        config.update.auto_install = self.settings.update_auto == "on";
        if let Err(error) = config.save() {
            self.modal_error = format!("{error:#}");
            return Ok(());
        }
        let summary = match (config.update.check_on_start, config.update.auto_install) {
            (false, _) => "update check at start off",
            (true, false) => "checks for updates at start",
            (true, true) => "installs updates at start",
        };
        self.adopt(config);
        self.modal = Modal::Updates;
        self.status = summary.into();
        Ok(())
    }
}
