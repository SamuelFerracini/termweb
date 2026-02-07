use axum::{
    extract::State,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::Mutex;
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Clone)]
struct AppState {
    terminal: Arc<Mutex<TerminalState>>,
}

#[derive(Default)]
struct TerminalState {
    fs: FileSystem,
    cwd: Vec<String>,
}

#[derive(Default)]
struct FileSystem {
    root: Node,
}

#[derive(Default)]
enum Node {
    #[default]
    Dir { children: BTreeMap<String, Node> },
    File { content: String },
}

#[derive(Debug, Deserialize)]
struct CommandRequest {
    command: String,
}

#[derive(Debug, Serialize)]
struct CommandResponse {
    output: String,
    cwd: String,
    status: String,
    clear: bool,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "termweb=info".to_string()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let state = AppState {
        terminal: Arc::new(Mutex::new(TerminalState::default())),
    };

    let app = Router::new()
        .route("/api/command", post(run_command))
        .with_state(state)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .expect("bind");
    axum::serve(listener, app).await.expect("serve");
}

async fn run_command(
    State(state): State<AppState>,
    Json(payload): Json<CommandRequest>,
) -> Json<CommandResponse> {
    let mut terminal = state.terminal.lock().await;
    let response = execute_command(&mut terminal, payload.command.trim());
    Json(response)
}

fn execute_command(state: &mut TerminalState, input: &str) -> CommandResponse {
    if input.is_empty() {
        return CommandResponse {
            output: String::new(),
            cwd: state.cwd_string(),
            status: "ok".to_string(),
            clear: false,
        };
    }

    let tokens = match tokenize(input) {
        Ok(tokens) => tokens,
        Err(message) => {
            return CommandResponse {
                output: message,
                cwd: state.cwd_string(),
                status: "error".to_string(),
                clear: false,
            }
        }
    };

    if tokens.is_empty() {
        return CommandResponse {
            output: String::new(),
            cwd: state.cwd_string(),
            status: "ok".to_string(),
            clear: false,
        };
    }

    let mut output = String::new();
    let mut status = "ok".to_string();
    let mut clear = false;

    match tokens[0].as_str() {
        "help" => {
            output = [
                "Available commands:",
                "  pwd",
                "  ls [path]",
                "  cd [path]",
                "  mkdir <name>...",
                "  touch <name>...",
                "  cat <file>...",
                "  echo <text> [> file | >> file]",
                "  clear",
                "  help",
            ]
            .join("\n");
        }
        "pwd" => {
            output = state.cwd_string();
        }
        "ls" => {
            let target = tokens.get(1).map(String::as_str).unwrap_or("");
            let path = if target.is_empty() {
                state.cwd.clone()
            } else {
                resolve_path(&state.cwd, target)
            };
            match state.fs.list(&path) {
                Ok(listing) => output = listing,
                Err(message) => {
                    output = message;
                    status = "error".to_string();
                }
            }
        }
        "cd" => {
            let target = tokens.get(1).map(String::as_str).unwrap_or("/");
            let path = resolve_path(&state.cwd, target);
            match state.fs.is_dir(&path) {
                Ok(true) => state.cwd = path,
                Ok(false) => {
                    output = "Not a directory".to_string();
                    status = "error".to_string();
                }
                Err(message) => {
                    output = message;
                    status = "error".to_string();
                }
            }
        }
        "mkdir" => {
            let args = &tokens[1..];
            if args.is_empty() {
                output = "mkdir: missing operand".to_string();
                status = "error".to_string();
            } else {
                for arg in args {
                    let path = resolve_path(&state.cwd, arg);
                    if let Err(message) = state.fs.mkdir(&path) {
                        output = message;
                        status = "error".to_string();
                        break;
                    }
                }
            }
        }
        "touch" => {
            let args = &tokens[1..];
            if args.is_empty() {
                output = "touch: missing operand".to_string();
                status = "error".to_string();
            } else {
                for arg in args {
                    let path = resolve_path(&state.cwd, arg);
                    if let Err(message) = state.fs.touch(&path) {
                        output = message;
                        status = "error".to_string();
                        break;
                    }
                }
            }
        }
        "cat" => {
            let args = &tokens[1..];
            if args.is_empty() {
                output = "cat: missing operand".to_string();
                status = "error".to_string();
            } else {
                let mut parts = Vec::new();
                for arg in args {
                    let path = resolve_path(&state.cwd, arg);
                    match state.fs.read_file(&path) {
                        Ok(content) => parts.push(content),
                        Err(message) => {
                            output = message;
                            status = "error".to_string();
                            parts.clear();
                            break;
                        }
                    }
                }
                if status == "ok" {
                    output = parts.join("\n");
                }
            }
        }
        "echo" => {
            let args = &tokens[1..];
            if let Some(pos) = args.iter().position(|token| token == ">" || token == ">>") {
                if pos + 1 >= args.len() {
                    output = "echo: missing file operand".to_string();
                    status = "error".to_string();
                } else {
                    let content = args[..pos].join(" ");
                    let target = &args[pos + 1];
                    let path = resolve_path(&state.cwd, target);
                    let append = args[pos] == ">>";
                    if let Err(message) = state.fs.write_file(&path, content, append) {
                        output = message;
                        status = "error".to_string();
                    }
                }
            } else {
                output = args.join(" ");
            }
        }
        "clear" => {
            clear = true;
        }
        _ => {
            output = format!("Unknown command: {}", tokens[0]);
            status = "error".to_string();
        }
    }

    CommandResponse {
        output,
        cwd: state.cwd_string(),
        status,
        clear,
    }
}

fn tokenize(input: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;

    for ch in input.chars() {
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            } else {
                current.push(ch);
            }
            continue;
        }

        match ch {
            '\'' | '"' => {
                quote = Some(ch);
            }
            c if c.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }

    if quote.is_some() {
        return Err("Unclosed quote".to_string());
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    Ok(tokens)
}

fn resolve_path(cwd: &[String], input: &str) -> Vec<String> {
    let mut parts = if input.starts_with('/') {
        Vec::new()
    } else {
        cwd.to_vec()
    };

    for segment in input.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(segment.to_string()),
        }
    }

    parts
}

impl TerminalState {
    fn cwd_string(&self) -> String {
        if self.cwd.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", self.cwd.join("/"))
        }
    }
}

impl FileSystem {
    fn get_node<'a>(&'a self, path: &[String]) -> Option<&'a Node> {
        let mut current = &self.root;
        for segment in path {
            match current {
                Node::Dir { children } => {
                    current = children.get(segment)?;
                }
                Node::File { .. } => return None,
            }
        }
        Some(current)
    }

    fn get_node_mut<'a>(&'a mut self, path: &[String]) -> Option<&'a mut Node> {
        let mut current = &mut self.root;
        for segment in path {
            match current {
                Node::Dir { children } => {
                    current = children.get_mut(segment)?;
                }
                Node::File { .. } => return None,
            }
        }
        Some(current)
    }

    fn is_dir(&self, path: &[String]) -> Result<bool, String> {
        match self.get_node(path) {
            Some(Node::Dir { .. }) => Ok(true),
            Some(Node::File { .. }) => Ok(false),
            None => Err("Path not found".to_string()),
        }
    }

    fn list(&self, path: &[String]) -> Result<String, String> {
        match self.get_node(path) {
            Some(Node::Dir { children }) => {
                let mut entries = Vec::new();
                for (name, node) in children.iter() {
                    let suffix = if matches!(node, Node::Dir { .. }) { "/" } else { "" };
                    entries.push(format!("{}{}", name, suffix));
                }
                Ok(entries.join("  "))
            }
            Some(Node::File { .. }) => Ok(path
                .last()
                .map(|name| name.to_string())
                .unwrap_or_default()),
            None => Err("Path not found".to_string()),
        }
    }

    fn mkdir(&mut self, path: &[String]) -> Result<(), String> {
        if path.is_empty() {
            return Err("mkdir: invalid path".to_string());
        }
        let (parent, name) = split_parent(path);
        let parent_node = self
            .get_node_mut(parent)
            .ok_or_else(|| "mkdir: parent not found".to_string())?;

        match parent_node {
            Node::Dir { children } => {
                if children.contains_key(name) {
                    return Err("mkdir: already exists".to_string());
                }
                children.insert(
                    name.to_string(),
                    Node::Dir {
                        children: BTreeMap::new(),
                    },
                );
                Ok(())
            }
            Node::File { .. } => Err("mkdir: parent is not a directory".to_string()),
        }
    }

    fn touch(&mut self, path: &[String]) -> Result<(), String> {
        if path.is_empty() {
            return Err("touch: invalid path".to_string());
        }
        let (parent, name) = split_parent(path);
        let parent_node = self
            .get_node_mut(parent)
            .ok_or_else(|| "touch: parent not found".to_string())?;

        match parent_node {
            Node::Dir { children } => {
                if let Some(existing) = children.get(name) {
                    if matches!(existing, Node::Dir { .. }) {
                        return Err("touch: is a directory".to_string());
                    }
                    return Ok(());
                }
                children.insert(
                    name.to_string(),
                    Node::File {
                        content: String::new(),
                    },
                );
                Ok(())
            }
            Node::File { .. } => Err("touch: parent is not a directory".to_string()),
        }
    }

    fn read_file(&self, path: &[String]) -> Result<String, String> {
        match self.get_node(path) {
            Some(Node::File { content }) => Ok(content.clone()),
            Some(Node::Dir { .. }) => Err("cat: is a directory".to_string()),
            None => Err("cat: file not found".to_string()),
        }
    }

    fn write_file(&mut self, path: &[String], content: String, append: bool) -> Result<(), String> {
        if path.is_empty() {
            return Err("echo: invalid path".to_string());
        }
        let (parent, name) = split_parent(path);
        let parent_node = self
            .get_node_mut(parent)
            .ok_or_else(|| "echo: parent not found".to_string())?;

        match parent_node {
            Node::Dir { children } => {
                let entry = children.entry(name.to_string()).or_insert_with(|| Node::File {
                    content: String::new(),
                });
                match entry {
                    Node::File { content: file_content } => {
                        if append && !file_content.is_empty() {
                            file_content.push('\n');
                        } else if !append {
                            file_content.clear();
                        }
                        file_content.push_str(&content);
                        Ok(())
                    }
                    Node::Dir { .. } => Err("echo: target is a directory".to_string()),
                }
            }
            Node::File { .. } => Err("echo: parent is not a directory".to_string()),
        }
    }
}

fn split_parent(path: &[String]) -> (&[String], &String) {
    let len = path.len();
    (&path[..len - 1], &path[len - 1])
}
