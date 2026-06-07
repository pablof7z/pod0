#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Library,
    Episodes,
    Player,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Library,
    Queue,
    Inbox,
    Search,
    Downloads,
    Bookmarks,
    Clips,
    Agent,
    Wiki,
    Social,
    Settings,
}

impl Tab {
    pub fn all() -> &'static [Self] {
        &[
            Self::Library,
            Self::Queue,
            Self::Inbox,
            Self::Search,
            Self::Downloads,
            Self::Bookmarks,
            Self::Clips,
            Self::Agent,
            Self::Wiki,
            Self::Social,
            Self::Settings,
        ]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Library => "library",
            Self::Queue => "queue",
            Self::Inbox => "inbox",
            Self::Search => "search",
            Self::Downloads => "downloads",
            Self::Bookmarks => "stars",
            Self::Clips => "clips",
            Self::Agent => "agent",
            Self::Wiki => "wiki",
            Self::Social => "social",
            Self::Settings => "settings",
        }
    }

    pub fn next(self) -> Self {
        let tabs = Self::all();
        let index = tabs.iter().position(|tab| *tab == self).unwrap_or(0);
        tabs[(index + 1) % tabs.len()]
    }

    pub fn previous(self) -> Self {
        let tabs = Self::all();
        let index = tabs.iter().position(|tab| *tab == self).unwrap_or(0);
        tabs[(index + tabs.len() - 1) % tabs.len()]
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Normal,
    SearchInput,
    SubscribeInput,
    RelayInput,
    SettingsInput,
    ProviderCatalog,
    AgentInput,
    AgentMemoryInput,
    AgentTaskInput,
    AgentNoteInput,
    EpisodeCommentInput,
    EpisodeDetail { scroll: usize },
}
