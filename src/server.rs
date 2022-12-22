#[macro_use]
extern crate lazy_static;

mod common;
mod actions {
    tonic::include_proto!("actions");
}

use actions::{
    controller_server::{Controller, ControllerServer},
    AuthAction, AuthRequest, AuthResponce, BackupRequest, CommandRequest, DownloadRequest,
    OpResponce, OpResult, StartRequest, StopRequest, WorldDownload,
};
use antidote::RwLock;
use futures::Stream;
use magic_crypt::{new_magic_crypt, MagicCrypt256, MagicCryptTrait};
use rand::prelude::*;
use std::{
    collections::HashSet,
    fs::File,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    pin::Pin,
    process::{Child, Command, Stdio},
    time::SystemTime,
};
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use tonic::{transport::Server, Request, Response, Status};
use ServerState::*;

///////////////////////////////////////////////////////////////////////////////////////////////////////////////
// Server setup
///////////////////////////////////////////////////////////////////////////////////////////////////////////////

/// Create a server that will allow users to start, stop a minecraft server as well as download the world file
// #[tokio::main]
#[tokio::main(flavor = "current_thread")] // no need to use many threads as traffic will be very low
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    {
        // Change working dir that of .minecraft
        // This is required for java to load the minecraft sever properly
        let current_working_directory =
            std::env::current_dir().expect("Couldn't load current working directory");
        let minecraft_directory = std::path::Path::new(&CONFIG.minecraft_directory);
        let mut working_directory = current_working_directory.to_path_buf();
        working_directory.push(minecraft_directory);
        std::env::set_current_dir(&working_directory)
            .expect(format!("Unable to set working-dir to {:?}", working_directory).as_ref());
    }
    //TODO change to real socket once on the actual server
    let socket = CONFIG.socket.parse()?;
    let server_loader = ControllerService::default();
    println!("Starting service");
    Server::builder()
        .add_service(ControllerServer::new(server_loader))
        .serve(socket)
        .await?;
    Ok(())
}

/// Used to set up our server, we will impl all methods outlined in proto/actions.proto on this struct
#[derive(Debug, Default)]
struct ControllerService {}

/// Shorthand for Ok(Responce::new(OpResponce{result: code, comment: comment}))
fn respond(code: OpResult, comment: &str) -> Result<Response<OpResponce>, Status> {
    println!("Replying with: {}", comment);
    Ok(Response::new(OpResponce {
        result: code.into(),
        comment: comment.to_owned(),
    }))
}

#[tonic::async_trait]
impl Controller for ControllerService {
    async fn auth(&self, req: Request<AuthRequest>) -> Result<Response<AuthResponce>, Status> {
        let action = match AuthAction::from_i32(req.into_inner().action) {
            Some(action) => action,
            None => {
                return Ok(Response::new(AuthResponce {
                    result: OpResult::Fail as i32,
                    key: Vec::new(),
                    comment: "Invalid action".to_string(),
                }))
            }
        };
        let key = authorize_key(action);
        let encrypted_key = encrypt(key);
        let result = OpResult::Success.into();
        Ok(Response::new(AuthResponce {
            result,
            key: encrypted_key,
            comment: "Success".to_string(),
        }))
    }

    async fn backup(&self, req: Request<BackupRequest>) -> Result<Response<OpResponce>, Status> {
        let key = req.into_inner().token;
        if !verify_key(Key {
            key,
            action: AuthAction::Backup,
        }) {
            return Err(Status::new(tonic::Code::InvalidArgument, "Invalid token"));
        }
        let mut state = STATE.write();
        let result = state.backup();
        match result {
            Ok(_) => respond(OpResult::Success, "Backed up successfully"),
            Err(download_error) => match download_error {
                BackupError::OtherBackup => respond(OpResult::Fail, "backed up successfully"),
                BackupError::ServerRunning => {
                    respond(OpResult::Fail, "Back up failed, server still running")
                }
                BackupError::Compression => respond(
                    OpResult::Fail,
                    "Back up failed to compress the world folder",
                ),
            },
        }
    }

    async fn command(&self, req: Request<CommandRequest>) -> Result<Response<OpResponce>, Status> {
        let req = req.into_inner();
        let key = req.token;
        if !verify_key(Key {
            key,
            action: AuthAction::Command,
        }) {
            return respond(OpResult::Denied, "Invalid token");
        }
        let mut state = STATE.write();
        let res = state.run_command(&req.command);
        match res {
            Err(command_error) => {
                match command_error{
                    CommandError::Idle => {
                        respond(OpResult::Fail, "Server idle, command can't be run")
                    },
                    CommandError::Downloading=> {
                        respond(OpResult::Fail, "Download in progress! Command can't be run")
                    },
                    CommandError::ProccesError => {
                        respond(OpResult::Fail, "Error running command on procces")
                    },
                }
            }
            Ok(_) => {
                respond(OpResult::Success, "Command ran successfully! note this does not necessarily mean the command was valid only that it's execution was attempted")
            }
        }
    }

    /// Request to download the world-file
    type DownloadStream = WDLStream;
    async fn download(
        &self,
        req: Request<DownloadRequest>,
    ) -> Result<Response<Self::DownloadStream>, Status> {
        let key = req.into_inner().token;
        if !verify_key(Key {
            key,
            action: AuthAction::Download,
        }) {
            return Err(Status::new(tonic::Code::InvalidArgument, "Invalid token"));
        }

        let file = match latest_file(&CONFIG.backup_directory) {
            Some(path) => match File::open(path) {
                Ok(handle) => handle,
                Err(_) => return Err(Status::not_found("No backups")),
            },
            None => return Err(Status::not_found("No backups")),
        };

        // Create iterator that yields WorldDownload
        let wdl = match WDLIter::new(file) {
            Some(dl) => dl,
            None => return Err(Status::aborted("Unable to fetch file metadata")),
        };

        let mut stream = Box::pin(tokio_stream::iter(wdl));

        let (send_channel, receive_channel) = mpsc::channel(128);
        tokio::spawn(async move {
            while let Some(item) = stream.next().await {
                match send_channel.send(Result::<_, Status>::Ok(item)).await {
                    Ok(_) => {
                        // item (server response) was queued to be send to client
                    }
                    Err(_item) => {
                        // output_stream was build from receive_channel and both are dropped
                        break;
                    }
                }
            }
            println!("\tclient disconnected");
        });

        let output_stream = ReceiverStream::new(receive_channel);
        Ok(Response::new(
            Box::pin(output_stream) as Self::DownloadStream
        ))
    }

    ///Handle startup request
    async fn start(&self, req: Request<StartRequest>) -> Result<Response<OpResponce>, Status> {
        let key = req.into_inner().token;
        if !verify_key(Key {
            key,
            action: AuthAction::Start,
        }) {
            return respond(OpResult::Denied, "Invalid Token");
        }

        let mut state = STATE.write();
        let res = state.start();
        match res {
            Ok(_) => respond(OpResult::Success, "Started successfully"),
            Err(start_error) => match start_error {
                StartError::Launch => respond(OpResult::Fail, "Failed to launch server"),
                StartError::AlreadyRunning => respond(OpResult::Fail, "Server already running"),
                StartError::Downloading => {
                    respond(OpResult::Fail, "Download in progress! Can't start")
                }
            },
        }
    }

    /// Handle stopping
    async fn stop(&self, req: Request<StopRequest>) -> Result<Response<OpResponce>, Status> {
        let key = req.into_inner().token;
        use AuthAction;
        if !verify_key(Key {
            key,
            action: AuthAction::Stop,
        }) {
            return respond(OpResult::Denied, "Invalid token");
        }
        let mut state = STATE.write();
        let res = state.stop();
        match res {
            Err(stop_error) => match stop_error {
                StopError::ProccesError => respond(
                    OpResult::Fail,
                    "Error occurred while stopping server procces",
                ),
                _ => respond(OpResult::Fail, "Server already idle"),
            },
            Ok(_) => {
                return respond(OpResult::Success, "Server stopped successfully");
            }
        }
    }
}

///////////////////////////////////////////////////////////////////////////////////////////////////////////////
// Server state management using the state machine pattern
///////////////////////////////////////////////////////////////////////////////////////////////////////////////

#[derive(Debug)]
enum ServerState {
    Idle,
    Running { procces: Child },
    BackingUp,
}

impl ServerState {
    fn backup(&mut self) -> Result<(), BackupError> {
        use ServerState::*;
        match self {
            Idle => {
                *self = BackingUp;
                // Compress the file
                match Command::new("tar")
                    .arg("-czf")
                    .arg(format!(
                        "{}/{}.tar.gz",
                        &CONFIG.backup_directory,
                        common::ran_letters(32)
                    ))
                    .arg("world")
                    .spawn()
                {
                    Ok(mut child) => {
                        if let Err(_) = child.wait() {
                            return Err(BackupError::Compression);
                        }
                    }
                    Err(_) => {
                        return Err(BackupError::Compression);
                    }
                }
                // remove_oldest_backup("minecraft/backups");
                {
                    let mut num_backups = std::fs::read_dir(&CONFIG.backup_directory)
                        .into_iter()
                        .flatten()
                        .count();
                    while num_backups > 10 {
                        num_backups -= 1;
                        remove_oldest_backup(&CONFIG.backup_directory);
                    }
                }
                *self = Idle;
                Ok(())
            }
            Running { procces: _ } => Err(BackupError::ServerRunning),
            BackingUp => Err(BackupError::OtherBackup),
        }
    }

    fn check_stop(&mut self) {
        if let Running { procces: c } = self {
            let res = c.try_wait();
            if let Ok(possible_exit_code) = res {
                if let Some(_exit_code) = possible_exit_code {
                    //Procces finished
                    *self = Idle;
                }
            }
        }
    }

    fn run_command(&mut self, cmd: &str) -> Result<(), CommandError> {
        match self {
            Running { procces } => {
                let pstdin = procces.stdin.as_mut();
                match pstdin {
                    Some(buff) => match buff.write_all(&format!("\n{}\n", cmd).into_bytes()) {
                        Err(_) => Err(CommandError::ProccesError),
                        _ => Ok(()),
                    },
                    None => Err(CommandError::ProccesError),
                }
            }
            Idle => Err(CommandError::Idle),
            BackingUp => Err(CommandError::Downloading),
        }
    }

    /// Spawn a new java procces and store it in MINECRAFT_SERVER_STATE
    fn start(&mut self) -> Result<(), StartError> {
        self.check_stop();
        match self {
            Idle => {
                let child = match Command::new("sh")
                    .stdin(Stdio::piped())
                    // .stdout(Stdio::piped())
                    .arg("launch.sh")
                    .spawn()
                {
                    Ok(child) => child,
                    Err(_c) => return Err(StartError::Launch),
                };
                *self = Running { procces: child };
                Ok(())
            }
            BackingUp => Err(StartError::Downloading),
            Running { procces: _ } => Err(StartError::AlreadyRunning),
        }
    }

    /// Stop the running procces by entering stop into the stdin
    fn stop(&mut self) -> Result<(), StopError> {
        self.check_stop();
        match self {
            Running { procces: child } => {
                let child_input = child.stdin.as_mut();
                match child_input {
                    Some(buff) => {
                        match buff.write_all(b"\nstop\n") {
                            Err(_) => return Err(StopError::ProccesError),
                            _ => {}
                        };
                        match child.wait() {
                            Ok(_) => {}
                            Err(_) => {}
                        }
                        *self = Idle;
                        Ok(())
                    }
                    None => Err(StopError::ProccesError),
                }
            }
            BackingUp => Err(StopError::Downloading),
            Idle => Err(StopError::Idle),
        }
    }
}

#[derive(Debug)]
enum StopError {
    Idle,
    Downloading,
    ProccesError,
}

#[derive(Debug)]
enum StartError {
    Launch,
    AlreadyRunning,
    Downloading,
}

#[derive(Debug)]
enum BackupError {
    ServerRunning,
    OtherBackup,
    Compression,
}

#[derive(Debug)]
enum CommandError {
    Idle,
    Downloading,
    ProccesError,
}

lazy_static! {
    static ref CONFIG: crate::Config = crate::config_load();
    /// Contains the current procces of the minecraft server and it's stdin
    static ref STATE: RwLock<ServerState> = RwLock::new(Idle);
}

///////////////////////////////////////////////////////////////////////////////////////////////////////////////
// Security
///////////////////////////////////////////////////////////////////////////////////////////////////////////////

lazy_static! {
    static ref CRYPT: MagicCrypt256 = new_magic_crypt!(CONFIG.key.clone(), 256);
    static ref SOCKET: String = CONFIG.socket.clone();
    static ref KEY: String = CONFIG.key.clone();
    static ref KEYS: RwLock<HashSet<Key>> = RwLock::new(HashSet::new());
}
const KEY_BYTES: usize = 256;

#[derive(Eq, Clone, Hash, PartialEq, Debug)]
struct Key {
    action: AuthAction,
    key: Vec<u8>,
}

fn encrypt(data: Vec<u8>) -> Vec<u8> {
    CRYPT.encrypt_bytes_to_bytes(&data)
}

/// Generate some some random bytes for authentication
fn gen_bytes(key_bytes: usize) -> Vec<u8> {
    let mut bytes: Vec<u8> = vec![0;key_bytes];
    thread_rng().fill_bytes(&mut bytes[..]);
    bytes
}

/// Create a new key to give to our client, and store it so it can be verified later
fn authorize_key(action: AuthAction) -> Vec<u8> {
    // Keys must be initialised before use
    let mut set = KEYS.write();
    let bytes = gen_bytes(KEY_BYTES);
    set.insert(Key {
        key: bytes.clone(),
        action,
    });
    bytes
}
/// Check that a key has been authored by us
fn verify_key(key: Key) -> bool {
    let mut set = KEYS.write();
    let res = set.remove(&key);
    res
}

///////////////////////////////////////////////////////////////////////////////////////////////////////////////
// Backup stuff
///////////////////////////////////////////////////////////////////////////////////////////////////////////////

fn latest_file(dir: &str) -> Option<PathBuf> {
    let files = match std::fs::read_dir(dir) {
        Ok(files) => files,
        Err(_) => return None,
    }
    .into_iter();

    match iter_paths_with_sys_time(files).max_by_key(|t| t.1) {
        Some(tuple) => Some(tuple.0),
        None => None,
    }
}

fn iter_paths_with_sys_time(
    files: std::fs::ReadDir,
) -> impl Iterator<Item = (PathBuf, SystemTime)> + 'static {
    files
        .flatten()
        .map(|f| f.path())
        .map(|p| {
            let time = match std::fs::metadata(&p) {
                Ok(metadata) => match metadata.modified() {
                    Ok(time) => time,
                    Err(_) => return None,
                },
                Err(_) => return None,
            };
            Some((p, time))
        })
        .flatten()
}

fn remove_oldest_backup(dir: &str) {
    let files = match std::fs::read_dir(dir) {
        Ok(files) => files,
        Err(_) => return,
    }
    .into_iter();

    match iter_paths_with_sys_time(files).min_by_key(|t| t.1) {
        Some(tuple) => {
            let _ = std::fs::remove_file(tuple.0);
        }
        _ => {}
    };
}

///////////////////////////////////////////////////////////////////////////////////////////////////////////////
// World Download types
///////////////////////////////////////////////////////////////////////////////////////////////////////////////

/// Produces an iterator of WorldDownload for streaming to the client
struct WDLIter {
    file_reader: BufReader<File>,
    error: bool,
    read: usize,
    size: usize,
}

impl WDLIter {
    fn new(file: File) -> Option<Self> {
        Some(Self {
            size: match file.metadata() {
                Ok(data) => data,
                Err(_) => return None,
            }
            .len() as usize,
            file_reader: BufReader::with_capacity(1024 * 1024, file),
            read: 0,
            error: false,
        })
    }
}

// bruh
type WDLStream = Pin<Box<dyn Stream<Item = Result<WorldDownload, Status>> + Send>>;

impl Iterator for WDLIter {
    type Item = WorldDownload;

    fn next(&mut self) -> Option<Self::Item> {
        if self.error {
            return None;
        }
        let bytes: Vec<u8> = match self.file_reader.fill_buf() {
            Ok(buff) => buff.to_vec(),
            Err(_) => {
                self.error = true;
                return Some(WorldDownload {
                    result: OpResult::Fail.into(),
                    size: self.size as u64,
                    comment: format!("Download failed"),
                    data: vec![],
                });
            }
        };
        self.file_reader.consume(bytes.len());
        self.read += bytes.len();
        let progress = (self.read as f64 / self.size as f64 * 100.) as u64;
        if !bytes.is_empty() {
            Some(WorldDownload {
                result: OpResult::Success.into(),
                size: self.size as u64,
                comment: format!("Download progress: {progress}%"),
                data: bytes,
            })
        } else {
            None
        }
    }
}

///////////////////////////////////////////////////////////////////////////////////////////////////////////////
// Config
///////////////////////////////////////////////////////////////////////////////////////////////////////////////

/// Contains config info
#[derive(serde_derive::Deserialize, Debug)]
struct Config {
    /// Working directory of minecraft server
    minecraft_directory: String,
    /// Where to store backups relative to minecraft dir
    backup_directory: String,
    /// Clients need to have this to authenticate their actions
    key: String,
    /// Service runs from this socket
    socket: String,
}

/// Load the config file and parse it into a convenient data structure
///
/// Panics if the config file couldn't be loaded or parsed
///
fn config_load() -> Config {
    let conf = std::fs::read("mcsc_server.toml").expect("Unable to load config file");
    toml::from_slice(&conf).expect("Unable to parse config, (syntax error)")
}
