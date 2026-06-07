use crate::app::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSection {
    General,
    Providers,
    Relays,
}

impl SettingsSection {
    pub fn all() -> &'static [Self] {
        &[Self::General, Self::Providers, Self::Relays]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Providers => "providers",
            Self::Relays => "relays",
        }
    }

    fn next(self) -> Self {
        let sections = Self::all();
        let index = sections
            .iter()
            .position(|section| *section == self)
            .unwrap_or(0);
        sections[(index + 1) % sections.len()]
    }

    fn previous(self) -> Self {
        let sections = Self::all();
        let index = sections
            .iter()
            .position(|section| *section == self)
            .unwrap_or(0);
        sections[(index + sections.len() - 1) % sections.len()]
    }
}

impl AppState {
    pub fn next_settings_section(&mut self) {
        self.settings_section = self.settings_section.next();
    }

    pub fn previous_settings_section(&mut self) {
        self.settings_section = self.settings_section.previous();
    }

    pub fn next_provider_setting(&mut self, count: usize) {
        advance_index(&mut self.selected_provider_setting, count);
    }

    pub fn previous_provider_setting(&mut self) {
        self.selected_provider_setting = self.selected_provider_setting.saturating_sub(1);
    }

    pub fn jump_provider_setting_top(&mut self) {
        self.selected_provider_setting = 0;
    }

    pub fn jump_provider_setting_bottom(&mut self, count: usize) {
        self.selected_provider_setting = count.saturating_sub(1);
    }

    pub fn next_relay(&mut self) {
        advance_index(&mut self.selected_relay, self.configured_relays.len());
    }

    pub fn previous_relay(&mut self) {
        self.selected_relay = self.selected_relay.saturating_sub(1);
    }

    pub fn jump_relay_top(&mut self) {
        self.selected_relay = 0;
    }

    pub fn jump_relay_bottom(&mut self) {
        self.selected_relay = self.configured_relays.len().saturating_sub(1);
    }

    pub fn selected_relay_url(&self) -> Option<String> {
        self.configured_relays
            .get(self.selected_relay)
            .map(|relay| relay.url.clone())
    }

    pub fn selected_relay_role(&self) -> Option<String> {
        self.configured_relays
            .get(self.selected_relay)
            .map(|relay| relay.role.clone())
    }

    pub fn next_provider_catalog_model(&mut self, count: usize) {
        advance_index(&mut self.selected_provider_catalog_model, count);
    }

    pub fn previous_provider_catalog_model(&mut self) {
        self.selected_provider_catalog_model =
            self.selected_provider_catalog_model.saturating_sub(1);
    }

    pub fn jump_provider_catalog_top(&mut self) {
        self.selected_provider_catalog_model = 0;
    }

    pub fn jump_provider_catalog_bottom(&mut self, count: usize) {
        self.selected_provider_catalog_model = count.saturating_sub(1);
    }
}

pub fn next_relay_role(current: &str) -> &'static str {
    const ROLES: [&str; 5] = ["read", "write", "both", "indexer", "both,indexer"];
    let index = ROLES.iter().position(|role| *role == current).unwrap_or(1);
    ROLES[(index + 1) % ROLES.len()]
}

fn advance_index(index: &mut usize, len: usize) {
    if len > 0 {
        *index = (*index + 1).min(len - 1);
    }
}
