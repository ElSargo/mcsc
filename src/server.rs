use mcsc::actions::{
    controller_server::{Controller, ControllerServer},
    AuthAction, AuthToken, BackupRequest, CommandRequest, DownloadRequest, LaunchRequest,
    OpResponse, OpResult, StopRequest, WorldDownload,
};
use mcsc::common;
use serde::Deserialize;
use std::collections::{HashSet, VecDeque};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::{Child, Command, Stdio};
use std::sync::{LazyLock, Mutex};
use std::time::SystemTime;
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, Stream, StreamExt};
use tonic::{transport::Server, Request, Response, Status};

const MAX_SEEN_NONCES: usize = 4096;

static CONFIG: LazyLock<Config> = LazyLock::new(|| {
    let config = Config::load().expect("unable to load mcsc_server.toml");
    config.validate().expect("invalid mcsc_server.toml");
    config
});
static STATE: LazyLock<Mutex<ServerState>> = LazyLock::new(|| Mutex::new(ServerState::Idle));
static SEEN_NONCES: LazyLock<Mutex<NonceCache>> =
    LazyLock::new(|| Mutex::new(NonceCache::new(MAX_SEEN_NONCES)));

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let current_working_directory = std::env::current_dir()?;
    let working_directory = current_working_directory.join(&CONFIG.minecraft_directory);
    std::env::set_current_dir(&working_directory)?;
    std::fs::create_dir_all(&CONFIG.backup_directory)?;

    let socket = CONFIG.socket.parse()?;
    let service = ControllerService::default();

    println!("Starting mcsc service on {socket}");
    Server::builder()
        .add_service(ControllerServer::new(service))
        .serve(socket)
        .await?;

    Ok(())
}

#[derive(Debug, Default)]
struct ControllerService {}

fn respond(code: OpResult, comment: &str) -> Result<Response<OpResponse>, Status> {
    println!("Replying with: {comment}");
    Ok(Response::new(OpResponse {
        result: code.into(),
        comment: comment.to_owned(),
    }))
}

#[tonic::async_trait]
impl Controller for ControllerService {
    async fn backup(&self, req: Request<BackupRequest>) -> Result<Response<OpResponse>, Status> {
        if !verify_auth(AuthAction::Backup, req.into_inner().auth, &[]) {
            return respond(OpResult::Denied, "Invalid auth token");
        }

        let mut state = STATE.lock().expect("server state lock poisoned");
        match state.backup() {
            Ok(_) => respond(OpResult::Success, "Backed up successfully"),
            Err(BackupError::OtherBackup) => respond(OpResult::Fail, "Backup already in progress"),
            Err(BackupError::ServerRunning) => respond(
                OpResult::Fail,
                "Backup failed because the server is running",
            ),
            Err(BackupError::Compression) => respond(
                OpResult::Fail,
                "Backup failed while compressing the world folder",
            ),
            Err(BackupError::Random) => respond(
                OpResult::Fail,
                "Backup failed while creating a random file name",
            ),
        }
    }

    async fn command(&self, req: Request<CommandRequest>) -> Result<Response<OpResponse>, Status> {
        let req = req.into_inner();
        if !verify_auth(AuthAction::Command, req.auth, req.command.as_bytes()) {
            return respond(OpResult::Denied, "Invalid auth token");
        }

        let mut state = STATE.lock().expect("server state lock poisoned");
        match state.run_command(&req.command) {
            Ok(_) => respond(OpResult::Success, "Command was sent to the server console"),
            Err(CommandError::Idle) => respond(OpResult::Fail, "Server is idle"),
            Err(CommandError::BackingUp) => {
                respond(OpResult::Fail, "Backup in progress; command was not run")
            }
            Err(CommandError::ProcessError) => {
                respond(OpResult::Fail, "Error while writing to the server process")
            }
            Err(CommandError::Empty) => respond(OpResult::Fail, "Command cannot be empty"),
            Err(CommandError::ControlCharacters) => {
                respond(OpResult::Denied, "Command contains control characters")
            }
            Err(CommandError::NotAllowed(command)) => respond(
                OpResult::Denied,
                &format!("Command '{command}' is not allowed by server config"),
            ),
        }
    }

    type DownloadStream = WdlStream;

    async fn download(
        &self,
        req: Request<DownloadRequest>,
    ) -> Result<Response<Self::DownloadStream>, Status> {
        if !verify_auth(AuthAction::Download, req.into_inner().auth, &[]) {
            return Err(Status::permission_denied("Invalid auth token"));
        }

        let file = match latest_file(Path::new(&CONFIG.backup_directory)) {
            Some(path) => File::open(path).map_err(|_| Status::not_found("No backups"))?,
            None => return Err(Status::not_found("No backups")),
        };

        let wdl = WorldDownloadIterator::new(file)
            .ok_or_else(|| Status::aborted("Unable to fetch file metadata"))?;
        let mut stream = Box::pin(tokio_stream::iter(wdl));

        let (send_channel, receive_channel) = mpsc::channel(128);
        tokio::spawn(async move {
            while let Some(item) = stream.next().await {
                if send_channel
                    .send(Result::<_, Status>::Ok(item))
                    .await
                    .is_err()
                {
                    break;
                }
            }
            println!("client disconnected from download stream");
        });

        Ok(Response::new(
            Box::pin(ReceiverStream::new(receive_channel)) as Self::DownloadStream,
        ))
    }

    async fn launch(&self, req: Request<LaunchRequest>) -> Result<Response<OpResponse>, Status> {
        if !verify_auth(AuthAction::Launch, req.into_inner().auth, &[]) {
            return respond(OpResult::Denied, "Invalid auth token");
        }

        let mut state = STATE.lock().expect("server state lock poisoned");
        match state.launch() {
            Ok(_) => respond(OpResult::Success, "Launched successfully"),
            Err(LaunchError::Launch) => respond(OpResult::Fail, "Failed to launch server"),
            Err(LaunchError::AlreadyRunning) => respond(OpResult::Fail, "Server already running"),
            Err(LaunchError::BackingUp) => {
                respond(OpResult::Fail, "Backup in progress; cannot launch")
            }
        }
    }

    async fn stop(&self, req: Request<StopRequest>) -> Result<Response<OpResponse>, Status> {
        if !verify_auth(AuthAction::Stop, req.into_inner().auth, &[]) {
            return respond(OpResult::Denied, "Invalid auth token");
        }

        let mut state = STATE.lock().expect("server state lock poisoned");
        match state.stop() {
            Ok(_) => respond(OpResult::Success, "Server stopped successfully"),
            Err(StopError::Idle) => respond(OpResult::Fail, "Server is already idle"),
            Err(StopError::BackingUp) => respond(OpResult::Fail, "Backup in progress"),
            Err(StopError::ProcessError) => respond(
                OpResult::Fail,
                "Error occurred while stopping server process",
            ),
        }
    }
}

#[derive(Debug)]
enum ServerState {
    Idle,
    Running { process: Child },
    BackingUp,
}

impl ServerState {
    fn backup(&mut self) -> Result<(), BackupError> {
        match self {
            ServerState::Idle => {
                *self = ServerState::BackingUp;
                let result = create_backup();
                *self = ServerState::Idle;
                result
            }
            ServerState::Running { .. } => Err(BackupError::ServerRunning),
            ServerState::BackingUp => Err(BackupError::OtherBackup),
        }
    }

    fn check_stop(&mut self) {
        if let ServerState::Running { process } = self {
            if matches!(process.try_wait(), Ok(Some(_))) {
                *self = ServerState::Idle;
            }
        }
    }

    fn run_command(&mut self, command: &str) -> Result<(), CommandError> {
        validate_command(command)?;

        match self {
            ServerState::Running { process } => {
                let Some(stdin) = process.stdin.as_mut() else {
                    return Err(CommandError::ProcessError);
                };

                stdin
                    .write_all(format!("{command}\n").as_bytes())
                    .map_err(|_| CommandError::ProcessError)
            }
            ServerState::Idle => Err(CommandError::Idle),
            ServerState::BackingUp => Err(CommandError::BackingUp),
        }
    }

    fn launch(&mut self) -> Result<(), LaunchError> {
        self.check_stop();

        match self {
            ServerState::Idle => {
                let child = launch_command()
                    .stdin(Stdio::piped())
                    .spawn()
                    .map_err(|_| LaunchError::Launch)?;

                *self = ServerState::Running { process: child };
                Ok(())
            }
            ServerState::BackingUp => Err(LaunchError::BackingUp),
            ServerState::Running { .. } => Err(LaunchError::AlreadyRunning),
        }
    }

    fn stop(&mut self) -> Result<(), StopError> {
        self.check_stop();

        match self {
            ServerState::Running { process } => {
                let Some(stdin) = process.stdin.as_mut() else {
                    return Err(StopError::ProcessError);
                };

                stdin
                    .write_all(b"stop\n")
                    .map_err(|_| StopError::ProcessError)?;
                process.wait().map_err(|_| StopError::ProcessError)?;
                *self = ServerState::Idle;
                Ok(())
            }
            ServerState::BackingUp => Err(StopError::BackingUp),
            ServerState::Idle => Err(StopError::Idle),
        }
    }
}

#[derive(Debug)]
enum StopError {
    Idle,
    BackingUp,
    ProcessError,
}

#[derive(Debug)]
enum LaunchError {
    Launch,
    AlreadyRunning,
    BackingUp,
}

#[derive(Debug)]
enum BackupError {
    ServerRunning,
    OtherBackup,
    Compression,
    Random,
}

#[derive(Debug)]
enum CommandError {
    Idle,
    BackingUp,
    ProcessError,
    Empty,
    ControlCharacters,
    NotAllowed(String),
}

fn verify_auth(action: AuthAction, token: Option<AuthToken>, request_body: &[u8]) -> bool {
    let Some(token) = token else {
        return false;
    };

    if token.nonce.len() != 32 || token.signature.len() != 32 {
        return false;
    }

    if !common::verify_signed_token(&CONFIG.key, action, &token, request_body) {
        return false;
    }

    SEEN_NONCES
        .lock()
        .expect("nonce cache lock poisoned")
        .insert(token.nonce)
}

fn validate_command(command: &str) -> Result<(), CommandError> {
    let command = command.trim();
    if command.is_empty() {
        return Err(CommandError::Empty);
    }

    if command.chars().any(|c| c.is_control()) {
        return Err(CommandError::ControlCharacters);
    }

    let command_name = command
        .split_whitespace()
        .next()
        .ok_or(CommandError::Empty)?
        .to_ascii_lowercase();

    if CONFIG
        .allowed_commands
        .iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(&command_name))
    {
        Ok(())
    } else {
        Err(CommandError::NotAllowed(command_name))
    }
}

fn create_backup() -> Result<(), BackupError> {
    let file_name = format!(
        "{}.tar.gz",
        common::random_letters(32).map_err(|_| BackupError::Random)?
    );
    let backup_path = Path::new(&CONFIG.backup_directory).join(file_name);

    let status = Command::new("tar")
        .arg("-czf")
        .arg(&backup_path)
        .arg("world")
        .status()
        .map_err(|_| BackupError::Compression)?;

    if !status.success() {
        let _ = std::fs::remove_file(&backup_path);
        return Err(BackupError::Compression);
    }

    prune_backups(Path::new(&CONFIG.backup_directory), CONFIG.max_backups);
    Ok(())
}

fn launch_command() -> Command {
    let mut command = Command::new(&CONFIG.launch_command[0]);
    command.args(&CONFIG.launch_command[1..]);
    command
}

fn latest_file(dir: &Path) -> Option<PathBuf> {
    backup_entries(dir)
        .max_by_key(|(_, modified)| *modified)
        .map(|(path, _)| path)
}

fn prune_backups(dir: &Path, max_backups: usize) {
    let mut entries: Vec<_> = backup_entries(dir).collect();
    entries.sort_by_key(|(_, modified)| *modified);

    while entries.len() > max_backups {
        if let Some((path, _)) = entries.first() {
            let _ = std::fs::remove_file(path);
        }
        entries.remove(0);
    }
}

fn backup_entries(dir: &Path) -> impl Iterator<Item = (PathBuf, SystemTime)> {
    let entries = std::fs::read_dir(dir).into_iter().flatten();

    entries.filter_map(|entry| {
        let entry = entry.ok()?;
        let file_type = entry.file_type().ok()?;
        if !file_type.is_file() {
            return None;
        }

        let path = entry.path();
        let file_name = path.file_name()?.to_str()?;
        if !file_name.ends_with(".tar.gz") {
            return None;
        }

        let modified = entry.metadata().ok()?.modified().ok()?;
        Some((path, modified))
    })
}

struct WorldDownloadIterator {
    file_reader: BufReader<File>,
    error: bool,
    read: usize,
    size: usize,
}

impl WorldDownloadIterator {
    fn new(file: File) -> Option<Self> {
        let size = file.metadata().ok()?.len() as usize;

        Some(Self {
            size,
            file_reader: BufReader::with_capacity(1024 * 1024, file),
            read: 0,
            error: false,
        })
    }
}

type WdlStream = Pin<Box<dyn Stream<Item = Result<WorldDownload, Status>> + Send>>;

impl Iterator for WorldDownloadIterator {
    type Item = WorldDownload;

    fn next(&mut self) -> Option<Self::Item> {
        if self.error {
            return None;
        }

        let bytes = match self.file_reader.fill_buf() {
            Ok(buffer) => buffer.to_vec(),
            Err(_) => {
                self.error = true;
                return Some(WorldDownload {
                    result: OpResult::Fail.into(),
                    size: self.size as u64,
                    comment: "Download failed".to_owned(),
                    data: Vec::new(),
                });
            }
        };

        self.file_reader.consume(bytes.len());
        self.read += bytes.len();

        if bytes.is_empty() {
            return None;
        }

        let progress = if self.size == 0 {
            100
        } else {
            (self.read as f64 / self.size as f64 * 100.) as u64
        };

        Some(WorldDownload {
            result: OpResult::Success.into(),
            size: self.size as u64,
            comment: format!("Download progress: {progress}%"),
            data: bytes,
        })
    }
}

#[derive(Debug, Deserialize)]
struct Config {
    minecraft_directory: PathBuf,
    backup_directory: PathBuf,
    key: String,
    socket: String,
    #[serde(default = "default_launch_command")]
    launch_command: Vec<String>,
    #[serde(default = "default_allowed_commands")]
    allowed_commands: Vec<String>,
    #[serde(default = "default_max_backups")]
    max_backups: usize,
}

impl Config {
    fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let bytes = std::fs::read("mcsc_server.toml")?;
        let text = std::str::from_utf8(&bytes)?;
        Ok(toml::from_str(text)?)
    }

    fn validate(&self) -> Result<(), String> {
        if self.key.len() < 32 {
            return Err("key must be at least 32 characters".to_owned());
        }

        let lower_key = self.key.to_ascii_lowercase();
        if lower_key.contains("change-this")
            || lower_key.contains("replace-with")
            || lower_key.contains("who was in paris")
        {
            return Err("key must be changed from the example value".to_owned());
        }

        if self.launch_command.is_empty() {
            return Err("launch_command must contain at least one item".to_owned());
        }

        if self.max_backups == 0 {
            return Err("max_backups must be greater than zero".to_owned());
        }

        Ok(())
    }
}

fn default_launch_command() -> Vec<String> {
    vec!["sh".to_owned(), "launch.sh".to_owned()]
}

fn default_allowed_commands() -> Vec<String> {
    vec![
        "list".to_owned(),
        "say".to_owned(),
        "save-all".to_owned(),
        "whitelist".to_owned(),
    ]
}

fn default_max_backups() -> usize {
    10
}

#[derive(Debug)]
struct NonceCache {
    capacity: usize,
    order: VecDeque<Vec<u8>>,
    seen: HashSet<Vec<u8>>,
}

impl NonceCache {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            order: VecDeque::with_capacity(capacity),
            seen: HashSet::with_capacity(capacity),
        }
    }

    fn insert(&mut self, nonce: Vec<u8>) -> bool {
        if self.seen.contains(&nonce) {
            return false;
        }

        while self.order.len() >= self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.seen.remove(&oldest);
            }
        }

        self.seen.insert(nonce.clone());
        self.order.push_back(nonce);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonce_cache_rejects_replay() {
        let mut cache = NonceCache::new(2);
        assert!(cache.insert(vec![1]));
        assert!(!cache.insert(vec![1]));
    }

    #[test]
    fn nonce_cache_evicts_oldest_nonce() {
        let mut cache = NonceCache::new(2);
        assert!(cache.insert(vec![1]));
        assert!(cache.insert(vec![2]));
        assert!(cache.insert(vec![3]));
        assert!(cache.insert(vec![1]));
    }
}
