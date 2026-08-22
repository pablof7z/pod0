mod input;
mod repl;

use std::io::{IsTerminal as _, Write as _};

use crate::app::Shell;
use crate::host::HostConfig;
use crate::protocol::{CliCommand, CliError, CliRequest, CliResponse, PROTOCOL_VERSION};
use input::{InputFrame, read_frame};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CliMode {
    Repl,
    Ndjson,
}

pub fn run(arguments: impl Iterator<Item = String>) -> Result<(), String> {
    let options = Options::parse(arguments)?;
    let mut shell = Shell::new(HostConfig::from_env()).map_err(|error| error.message)?;
    if let Some(path) = options.store {
        let response = shell.handle(CliRequest {
            version: PROTOCOL_VERSION,
            request_id: Some("startup".to_owned()),
            command: if options.create {
                CliCommand::CreateStore { path }
            } else {
                CliCommand::OpenStore { path }
            },
        });
        if !response.ok {
            write_response(options.mode, &response)?;
            return Ok(());
        }
    }
    match options.mode {
        CliMode::Ndjson => run_ndjson(&mut shell),
        CliMode::Repl => run_repl(&mut shell),
    }
}

struct Options {
    mode: CliMode,
    store: Option<String>,
    create: bool,
}

impl Options {
    fn parse(mut arguments: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut mode = None;
        let mut store = None;
        let mut create = false;
        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--json" | "--ndjson" => mode = Some(CliMode::Ndjson),
                "--repl" => mode = Some(CliMode::Repl),
                "--create" => create = true,
                "--store" => {
                    store = Some(
                        arguments
                            .next()
                            .ok_or_else(|| "--store requires a path".to_owned())?,
                    );
                }
                _ => return Err(format!("unknown argument: {argument}")),
            }
        }
        if create && store.is_none() {
            return Err("--create requires --store".to_owned());
        }
        Ok(Self {
            mode: mode.unwrap_or_else(|| {
                if std::io::stdin().is_terminal() {
                    CliMode::Repl
                } else {
                    CliMode::Ndjson
                }
            }),
            store,
            create,
        })
    }
}

fn run_ndjson(shell: &mut Shell) -> Result<(), String> {
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let mut stdout = std::io::stdout().lock();
    loop {
        let response = match read_frame(&mut input).map_err(|_| "stdin read failed")? {
            InputFrame::Line(line) if line.trim().is_empty() => continue,
            InputFrame::Line(line) => parse_request(&line).map_or_else(
                |error| CliResponse::failure(None, error),
                |request| shell.handle(request),
            ),
            InputFrame::Oversized => oversized_response(),
            InputFrame::InvalidUtf8 => invalid_utf8_response(),
            InputFrame::Eof => break,
        };
        serde_json::to_writer(&mut stdout, &response).map_err(|_| "stdout write failed")?;
        writeln!(stdout).map_err(|_| "stdout write failed")?;
        stdout.flush().map_err(|_| "stdout flush failed")?;
        if shell.exiting() {
            break;
        }
    }
    Ok(())
}

fn run_repl(shell: &mut Shell) -> Result<(), String> {
    repl::run_repl(shell)
}

fn parse_request(line: &str) -> Result<CliRequest, CliError> {
    serde_json::from_str(line)
        .map_err(|_| CliError::new("invalid_json", "request is not valid protocol JSON", false))
}

fn parse_repl_request(line: &str) -> Result<CliRequest, CliError> {
    if line.starts_with('{') {
        return parse_request(line);
    }
    let command = match line {
        "status" => CliCommand::Status,
        "help" => CliCommand::Help,
        "settings" | "settings_get" => CliCommand::SettingsGet {
            offset: 0,
            limit: 100,
        },
        "host_drain" => CliCommand::HostDrain { limit: 32 },
        "library" | "subscriptions" | "shows" => CliCommand::Library {
            offset: 0,
            limit: 100,
        },
        "pause" => CliCommand::Pause,
        "resume" => CliCommand::Resume,
        _ if line.starts_with("play ") => CliCommand::Play {
            episode_id: line[5..].trim().to_owned(),
        },
        "exit" | "quit" => CliCommand::Exit,
        _ if line.starts_with("create ") => CliCommand::CreateStore {
            path: line[7..].trim().to_owned(),
        },
        _ if line.starts_with("open ") => CliCommand::OpenStore {
            path: line[5..].trim().to_owned(),
        },
        _ if line.starts_with("subscribe_feed ") => CliCommand::SubscribeFeed {
            feed_url: line[15..].trim().to_owned(),
            offset: 0,
            limit: 100,
        },
        _ if line.starts_with("search ") => CliCommand::SearchPodcasts {
            term: line[7..].trim().to_owned(),
            limit: 25,
        },
        _ if line.starts_with("search_podcasts ") => CliCommand::SearchPodcasts {
            term: line[16..].trim().to_owned(),
            limit: 25,
        },
        _ if line.starts_with("ask_agent ") => CliCommand::AskAgent {
            input: line[10..].trim().to_owned(),
            conversation_id: None,
            provider: None,
            model: None,
        },
        _ => {
            return Err(CliError::new(
                "unknown_command",
                "unknown REPL command; use help or a JSON request",
                false,
            ));
        }
    };
    Ok(CliRequest {
        version: PROTOCOL_VERSION,
        request_id: None,
        command,
    })
}

fn oversized_response() -> CliResponse {
    CliResponse::failure(
        None,
        CliError::new(
            "input_too_large",
            "input frame exceeds the 256 KiB limit",
            false,
        ),
    )
}

fn invalid_utf8_response() -> CliResponse {
    CliResponse::failure(
        None,
        CliError::new("invalid_json", "input frame is not valid UTF-8", false),
    )
}

fn write_response(mode: CliMode, response: &CliResponse) -> Result<(), String> {
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    match mode {
        CliMode::Ndjson => serde_json::to_writer(&mut output, response),
        CliMode::Repl => serde_json::to_writer_pretty(&mut output, response),
    }
    .map_err(|_| "stdout write failed")?;
    writeln!(output).map_err(|_| "stdout write failed".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_parses_versioned_requests_and_rejects_bad_json() {
        let request = parse_request(r#"{"v":1,"request_id":"a","command":"status"}"#).unwrap();
        assert_eq!(request.version, 1);
        assert_eq!(request.request_id.as_deref(), Some("a"));
        assert_eq!(parse_request("not-json").unwrap_err().code, "invalid_json");
    }
}
