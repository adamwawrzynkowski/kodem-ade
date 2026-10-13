//! Serves Kodem's own tools to a provider agent, as an MCP server over standard input and output.
//!
//! A provider CLI starts this process for one task: `kodem-host mcp --root <dir> --task <id>`.
//! The process holds no state. Each tool call is one request to the host that owns the task,
//! over the host's local socket and with the checks that the socket applies to every client.
use anyhow::{Context, Result, bail};
use kodem_core::{Delegation, Provider, Request, Response, ipc};
use serde_json::{Value, json};
use std::{
    io::{BufRead, Write},
    path::{Path, PathBuf},
};

/// Name under which a provider lists this server and prefixes its tools.
pub(crate) const SERVER: &str = "kodem";
/// Longest wait of one tool call, in seconds. It stays below the shortest default limit that a
/// provider sets for a tool call, so a long task is read with several calls.
const WAIT_LIMIT: u64 = 45;
const INSTRUCTIONS: &str = "Kodem runs this session. Its tools start subagents as Kodem tasks on any provider and model that the user has installed, for example a Codex model from a Claude Code session or the reverse. Use `delegate_task` when the user asks for a subagent on another provider or on a model that your own subagent tool does not offer.";

/// Returns the command line that starts this server for one task of this host.
pub(crate) fn command(task_id: &str) -> Result<(PathBuf, Vec<String>)> {
    let root = ipc::data_dir()?;
    Ok((
        std::env::current_exe()?,
        vec![
            "mcp".into(),
            "--root".into(),
            root.to_str()
                .context("Kodem data path is not UTF-8")?
                .to_owned(),
            "--task".into(),
            task_id.into(),
        ],
    ))
}

// The value that follows an option on the command line of this server.
fn option<'a>(arguments: &'a [String], name: &str) -> Result<&'a str> {
    arguments
        .iter()
        .position(|argument| argument == name)
        .and_then(|index| arguments.get(index + 1))
        .map(String::as_str)
        .with_context(|| format!("Missing {name}"))
}

/// Answers the requests of one provider process until it closes the input.
pub(crate) fn serve(arguments: &[String]) -> Result<()> {
    let root = PathBuf::from(option(arguments, "--root")?);
    let task = option(arguments, "--task")?.to_owned();
    let mut output = std::io::stdout().lock();
    for line in std::io::stdin().lock().lines() {
        let line = line?;
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        // A message without an identity is a notification and gets no answer.
        let Some(id) = message.get("id").filter(|id| !id.is_null()) else {
            continue;
        };
        let reply = match answer(&root, &task, &message) {
            Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
            Err(error) => json!({"jsonrpc":"2.0","id":id,
                "error":{"code":-32601,"message":format!("{error:#}")}}),
        };
        serde_json::to_writer(&mut output, &reply)?;
        output.write_all(b"\n")?;
        output.flush()?;
    }
    Ok(())
}

// The result of one protocol request. A failed tool is a result that says so, because the agent
// must read the reason; only an unknown method is a protocol error.
fn answer(root: &Path, task: &str, message: &Value) -> Result<Value> {
    let parameters = &message["params"];
    match message["method"].as_str().unwrap_or_default() {
        "initialize" => Ok(json!({
            "protocolVersion": parameters["protocolVersion"].as_str().unwrap_or("2025-06-18"),
            "capabilities": {"tools": {}},
            "serverInfo": {"name": SERVER, "title": "Kodem", "version": env!("CARGO_PKG_VERSION")},
            "instructions": INSTRUCTIONS,
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools": tools()})),
        "tools/call" => {
            let name = parameters["name"].as_str().unwrap_or_default();
            Ok(match call(root, task, name, &parameters["arguments"]) {
                Ok(text) => json!({"content":[{"type":"text","text":text}],"isError":false}),
                Err(error) => {
                    json!({"content":[{"type":"text","text":format!("{error:#}")}],"isError":true})
                }
            })
        }
        method => bail!("Kodem does not implement {method}"),
    }
}

fn tools() -> Value {
    json!([
        {
            "name": "list_agents",
            "description": "List the providers and models that delegate_task can use in this Kodem installation.",
            "inputSchema": {"type":"object","properties":{},"additionalProperties":false}
        },
        {
            "name": "delegate_task",
            "description": "Start a subagent as a Kodem task on any available provider and model, give it a task, and wait for its result. The subagent sees only the task text you write, so include all the context it needs. Without `write` it reads the files of your checkout and changes nothing. With `write` it works in a worktree and branch of its own, started from your present files, and you read its changes from that branch. If the result is not ready in time, the answer names the task; call task_status to wait further.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task": {"type":"string","description":"The complete task for the subagent."},
                    "model": {"type":"string","description":"Model to use, by name or identity as list_agents shows it, for example \"gpt-6.1-sol\". Omit it for the provider's default model."},
                    "provider": {"type":"string","enum":["codex","claude"],"description":"Provider to use when no model is named. Omit it to stay on your own provider."},
                    "write": {"type":"boolean","description":"Let the subagent change files, in a worktree of its own. Default false."},
                    "wait_seconds": {"type":"integer","minimum":0,"maximum":WAIT_LIMIT,"description":"How long to wait for the result. Default 40."}
                },
                "required": ["task"],
                "additionalProperties": false
            }
        },
        {
            "name": "task_status",
            "description": "Wait for a task that delegate_task started and read its state and result.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task_id": {"type":"string"},
                    "wait_seconds": {"type":"integer","minimum":0,"maximum":WAIT_LIMIT,"description":"How long to wait for the result. Default 40."}
                },
                "required": ["task_id"],
                "additionalProperties": false
            }
        },
        {
            "name": "task_cancel",
            "description": "Stop the running turn of a task that delegate_task started.",
            "inputSchema": {
                "type": "object",
                "properties": {"task_id": {"type":"string"}},
                "required": ["task_id"],
                "additionalProperties": false
            }
        }
    ])
}

fn wait_ms(arguments: &Value) -> u64 {
    arguments["wait_seconds"]
        .as_u64()
        .unwrap_or(40)
        .min(WAIT_LIMIT)
        * 1000
}

fn provider(name: Option<&str>) -> Result<Option<Provider>> {
    Ok(match name.map(str::to_lowercase).as_deref() {
        None => None,
        Some("codex" | "openai" | "gpt") => Some(Provider::Codex),
        Some("claude" | "claude code" | "anthropic") => Some(Provider::Claude),
        Some(other) => bail!("Unknown provider \"{other}\". Use \"codex\" or \"claude\"."),
    })
}

// The name of a provider as the tools take it.
fn provider_name(provider: Provider) -> &'static str {
    match provider {
        Provider::Codex => "codex",
        Provider::Claude => "claude",
    }
}

// Runs one tool and returns the text that the agent reads.
fn call(root: &Path, task: &str, name: &str, arguments: &Value) -> Result<String> {
    let task_id = || {
        arguments["task_id"]
            .as_str()
            .map(str::to_owned)
            .context("Name the task_id that delegate_task returned")
    };
    let request = match name {
        "list_agents" => Request::Agents {
            parent: task.into(),
        },
        "delegate_task" => Request::Delegate {
            parent: task.into(),
            prompt: arguments["task"]
                .as_str()
                .context("Write the task for the subagent")?
                .to_owned(),
            provider: provider(arguments["provider"].as_str())?,
            model: arguments["model"].as_str().map(str::to_owned),
            write: arguments["write"].as_bool().unwrap_or(false),
            wait_ms: wait_ms(arguments),
        },
        "task_status" => Request::DelegateStatus {
            parent: task.into(),
            task_id: task_id()?,
            wait_ms: wait_ms(arguments),
        },
        "task_cancel" => Request::DelegateCancel {
            parent: task.into(),
            task_id: task_id()?,
        },
        other => bail!("Kodem has no tool named {other}"),
    };
    match ipc::request(root, &request)? {
        Response::Agents(agents) => Ok(agents
            .iter()
            .map(|agent| {
                format!(
                    "- provider {}, model {} ({}){}",
                    provider_name(agent.provider),
                    agent.model,
                    agent.name,
                    if agent.default {
                        ", the provider's default"
                    } else {
                        ""
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("\n")),
        Response::Delegated(delegation) => Ok(report(&delegation)),
        _ => bail!("Kodem gave an answer that this tool does not understand"),
    }
}

// What the agent reads about a delegated task.
fn report(delegation: &Delegation) -> String {
    let mut text = format!(
        "Task {} on {}: {}.",
        delegation.task_id,
        delegation.agent,
        delegation.state.label()
    );
    if let Some((branch, directory)) = &delegation.worktree {
        text.push_str(&format!(
            "\nIt works on branch {branch} in {}. Its changes are there, not in your checkout.",
            directory.display()
        ));
    }
    if let Some(error) = &delegation.error {
        text.push_str(&format!("\nError: {error}"));
    }
    if delegation.done {
        match &delegation.result {
            Some(result) => text.push_str(&format!("\n\nResult:\n{result}")),
            None => text.push_str("\nIt ended without a reply."),
        }
    } else if delegation.needs_input {
        text.push_str("\nIt waits for an approval or an answer of the user. Tell the user to open this task in Kodem. Then call task_status with this task_id.");
    } else {
        text.push_str(
            "\nIt still works. Call task_status with this task_id to wait for the result.",
        );
    }
    text
}
