//! Settings > Decision model: a small model for typed judgements, off by
//! default. One switch, the provider and what it needs (model, account,
//! endpoint, key), how long a judgement may take, shadow mode, a connection
//! test, and each use with its threshold. Saved to `config.toml` under
//! `[decision]`; the key goes to `auth.json` and is never shown.

use super::*;
use crate::modal::{DecField, SettingsField};
use enowx_core::decision::{self, Use};

const TEST_HINT: &str = "Enter: save, then ask a test question";

impl App {
    /// Open the section, filled from the config. A stored key is never
    /// shown: the field is blank, and blank keeps it.
    pub(crate) fn open_decision(&mut self) {
        let d = &self.config.decision;
        let on = |b: bool| if b { "on" } else { "off" }.to_owned();
        let mut dec = std::collections::BTreeMap::new();
        dec.insert(DecField::Enabled.key(), on(d.enabled));
        dec.insert(DecField::Provider.key(), d.provider.clone());
        dec.insert(DecField::ModelPick.key(), d.model.clone());
        dec.insert(DecField::Account.key(), d.account_id.clone());
        dec.insert(DecField::BaseUrl.key(), d.base_url.clone());
        dec.insert(DecField::Key.key(), String::new());
        dec.insert(DecField::Timeout.key(), d.timeout_ms.to_string());
        dec.insert(DecField::Shadow.key(), on(d.shadow));
        dec.insert(DecField::Test.key(), TEST_HINT.to_owned());
        for u in Use::ALL {
            let use_config = d.uses.get(u);
            dec.insert(DecField::UseOn(u).key(), on(use_config.enabled));
            dec.insert(
                DecField::Threshold(u).key(),
                format!("{}", use_config.threshold),
            );
        }
        self.settings = crate::modal::SettingsDraft {
            provider_id: "decision".into(),
            name: "decision".into(),
            dec,
            ..Default::default()
        };
        self.modal_cursor = 0;
        self.field_cursor = 0;
        self.modal_error.clear();
        self.modal = Modal::Decision;
    }

    /// The fields for the provider chosen in the draft.
    pub(crate) fn decision_fields(&self) -> &'static [SettingsField] {
        crate::modal::decision_fields(self.settings.value(SettingsField::Dec(DecField::Provider)))
    }

    /// The config the draft describes, with a key typed here stored first.
    /// `Err` is what to say under the form.
    fn decision_draft(&self) -> std::result::Result<Config, String> {
        let draft = &self.settings;
        let get = |f: DecField| draft.value(SettingsField::Dec(f)).trim().to_owned();
        let mut config = self.config.clone();
        let d = &mut config.decision;
        d.enabled = get(DecField::Enabled) == "on";
        d.provider = get(DecField::Provider);
        d.model = get(DecField::ModelPick);
        d.account_id = get(DecField::Account);
        d.base_url = get(DecField::BaseUrl);
        d.timeout_ms = get(DecField::Timeout)
            .parse()
            .map_err(|_| "the timeout must be a number of milliseconds".to_owned())?;
        d.shadow = get(DecField::Shadow) == "on";
        for u in Use::ALL {
            let use_config = d.uses.get_mut(u);
            use_config.enabled = get(DecField::UseOn(u)) == "on";
            use_config.threshold = get(DecField::Threshold(u))
                .parse::<f64>()
                .map_err(|_| format!("{}: the threshold must be a number", u.label()))?
                .clamp(0.5, 0.999);
        }
        let key = get(DecField::Key);
        if !key.is_empty() {
            config
                .auth
                .store(&decision::secret_id(&config.decision.provider), &key)
                .map_err(|e| format!("{e:#}"))?;
        }
        // On means usable: a provider missing its key or account is said
        // here, not discovered as silence later.
        if config.decision.enabled {
            decision::build_provider(&config)?;
        }
        Ok(config)
    }

    /// Enter in the section: save, and use it from the next message. On the
    /// test row, save and then ask the provider a test question.
    pub(crate) fn save_decision(&mut self) -> Result<()> {
        let testing = self
            .decision_fields()
            .get(self.modal_cursor)
            .is_some_and(|f| *f == SettingsField::Dec(DecField::Test));
        let config = match self.decision_draft() {
            Ok(config) => config,
            Err(why) => {
                self.modal_error = why;
                return Ok(());
            }
        };
        if let Err(error) = config.save() {
            self.modal_error = format!("{error:#}");
            return Ok(());
        }
        let d = &config.decision;
        self.status = if !d.enabled {
            "decision model off".into()
        } else if d.shadow {
            format!("decision model on in shadow mode: {}", d.model())
        } else {
            format!("decision model on: {}", d.model())
        };
        let cursor = self.modal_cursor;
        self.adopt(config.clone());
        self.settings.dec.insert(DecField::Key.key(), String::new());
        self.field_cursor = 0;
        self.modal = Modal::Decision;
        self.modal_cursor = cursor;
        if testing {
            self.start_decision_test(config);
        }
        Ok(())
    }

    /// Ask the provider a test question off the UI thread.
    fn start_decision_test(&mut self, config: Config) {
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            self.settings
                .dec
                .insert(DecField::Test.key(), "cannot test here".into());
            return;
        };
        let (tx, rx) = mpsc::channel(1);
        self.decision_test = Some(rx);
        self.settings
            .dec
            .insert(DecField::Test.key(), "asking…".into());
        handle.spawn(async move {
            let _ = tx.send(decision::test_connection(&config).await).await;
        });
    }

    /// Fold in the test's answer once it lands. Called from the frame loop.
    pub(crate) fn drain_decision_test(&mut self) {
        let Some(rx) = self.decision_test.as_mut() else {
            return;
        };
        let shown = match rx.try_recv() {
            Ok(Ok(fine)) => fine,
            Ok(Err(why)) => format!("failed: {why}"),
            Err(mpsc::error::TryRecvError::Empty) => return,
            Err(mpsc::error::TryRecvError::Disconnected) => "the test stopped".into(),
        };
        self.decision_test = None;
        self.status = shown.clone();
        if self.modal == Modal::Decision {
            self.settings.dec.insert(DecField::Test.key(), shown);
        }
    }
}
