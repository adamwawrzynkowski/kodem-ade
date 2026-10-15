//! The Git workspace's saved settings as a GPUI global.
use gpui::{App, Global};
use kodem_git::git::{AccountCredential, set_account_credentials};
use kodem_git::settings::Settings;

/// The Git workspace's settings, loaded once and saved on each change.
pub struct GitSettings {
    pub settings: Settings,
}

impl Global for GitSettings {}

impl GitSettings {
    /// Loads the saved settings into the app and gives Git the linked accounts' tokens.
    pub fn init(cx: &mut App) {
        if cx.has_global::<GitSettings>() {
            return;
        }
        let settings = Settings::load();
        sync_account_credentials(&settings);
        cx.set_global(GitSettings { settings });
    }

    /// Returns the settings.
    pub fn get(cx: &App) -> &Settings {
        &cx.global::<GitSettings>().settings
    }

    /// Changes the settings, saves them and redraws every window.
    pub fn update(cx: &mut App, f: impl FnOnce(&mut Settings)) {
        Self::update_saved(cx, f);
        cx.refresh_windows();
    }

    /// Changes the settings and saves them without a redraw.
    pub fn update_saved(cx: &mut App, f: impl FnOnce(&mut Settings)) {
        let state = cx.global_mut::<GitSettings>();
        f(&mut state.settings);
        state.settings.save();
        sync_account_credentials(&state.settings);
    }

    /// Saves the current settings.
    pub fn save(cx: &mut App) {
        cx.global::<GitSettings>().settings.save();
    }
}

/// Gives Git the tokens of the linked accounts, so HTTPS remotes on
/// their hosts work without a separate keychain login.
pub fn sync_account_credentials(settings: &Settings) {
    set_account_credentials(
        settings
            .accounts
            .iter()
            .map(|account| AccountCredential {
                host: account.host.clone(),
                username: account.username.clone(),
                token: account.token.clone(),
            })
            .collect(),
    );
}
