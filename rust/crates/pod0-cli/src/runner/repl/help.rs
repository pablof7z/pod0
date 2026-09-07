pub(super) fn print_help(line: &str) {
    let target = line
        .strip_prefix("help ")
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let blocks = help_blocks();
    for (name, body) in blocks {
        if let Some(wanted) = target {
            if name == wanted || is_alias(name, wanted) {
                println!("\x1b[1;36m{name}\x1b[0m");
                println!("{body}");
                return;
            }
        } else {
            println!("\x1b[1;36m{name}\x1b[0m");
            println!("{body}");
            println!();
        }
    }
    if let Some(wanted) = target {
        eprintln!(
            "\x1b[1;33mno help for `{wanted}`\x1b[0m — type \x1b[36mhelp\x1b[0m for all commands."
        );
    }
}

fn is_alias(name: &str, wanted: &str) -> bool {
    matches!(
        (name, wanted),
        ("create_store", "create")
            | ("open_store", "open")
            | ("subscribe_feed", "subscribe")
            | ("search_podcasts", "search")
            | ("settings_get", "settings")
            | ("exit", "quit")
    )
}

fn help_blocks() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "status",
            "  Show the store path, facade contract version, and capability flags.\n  \x1b[2mREPL:\x1b[0m status",
        ),
        (
            "help",
            "  Show all commands, or usage for one: \x1b[36mhelp search_podcasts\x1b[0m",
        ),
        (
            "create_store",
            "  Create a new authoritative store (refuses to overwrite an existing one).\n  \x1b[2mREPL:\x1b[0m create ./pod0.sqlite\n  \x1b[2mJSON:\x1b[0m {\"v\":1,\"command\":\"create_store\",\"path\":\"./pod0.sqlite\"}",
        ),
        (
            "open_store",
            "  Open an existing authoritative store.\n  \x1b[2mREPL:\x1b[0m open ./pod0.sqlite",
        ),
        (
            "subscribe_feed",
            "  Subscribe to an RSS feed by URL (real HTTP fetch + parse).\n  \x1b[2mREPL:\x1b[0m subscribe_feed https://feeds.example.com/show.rss\n  \x1b[2mJSON:\x1b[0m {\"v\":1,\"command\":\"subscribe_feed\",\"feed_url\":\"https://…\",\"limit\":100}",
        ),
        (
            "search_podcasts",
            "  Search the iTunes podcast directory by term. Returns titles, authors, and feed URLs.\n  \x1b[2mREPL:\x1b[0m search acquired\n  \x1b[2mJSON:\x1b[0m {\"v\":1,\"command\":\"search_podcasts\",\"term\":\"acquired\",\"limit\":25}",
        ),
        (
            "settings_get",
            "  Read settings: notifications, playback, recall, workflow, and subscription pages.\n  \x1b[2mREPL:\x1b[0m settings",
        ),
        (
            "settings_set",
            "  Write a setting. Kinds: NewEpisodeNotifications, SubscriptionNotifications,\n  SubscriptionAutoDownload, SubscriptionTranscriptPolicy, PlaybackRate,\n  PlaybackPreferences, Recall, Workflow.\n  \x1b[2mJSON:\x1b[0m {\"v\":1,\"command\":\"settings_set\",\"setting\":{\"Recall\":{\"stored_embedding_model_id\":\"openai/text-embedding-3-large\",\"reranker_enabled\":true}}}",
        ),
        (
            "ask_agent",
            "  Ask the configured agent a question (real model turn via OpenAI-compatible or Ollama).\n  \x1b[2mREPL:\x1b[0m ask_agent summarize the latest episode\n  \x1b[2mJSON:\x1b[0m {\"v\":1,\"command\":\"ask_agent\",\"input\":\"…\",\"provider\":\"ollama\",\"model\":\"llama3\"}",
        ),
        (
            "host_drain",
            "  Inspect pending durable host work without claiming or mutating it.\n  \x1b[2mREPL:\x1b[0m host_drain",
        ),
        (
            "library",
            "  List your subscribed podcasts and their episodes (read-only projection).\n  \x1b[2mREPL:\x1b[0m library\n  \x1b[2maliases:\x1b[0m subscriptions, shows\n  \x1b[2mJSON:\x1b[0m {\"v\":1,\"command\":\"library\",\"offset\":0,\"limit\":100}",
        ),
        (
            "play",
            "  Start playback of an episode by its id (from `library`). Real audio output.\n  \x1b[2mREPL:\x1b[0m play <episode_id>\n  \x1b[2mJSON:\x1b[0m {\"v\":1,\"command\":\"play\",\"episode_id\":\"<32 hex>\"}",
        ),
        (
            "pause",
            "  Pause the current episode and persist the playhead.\n  \x1b[2mREPL:\x1b[0m pause",
        ),
        (
            "resume",
            "  Resume the paused episode.\n  \x1b[2mREPL:\x1b[0m resume",
        ),
        (
            "clear",
            "  Clear the screen. (\x1b[2mcls\x1b[0m also works.)",
        ),
        (
            "exit",
            "  Exit the REPL. (\x1b[2mquit\x1b[0m, Ctrl-D, and Ctrl-C also exit.)",
        ),
    ]
}
