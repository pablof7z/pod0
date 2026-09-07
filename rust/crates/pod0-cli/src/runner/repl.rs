use std::borrow::Cow::{self, Owned};
use std::path::PathBuf;

use rustyline::completion::{Candidate, Completer};
use rustyline::error::ReadlineError;
use rustyline::highlight::{CmdKind, Highlighter};
use rustyline::hint::{Hinter, HistoryHinter};
use rustyline::validate::Validator;
use rustyline::{CompletionType, Config, Context, EditMode, Editor, Helper};

mod help;
mod render;

use help::print_help;
use render::{print, render_response};

use crate::app::Shell;
use crate::protocol::PROTOCOL_VERSION;

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
