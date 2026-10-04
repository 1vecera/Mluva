//! Isolated app-server JSONL sessions, with no permission fallback or tool dispatch.
use crate::{
    ProviderError, Result, codex_policy,
    compatible::RewriteOptions,
    models::{Model, select_codex_model, truthy},
};
use mluva_core::{
    executables::find_executable,
    screenshots::{image_context, validate_images},
};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::{Notify, mpsc, oneshot};
use tokio_util::sync::CancellationToken;

pub const MODEL_PAGE_SIZE: usize = 100;
pub const MAX_MODEL_PAGES: usize = 10;
const SERVER_EXITED: &str = "_mluva/serverExited";
const UNEXPECTED_REQUEST: &str = "_mluva/unexpectedRequest";
const MALFORMED: &str = "Codex app-server returned a malformed response.";

#[derive(Clone, Debug)]
pub struct CodexOptions {
    pub command: Vec<OsString>,
    pub request_timeout: Duration,
    pub turn_timeout: Duration,
}
impl Default for CodexOptions {
    fn default() -> Self {
        Self {
            command: vec![
                "codex".into(),
                "app-server".into(),
                "--listen".into(),
                "stdio://".into(),
            ],
            request_timeout: Duration::from_secs(30),
            turn_timeout: Duration::from_secs(180),
        }
    }
}

struct Frame {
    message: Value,
    written: oneshot::Sender<bool>,
}
struct ResponseSlot {
    sender: Option<oneshot::Sender<Value>>,
}
struct Connection {
    pid: u32,
    workspace: PathBuf,
    writes: mpsc::Sender<Frame>,
    responses: Mutex<HashMap<u64, ResponseSlot>>,
    notifications: tokio::sync::Mutex<mpsc::UnboundedReceiver<Value>>,
    next_id: Arc<AtomicU64>,
    stop: CancellationToken,
    alive: AtomicBool,
    process_alive: AtomicBool,
    done: AtomicBool,
    changed: Notify,
}
struct Registration {
    connection: Arc<Connection>,
    id: u64,
}
impl Drop for Registration {
    fn drop(&mut self) {
        self.connection.responses.lock().unwrap().remove(&self.id);
    }
}
impl Connection {
    async fn send(&self, message: Value) -> bool {
        let (tx, rx) = oneshot::channel();
        let sent = tokio::select! {biased;_=self.stop.cancelled()=>return false,value=self.writes.send(Frame {message,written:tx})=>value};
        sent.is_ok()
            && tokio::select! {biased;_=self.stop.cancelled()=>false,value=rx=>value.unwrap_or(false)}
    }
    async fn request(
        self: &Arc<Self>,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.responses
            .lock()
            .unwrap()
            .insert(id, ResponseSlot { sender: Some(tx) });
        let _registration = Registration {
            connection: self.clone(),
            id,
        };
        let deadline = tokio::time::Instant::now() + timeout;
        let sent = tokio::time::timeout_at(
            deadline,
            self.send(json!({"method":method,"id":id,"params":params})),
        )
        .await;
        if sent != Ok(true) {
            return Err(ProviderError(format!(
                "Codex app-server connection failed during {method}."
            )));
        }
        let response = tokio::select! {biased;_=self.stop.cancelled()=>return Err(ProviderError(format!("Codex app-server rejected {method}."))),value=tokio::time::timeout_at(deadline,rx)=>value};
        let response = response
            .map_err(|_| ProviderError(format!("Codex app-server did not answer {method}.")))?
            .map_err(|_| ProviderError(format!("Codex app-server rejected {method}.")))?;
        if response.get("error").is_some() {
            return Err(ProviderError(format!(
                "Codex app-server rejected {method}."
            )));
        }
        response
            .get("result")
            .cloned()
            .ok_or_else(|| ProviderError::message(MALFORMED))
    }
    fn exited(&self, notifications: &mpsc::UnboundedSender<Value>) {
        self.alive.store(false, Ordering::Release);
        for slot in self.responses.lock().unwrap().values_mut() {
            if let Some(sender) = slot.sender.take() {
                let _ = sender.send(json!({"error":{}}));
            }
        }
        let _ = notifications.send(json!({"method":SERVER_EXITED,"params":{}}));
    }
    async fn shutdown(&self) {
        self.stop.cancel();
        loop {
            let ready = self.changed.notified();
            tokio::pin!(ready);
            ready.as_mut().enable();
            if self.done.load(Ordering::Acquire) {
                self.clear_notifications().await;
                return;
            }
            ready.await;
        }
    }
    async fn clear_notifications(&self) {
        let mut notifications = self.notifications.lock().await;
        while notifications.try_recv().is_ok() {}
    }
}

pub struct CodexAppServerClient {
    options: CodexOptions,
    cancelled: CancellationToken,
    next_id: Arc<AtomicU64>,
    current: Mutex<Option<Arc<Connection>>>,
    start_gate: tokio::sync::Mutex<()>,
    last_model: Mutex<Option<String>>,
}
impl CodexAppServerClient {
    pub fn new(options: CodexOptions) -> Self {
        Self {
            options,
            cancelled: CancellationToken::new(),
            next_id: Arc::new(AtomicU64::new(0)),
            current: Mutex::new(None),
            start_gate: tokio::sync::Mutex::new(()),
            last_model: Mutex::new(None),
        }
    }
    pub fn spawn(&self) -> Self {
        Self::new(self.options.clone())
    }
    pub fn process_id(&self) -> Option<u32> {
        self.current
            .lock()
            .unwrap()
            .as_ref()
            .filter(|connection| {
                connection.alive.load(Ordering::Acquire)
                    && connection.process_alive.load(Ordering::Acquire)
                    && !connection.stop.is_cancelled()
            })
            .map(|connection| connection.pid)
    }
    pub fn last_model_identifier(&self) -> Option<String> {
        self.last_model.lock().unwrap().clone()
    }

    async fn connection(&self) -> Result<Arc<Connection>> {
        let _gate = self.start_gate.lock().await;
        if self.cancelled.is_cancelled() {
            return Err(ProviderError::message(
                "Codex app-server work was cancelled.",
            ));
        }
        let current = self.current.lock().unwrap().clone();
        if let Some(connection) = current {
            if connection.alive.load(Ordering::Acquire)
                && connection.process_alive.load(Ordering::Acquire)
                && !connection.stop.is_cancelled()
            {
                return Ok(connection);
            }
            self.close().await;
        }
        let failure = || ProviderError::message("Codex app-server could not start.");
        let workspace = tempfile::Builder::new()
            .prefix("mluva-codex-")
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir()
            .map_err(|_| failure())?;
        let mut command = self.options.command.clone();
        let executable = command
            .first()
            .and_then(find_executable)
            .ok_or_else(failure)?;
        command[0] = std::path::absolute(executable)
            .map_err(|_| failure())?
            .into_os_string();
        command.push("--strict-config".into());
        for (key, value) in codex_policy::TEXT_ONLY_CONFIG.as_object().unwrap() {
            command.extend([
                "-c".into(),
                format!("{key}={}", serde_json::to_string(value).unwrap()).into(),
            ]);
        }
        let environment = codex_policy::child_environment(std::env::vars_os());
        let command = codex_policy::isolated_command(command, &environment)?;
        let mut process = Command::new(&command[0])
            .args(&command[1..])
            .env_clear()
            .envs(environment)
            .current_dir(workspace.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| failure())?;
        let stdout = process.stdout.take().unwrap();
        let stdin = process.stdin.take().unwrap();
        let (writes, writing) = mpsc::channel(32);
        let (notify, notifications) = mpsc::unbounded_channel();
        let connection = Arc::new(Connection {
            pid: process.id().unwrap(),
            workspace: workspace.path().into(),
            writes,
            responses: Mutex::new(HashMap::new()),
            notifications: tokio::sync::Mutex::new(notifications),
            next_id: self.next_id.clone(),
            stop: self.cancelled.child_token(),
            alive: AtomicBool::new(true),
            process_alive: AtomicBool::new(true),
            done: AtomicBool::new(false),
            changed: Notify::new(),
        });
        *self.current.lock().unwrap() = Some(connection.clone());
        tokio::spawn(lifecycle(
            process,
            workspace,
            stdin,
            stdout,
            writing,
            notify,
            connection.clone(),
        ));
        let initialized=connection.request("initialize",json!({"clientInfo":{"name":"mluva-linux","title":"Mluva","version":env!("CARGO_PKG_VERSION")},"capabilities":{"experimentalApi":true}}),self.options.request_timeout).await;
        if let Err(error) = initialized {
            self.close().await;
            return Err(error);
        }
        if !connection
            .send(json!({"method":"initialized","params":{}}))
            .await
        {
            self.close().await;
            return Err(ProviderError::message("Codex app-server is not running."));
        }
        Ok(connection)
    }
    pub async fn start(&self) -> Result<()> {
        self.connection().await.map(|_| ())
    }
    pub async fn close(&self) {
        let connection = self.current.lock().unwrap().take();
        if let Some(connection) = connection {
            connection.shutdown().await;
        }
    }
    pub fn cancel(&self) {
        self.cancelled.cancel();
        if let Some(connection) = self.current.lock().unwrap().as_ref() {
            connection.stop.cancel();
        }
    }
    pub async fn list_models(&self) -> Result<Vec<Model>> {
        let connection = self.connection().await?;
        let mut cursor = None;
        let mut models = vec![];
        for _ in 0..MAX_MODEL_PAGES {
            let mut params = json!({"includeHidden":true,"limit":MODEL_PAGE_SIZE});
            if let Some(cursor) = cursor.take() {
                params["cursor"] = cursor;
            }
            let page = connection
                .request("model/list", params, self.options.request_timeout)
                .await?;
            for row in page
                .get("data")
                .and_then(Value::as_array)
                .ok_or_else(|| ProviderError::message(MALFORMED))?
            {
                models.push(Model::from_codex_catalog(row)?);
            }
            cursor = page
                .get("nextCursor")
                .filter(|cursor| !cursor.is_null())
                .cloned();
            if cursor.is_none() {
                return Ok(models);
            }
        }
        Err(ProviderError::message(
            "Codex app-server returned too many model-list pages.",
        ))
    }
    pub async fn resolve_model(&self, requested: Option<&str>) -> Result<String> {
        Ok(select_codex_model(&self.list_models().await?, requested)?
            .identifier
            .clone())
    }

    pub async fn transform(
        &self,
        prompt: &str,
        _cwd: &Path,
        options: RewriteOptions<'_>,
        mut on_delta: Option<&mut (dyn FnMut(&str) + Send)>,
    ) -> Result<String> {
        validate_images(options.images).map_err(|error| ProviderError(error.to_string()))?;
        let model = match options.model.filter(|model| !model.is_empty()) {
            Some(model) => model.into(),
            None => self.resolve_model(None).await?,
        };
        let connection = self.connection().await?;
        *self.last_model.lock().unwrap() = None;
        let cwd = connection.workspace.to_string_lossy();
        let configuration = connection
            .request(
                "config/read",
                json!({"includeLayers":false,"cwd":cwd}),
                self.options.request_timeout,
            )
            .await?;
        let config = configuration
            .get("config")
            .and_then(Value::as_object)
            .ok_or_else(|| ProviderError::message(MALFORMED))?;
        let servers = match config.get("mcp_servers") {
            None => vec![],
            Some(value) => {
                let Some(servers) = value.as_object() else {
                    self.close().await;
                    return Err(ProviderError::message(
                        "Codex could not establish text-only permissions.",
                    ));
                };
                servers.keys().cloned().collect::<Vec<_>>()
            }
        };
        let mut overrides = codex_policy::TEXT_ONLY_CONFIG.clone();
        overrides["mcp_servers"] = Value::Object(
            servers
                .into_iter()
                .map(|name| (name, json!({"enabled":false})))
                .collect(),
        );
        let thread=connection.request("thread/start",json!({"cwd":cwd,"model":model,"approvalPolicy":"never","sandbox":"read-only","ephemeral":true,"serviceName":"mluva_linux","environments":[],"dynamicTools":[],"config":overrides,"developerInstructions":"","baseInstructions":"You transform dictated text. Follow the user's requested operation exactly. Return only replacement text, without commentary, quotes, or Markdown fences. Never use tools or infer facts absent from the supplied text or explicitly attached images."}),self.options.request_timeout).await?;
        if thread.get("model").and_then(Value::as_str) != Some(&model) {
            return Err(ProviderError::message(
                "Codex app-server changed the frozen model for this transformation.",
            ));
        }
        *self.last_model.lock().unwrap() = Some(model);
        let detail = thread
            .get("thread")
            .ok_or_else(|| ProviderError::message(MALFORMED))?;
        if detail.get("environments") != Some(&json!([]))
            || detail.get("ephemeral") != Some(&json!(true))
            || thread.get("instructionSources").is_some_and(truthy)
        {
            self.close().await;
            return Err(ProviderError::message(
                "Codex cannot confirm text-only isolation. Update Codex before rewriting.",
            ));
        }
        let thread_id = detail
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| ProviderError::message(MALFORMED))?;
        let inventory = connection
            .request(
                "mcpServerStatus/list",
                json!({"threadId":thread_id,"limit":100}),
                self.options.request_timeout,
            )
            .await?;
        let entries = inventory
            .get("data")
            .and_then(Value::as_array)
            .ok_or_else(|| ProviderError::message(MALFORMED))?;
        if inventory.get("nextCursor").is_some_and(truthy)
            || entries.iter().any(|entry| {
                ["tools", "resources", "resourceTemplates"]
                    .iter()
                    .any(|key| entry.get(*key).is_some_and(truthy))
            })
        {
            self.close().await;
            return Err(ProviderError::message(
                "Codex exposed external capabilities during text-only setup.",
            ));
        }
        let mut inputs = vec![json!({"type":"text","text":image_context(prompt,options.images)})];
        inputs.extend(
            options
                .images
                .iter()
                .map(|image| json!({"type":"image","url":image.data_url()})),
        );
        let mut turn = json!({"threadId":thread_id,"input":inputs});
        if let Some(effort) = options.effort {
            turn["effort"] = json!(effort);
        }
        if let Some(tier) = options.service_tier {
            turn["serviceTier"] = json!(tier);
        }
        let started = connection
            .request("turn/start", turn, self.options.request_timeout)
            .await?;
        let turn_id = started
            .get("turn")
            .and_then(|turn| turn.get("id"))
            .and_then(Value::as_str)
            .ok_or_else(|| ProviderError::message(MALFORMED))?;
        let deadline = tokio::time::Instant::now() + self.options.turn_timeout;
        let mut output = String::new();
        let mut characters = 0_usize;
        loop {
            let value = tokio::select! {biased;_=connection.stop.cancelled()=>return Err(ProviderError::message("Codex app-server exited before completing the text transformation.")),value=tokio::time::timeout_at(deadline,async {connection.notifications.lock().await.recv().await})=>value};
            let message = value
                .map_err(|_| {
                    ProviderError::message("Codex app-server timed out while producing text.")
                })?
                .ok_or_else(|| {
                    ProviderError::message(
                        "Codex app-server exited before completing the text transformation.",
                    )
                })?;
            let method = message
                .get("method")
                .and_then(Value::as_str)
                .ok_or_else(|| ProviderError::message(MALFORMED))?;
            if method == SERVER_EXITED {
                return Err(ProviderError::message(
                    "Codex app-server exited before completing the text transformation.",
                ));
            }
            if method == UNEXPECTED_REQUEST {
                self.close().await;
                return Err(ProviderError::message(
                    "Codex requested an operation outside text-only rewriting.",
                ));
            }
            let params = message
                .get("params")
                .ok_or_else(|| ProviderError::message(MALFORMED))?;
            if params.get("threadId").and_then(Value::as_str) != Some(thread_id) {
                continue;
            }
            let permitted = |item: &Value| {
                matches!(
                    item.get("type").and_then(Value::as_str),
                    Some("userMessage" | "agentMessage" | "reasoning")
                )
            };
            let item_invalid = matches!(method, "item/started" | "item/completed")
                && !params.get("item").is_some_and(permitted);
            let turn_invalid = method == "turn/completed"
                && params
                    .get("turn")
                    .and_then(|turn| turn.get("items"))
                    .is_some_and(|items| {
                        items
                            .as_array()
                            .is_none_or(|items| items.iter().any(|item| !permitted(item)))
                    });
            let method_invalid = method.starts_with("item/")
                && !matches!(method, "item/started" | "item/completed")
                && !method.starts_with("item/agentMessage/")
                && !method.starts_with("item/reasoning/");
            if item_invalid || turn_invalid || method_invalid {
                self.close().await;
                return Err(ProviderError::message(
                    "Codex attempted an operation outside text-only rewriting.",
                ));
            }
            if method == "item/agentMessage/delta"
                && params.get("turnId").and_then(Value::as_str) == Some(turn_id)
            {
                let Some(delta) = params.get("delta").and_then(Value::as_str) else {
                    self.close().await;
                    return Err(ProviderError::message(
                        "Codex returned malformed replacement text.",
                    ));
                };
                characters = characters.saturating_add(delta.chars().count());
                if characters > options.max_output_characters {
                    self.close().await;
                    return Err(ProviderError::message(
                        "Codex replacement text exceeded the supported bound.",
                    ));
                }
                output.push_str(delta);
                if let Some(callback) = &mut on_delta {
                    callback(delta);
                }
            }
            if method == "turn/completed"
                && params
                    .get("turn")
                    .and_then(|turn| turn.get("id"))
                    .and_then(Value::as_str)
                    == Some(turn_id)
            {
                let status = params["turn"]
                    .get("status")
                    .and_then(Value::as_str)
                    .ok_or_else(|| ProviderError::message(MALFORMED))?;
                if status != "completed" {
                    return Err(ProviderError(format!(
                        "Codex turn ended with status {status}."
                    )));
                }
                let text = mluva_core::text::trim(&output);
                if text.is_empty() {
                    return Err(ProviderError::message(
                        "Codex returned no replacement text.",
                    ));
                }
                if self.cancelled.is_cancelled() {
                    return Err(ProviderError::message(
                        "Codex app-server exited before completing the text transformation.",
                    ));
                }
                return Ok(text.into());
            }
        }
    }
}
impl Drop for CodexAppServerClient {
    fn drop(&mut self) {
        self.cancel();
    }
}

async fn write_messages(
    mut input: ChildStdin,
    mut frames: mpsc::Receiver<Frame>,
    connection: Arc<Connection>,
) {
    loop {
        let next =
            tokio::select! {biased;_=connection.stop.cancelled()=>break,next=frames.recv()=>next};
        let Some(frame) = next else {
            break;
        };
        let mut bytes = crate::multipart::json_compact(&frame.message);
        bytes.push(b'\n');
        let written = tokio::select! {biased;_=connection.stop.cancelled()=>false,value=async {input.write_all(&bytes).await?;input.flush().await}=>value.is_ok()};
        let _ = frame.written.send(written);
        if !written {
            break;
        }
    }
}
async fn read_messages(
    output: ChildStdout,
    notifications: mpsc::UnboundedSender<Value>,
    connection: Arc<Connection>,
) {
    let mut lines = BufReader::new(output).lines();
    loop {
        let line = tokio::select! {biased;_=connection.stop.cancelled()=>break,line=lines.next_line()=>line};
        let Ok(Some(line)) = line else {
            break;
        };
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            break;
        };
        if !message.is_object() {
            break;
        }
        if let Some(id) = message.get("id") {
            if message.get("method").is_some() {
                if !connection
                    .send(
                        json!({"id":id,"error":{"code":-32601,"message":"Unsupported operation"}}),
                    )
                    .await
                {
                    break;
                }
                let _ = notifications.send(json!({"method":UNEXPECTED_REQUEST,"params":{}}));
            } else {
                let id = match id {
                    Value::Bool(value) => Some(u64::from(*value)),
                    _ => id.as_u64(),
                };
                if let Some(id) = id {
                    let mut responses = connection.responses.lock().unwrap();
                    if let Some(slot) = responses.get_mut(&id) {
                        let Some(sender) = slot.sender.take() else {
                            break;
                        };
                        let _ = sender.send(message);
                    }
                }
            }
        } else if message.get("method").is_some() {
            let _ = notifications.send(message);
        }
    }
    connection.exited(&notifications);
}
async fn lifecycle(
    mut process: Child,
    workspace: tempfile::TempDir,
    input: ChildStdin,
    output: ChildStdout,
    frames: mpsc::Receiver<Frame>,
    notifications: mpsc::UnboundedSender<Value>,
    connection: Arc<Connection>,
) {
    let writer = tokio::spawn(write_messages(input, frames, connection.clone()));
    let mut reader = tokio::spawn(read_messages(
        output,
        notifications.clone(),
        connection.clone(),
    ));
    tokio::select! {
        biased;_=connection.stop.cancelled()=>{
            if let Some(pid)=process.id(){unsafe{libc::kill(pid as libc::pid_t,libc::SIGTERM);}}
            if tokio::time::timeout(Duration::from_secs(5),process.wait()).await.is_err(){let _=process.start_kill();let _=process.wait().await;}
        }
        _=process.wait()=>{},
    }
    connection.process_alive.store(false, Ordering::Release);
    if tokio::time::timeout(Duration::from_secs(5), &mut reader)
        .await
        .is_err()
    {
        reader.abort();
    }
    writer.abort();
    connection.exited(&notifications);
    if connection.stop.is_cancelled() {
        connection.clear_notifications().await;
    }
    drop(workspace);
    connection.done.store(true, Ordering::Release);
    connection.changed.notify_waiters();
}
