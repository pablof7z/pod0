use std::borrow::Cow::{self, Owned};
use std::path::PathBuf;

use rustyline::completion::{Candidate, Completer};
use rustyline::error::ReadlineError;
use rustyline::highlight::{CmdKind, Highlighter};
use rustyline::hint::{Hinter, HistoryHinter};
use rustyline::validate::Validator;
use rustyline::{
    CompletionType, Config, Context, EditMode, Editor, Helper,
};

use crate::app::Shell;
use crate::protocol::{CliResponse, PROTOCOL_VERSION};

/// All recognized REPL tokens (full protocol commands + shorthands), used for
/// tab completion and input highlighting.
const COMMANDS: &[&str] = &[
    "status",
    "help",
    "create_store",
    "open_store",
    "subscribe_feed",
    "search_podcasts",
    "settings_get",
    "settings_set",
    "ask_agent",
    "host_drain",
    "library",
    "subscriptions",
    "shows",
    "play",
    "pause",
    "resume",
    "exit",
    "quit",
    "clear",
    // shorthands
    "create",
    "open",
    "subscribe",
    "search",
    "settings",
];

struct CommandCandidate {
    display: String,
}

impl Candidate for CommandCandidate {
    fn display(&self) -> &str {
        &self.display
    }
    fn replacement(&self) -> &str {
        &self.display
    }
}

struct ReplHelper {
    hinter: HistoryHinter,
}

impl Completer for ReplHelper {
    type Candidate = CommandCandidate;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Self::Candidate>)> {
        let upto = &line[..pos];
        // Only complete the first token, before any whitespace.
        if upto.contains(char::is_whitespace) {
            return Ok((pos, Vec::new()));
        }
        let start = pos - upto.len();
        let matches: Vec<CommandCandidate> = COMMANDS
            .iter()
            .filter(|command| command.starts_with(upto))
            .map(|command| CommandCandidate {
                display: (*command).to_owned(),
            })
            .collect();
        Ok((start, matches))
    }
}

impl Hinter for ReplHelper {
    type Hint = String;

    fn hint(&self, line: &str, pos: usize, ctx: &Context<'_>) -> Option<Self::Hint> {
        // History-based hints (from rustyline) give inline suggestions drawn
        // from the persistent history file.
        self.hinter.hint(line, pos, ctx)
    }
}

impl Validator for ReplHelper {}

impl Highlighter for ReplHelper {
    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        Owned(format!("\x1b[2;3;37m{hint}\x1b[0m"))
    }

    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        let trimmed = line.trim_start();
        let leading_ws_len = line.len() - trimmed.len();
        let (keyword, rest) = trimmed
            .split_once(char::is_whitespace)
            .unwrap_or((trimmed, ""));
        if COMMANDS.contains(&keyword) {
            Owned(format!(
                "{}\x1b[1;36m{}\x1b[0m{}",
                &line[..leading_ws_len],
                keyword,
                rest
            ))
        } else {
            Cow::Borrowed(line)
        }
    }

    fn highlight_char(&self, _line: &str, _pos: usize, kind: CmdKind) -> bool {
        !matches!(kind, CmdKind::Other)
    }
}

impl Helper for ReplHelper {}

fn history_path() -> PathBuf {
    std::env::var("POD0_CLI_HISTORY")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_owned());
            PathBuf::from(home).join(".pod0").join("cli_history")
        })
}

pub(super) fn run_repl(shell: &mut Shell) -> Result<(), String> {
    let history = history_path();
    if let Some(parent) = history.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let config = Config::builder()
        .history_ignore_space(true)
        .history_ignore_dups(true)
        .and_then(|builder| builder.max_history_size(10_000))
        .map(|builder| {
            builder
                .completion_type(CompletionType::List)
                .edit_mode(EditMode::Emacs)
                .build()
        })
        .map_err(|error| error.to_string())?;
    let helper = ReplHelper {
        hinter: HistoryHinter::new(),
    };
    let mut rl = Editor::with_config(config).map_err(|error| error.to_string())?;
    rl.set_helper(Some(helper));
    let _ = rl.load_history(&history);

    print_banner();
    let prompt = "pod0> ";
    let colored_prompt = "\x1b[1;36mpod0\x1b[0m \x1b[2m❯\x1b[0m ";

    loop {
        let readline = rl.readline(&(prompt, colored_prompt));
        match readline {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let _ = rl.add_history_entry(trimmed);
                let _ = rl.save_history(&history);
                if matches!(trimmed, "exit" | "quit") {
                    break;
                }
                if matches!(trimmed, "clear" | "cls") {
                    print("\x1b[2J\x1b[H");
                    continue;
                }
                if trimmed == "help" || trimmed.starts_with("help ") {
                    print_help(trimmed);
                    continue;
                }
                let request = match super::parse_repl_request(trimmed) {
                    Ok(request) => request,
                    Err(error) => {
                        eprintln!("\x1b[1;31m✗\x1b[0m {}", error.message);
                        continue;
                    }
                };
                let response = shell.handle(request);
                render_response(&response);
                if shell.exiting() {
                    break;
                }
            }
            Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => break,
            Err(error) => {
                eprintln!("\x1b[1;31mrepl error:\x1b[0m {error}");
                break;
            }
        }
    }
    let _ = rl.save_history(&history);
    Ok(())
}

fn print_banner() {
    println!(
        "\x1b[1mPod0 CLI\x1b[0m \x1b[2mprotocol v{PROTOCOL_VERSION}\x1b[0m — type \
         \x1b[36mhelp\x1b[0m for commands, \x1b[36mexit\x1b[0m (or Ctrl-D) to quit."
    );
    println!(
        "\x1b[2mREPL: emacs keys, ↑/Ctrl-R history, Tab completes commands. \
         JSON requests start with {{.\x1b[0m"
    );
}

fn render_response(response: &CliResponse) {
    let value = match serde_json::to_value(response) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("\x1b[1;31m✗\x1b[0m could not serialize response: {error}");
            return;
        }
    };
    if response.ok {
        println!("\x1b[1;32m● ok\x1b[0m");
        if let Some(result) = value.get("result") {
            print_colored_json(result, 0);
        }
    } else {
        let code = value
            .pointer("/error/code")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("error");
        let message = value
            .pointer("/error/message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let retryable = value
            .pointer("/error/retryable")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        println!(
            "\x1b[1;31m✗ {code}\x1b[0m \x1b[2m{}{}\x1b[0m",
            if retryable { "(retryable) " } else { "" },
            message,
        );
        if let Some(error) = value.get("error") {
            print_colored_json(error, 0);
        }
    }
}

fn print_colored_json(value: &serde_json::Value, indent: usize) {
    let pad = "  ".repeat(indent);
    match value {
        serde_json::Value::Null => println!("\x1b[2m{pad}null\x1b[0m"),
        serde_json::Value::Bool(b) => println!("\x1b[35m{pad}{b}\x1b[0m"),
        serde_json::Value::Number(n) => println!("\x1b[33m{pad}{n}\x1b[0m"),
        serde_json::Value::String(s) => {
            println!("\x1b[32m{pad}\"{}\"\x1b[0m", escape_json_string(s))
        }
        serde_json::Value::Array(items) => {
            if items.is_empty() {
                println!("\x1b[2m{pad}[]\x1b[0m");
                return;
            }
            println!("{pad}[");
            for item in items {
                print_colored_json(item, indent + 1);
            }
            println!("{pad}]");
        }
        serde_json::Value::Object(map) => {
            if map.is_empty() {
                println!("\x1b[2m{pad}{{}}\x1b[0m");
                return;
            }
            for (key, val) in map {
                match val {
                    serde_json::Value::Array(items) if !items.is_empty() => {
                        println!("\x1b[36m{pad}{key}\x1b[0m:");
                        print_colored_json(val, indent + 1);
                    }
                    serde_json::Value::Object(inner) if !inner.is_empty() => {
                        println!("\x1b[36m{pad}{key}\x1b[0m:");
                        print_colored_json(val, indent + 1);
                    }
                    _ => {
                        print!("\x1b[36m{pad}{key}\x1b[0m: ");
                        print_inline(val);
                        println!();
                    }
                }
            }
        }
    }
}

fn print_inline(value: &serde_json::Value) {
    match value {
        serde_json::Value::Null => print!("\x1b[2mnull\x1b[0m"),
        serde_json::Value::Bool(b) => print!("\x1b[35m{b}\x1b[0m"),
        serde_json::Value::Number(n) => print!("\x1b[33m{n}\x1b[0m"),
        serde_json::Value::String(s) => {
            print!("\x1b[32m\"{}\"\x1b[0m", escape_json_string(s))
        }
        serde_json::Value::Array(items) => {
            print!("\x1b[2m[\x1b[0m");
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    print!("\x1b[2m, \x1b[0m");
                }
                print_inline(item);
            }
            print!("\x1b[2m]\x1b[0m");
        }
        serde_json::Value::Object(map) => {
            print!("\x1b[2m{{\x1b[0m");
            for (i, (k, v)) in map.iter().enumerate() {
                if i > 0 {
                    print!("\x1b[2m, \x1b[0m");
                }
                print!("\x1b[36m{k}\x1b[0m: ");
                print_inline(v);
            }
            print!("\x1b[2m}}\x1b[0m");
        }
    }
}

fn escape_json_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn print(text: &str) {
    use std::io::Write as _;
    let mut stdout = std::io::stdout().lock();
    let _ = stdout.write_all(text.as_bytes());
    let _ = stdout.flush();
}

fn print_help(line: &str) {
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
        ("status", "  Show the store path, facade contract version, and capability flags.\n  \x1b[2mREPL:\x1b[0m status"),
        ("help", "  Show all commands, or usage for one: \x1b[36mhelp search_podcasts\x1b[0m"),
        ("create_store", "  Create a new authoritative store (refuses to overwrite an existing one).\n  \x1b[2mREPL:\x1b[0m create ./pod0.sqlite\n  \x1b[2mJSON:\x1b[0m {\"v\":1,\"command\":\"create_store\",\"path\":\"./pod0.sqlite\"}"),
        ("open_store", "  Open an existing authoritative store.\n  \x1b[2mREPL:\x1b[0m open ./pod0.sqlite"),
        ("subscribe_feed", "  Subscribe to an RSS feed by URL (real HTTP fetch + parse).\n  \x1b[2mREPL:\x1b[0m subscribe_feed https://feeds.example.com/show.rss\n  \x1b[2mJSON:\x1b[0m {\"v\":1,\"command\":\"subscribe_feed\",\"feed_url\":\"https://…\",\"limit\":100}"),
        ("search_podcasts", "  Search the iTunes podcast directory by term. Returns titles, authors, and feed URLs.\n  \x1b[2mREPL:\x1b[0m search acquired\n  \x1b[2mJSON:\x1b[0m {\"v\":1,\"command\":\"search_podcasts\",\"term\":\"acquired\",\"limit\":25}"),
        ("settings_get", "  Read settings: notifications, playback, recall, workflow, and subscription pages.\n  \x1b[2mREPL:\x1b[0m settings"),
        ("settings_set", "  Write a setting. Kinds: NewEpisodeNotifications, SubscriptionNotifications,\n  SubscriptionAutoDownload, SubscriptionTranscriptPolicy, PlaybackRate,\n  PlaybackPreferences, Recall, Workflow.\n  \x1b[2mJSON:\x1b[0m {\"v\":1,\"command\":\"settings_set\",\"setting\":{\"Recall\":{\"stored_embedding_model_id\":\"openai/text-embedding-3-large\",\"reranker_enabled\":true}}}"),
        ("ask_agent", "  Ask the configured agent a question (real model turn via OpenAI-compatible or Ollama).\n  \x1b[2mREPL:\x1b[0m ask_agent summarize the latest episode\n  \x1b[2mJSON:\x1b[0m {\"v\":1,\"command\":\"ask_agent\",\"input\":\"…\",\"provider\":\"ollama\",\"model\":\"llama3\"}"),
        ("host_drain", "  Inspect pending durable host work without claiming or mutating it.\n  \x1b[2mREPL:\x1b[0m host_drain"),
        ("library", "  List your subscribed podcasts and their episodes (read-only projection).\n  \x1b[2mREPL:\x1b[0m library\n  \x1b[2maliases:\x1b[0m subscriptions, shows\n  \x1b[2mJSON:\x1b[0m {\"v\":1,\"command\":\"library\",\"offset\":0,\"limit\":100}"),
        ("play", "  Start playback of an episode by its id (from `library`). Real audio output.\n  \x1b[2mREPL:\x1b[0m play <episode_id>\n  \x1b[2mJSON:\x1b[0m {\"v\":1,\"command\":\"play\",\"episode_id\":\"<32 hex>\"}"),
        ("pause", "  Pause the current episode and persist the playhead.\n  \x1b[2mREPL:\x1b[0m pause"),
        ("resume", "  Resume the paused episode.\n  \x1b[2mREPL:\x1b[0m resume"),
        ("clear", "  Clear the screen. (\x1b[2mcls\x1b[0m also works.)"),
        ("exit", "  Exit the REPL. (\x1b[2mquit\x1b[0m, Ctrl-D, and Ctrl-C also exit.)"),
    ]
}