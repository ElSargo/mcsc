#![forbid(unsafe_code)]

#[macro_use]
extern crate lazy_static;
use actions::controller_server::{Controller, ControllerServer};
use std::path::PathBuf;
// use no_panic::no_panic;
use actions::{
    AuthAction, AuthRequest, AuthResponce, BackupRequest, CommandRequest, DownloadRequest,
    OpResponce, OpResult, StartRequest, StopRequest, WorldDownload,
};
use tonic::{transport::Server, Request, Response, Status};
pub mod actions {
    tonic::include_proto!("actions");
}

/// Create a server that will allow users to start, stop a minecraft server as well as download the world file
#[tokio::main(flavor = "current_thread")] // no need to use many threads as trafic will be very low
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
            .expect(format!("Unable to set workingdir to {:?}", working_directory).as_ref());
    }
    //TODO change to real socket once on the actuall server
    let socket = CONFIG.socket.parse()?;
    let server_loader = ControllerService::default();
    println!("Starting service");
    Server::builder()
        .add_service(ControllerServer::new(server_loader))
        .serve(socket)
        .await?;

    Ok(())
}

#[derive(serde_derive::Deserialize, Debug)]
struct Config {
    minecraft_directory: String,
    key: String,
    socket: String,
}

fn config_load() -> Config {
    let conf = std::fs::read("mcsc_server.toml").expect("Unable to load config file");
    toml::from_slice(&conf).expect("Unable to parse config, (syntax error)")
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
        let key = gen_key(action);
        let encypted_key = encrypt(key);
        let result = OpResult::Success.into();
        Ok(Response::new(AuthResponce {
            result,
            key: encypted_key,
            comment: "Succces".to_string(),
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
        let mut state = STATE.lock();
        let result = state.backup();
        match result {
            Ok(_) => respond(OpResult::Success, "Backed up succesfuly"),
            Err(download_error) => match download_error {
                BackupError::OtherBackup => respond(OpResult::Fail, "backed up succesfuly"),
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
        let mut state = STATE.lock();
        let res = state.run_command(&req.command);
        match res {
            Err(command_error) => {
                match command_error{
                    CommandError::Idle => {
                        respond(OpResult::Fail, "Server idle, comamnd can't be run")
                    },
                    CommandError::Downloading=> {
                        respond(OpResult::Fail, "Download in progress! Comamnd can't be run")
                    },
                    CommandError::ProccesError => {
                        respond(OpResult::Fail, "Error running command on procces")
                    },
                }
            }
            Ok(_) => {
                respond(OpResult::Success, "Command ran succesfully! note this does not nessisarly mean the command was valid only that it's execution was attempted")
            }
        }
    }

    /// Requset to download the worldfile
    async fn download(
        &self,
        req: Request<DownloadRequest>,
    ) -> Result<Response<WorldDownload>, Status> {
        let key = req.into_inner().token;
        if !verify_key(Key {
            key,
            action: AuthAction::Download,
        }) {
            return Err(Status::new(tonic::Code::InvalidArgument, "Invalid token"));
        }
        let path = match latest_file("./backups") {
            Some(path) => path,
            None => {
                return Ok(Response::new(WorldDownload {
                    result: OpResult::Fail.into(),
                    comment: "Download failed, no backups to download!".to_string(),
                    data: Vec::new(),
                }));
            }
        };
        match std::fs::read(path) {
            Ok(data) => Ok(Response::new(WorldDownload {
                result: OpResult::Success.into(),
                comment: "Download succesful".to_string(),
                data,
            })),
            Err(_) => Ok(Response::new(WorldDownload {
                result: OpResult::Fail.into(),
                comment: "Download failed, couldn't read file".to_string(),
                data: Vec::new(),
            })),
        }
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

        let mut state = STATE.lock();
        let res = state.start();
        match res {
            Ok(_) => {
                respond(OpResult::Success, "Started succesfuly")
            }
            Err(start_error) => match start_error {
                StartError::Launch => respond(OpResult::Fail, "Failed to launch server"),
                StartError::AlreadyRunning => respond(OpResult::Fail, "Server already running"),
                StartError::Downloading => {
                    respond(OpResult::Fail, "Download in proggress! Can't start")
                }
            },
        }
    }

    /// Handle stoping
    async fn stop(&self, req: Request<StopRequest>) -> Result<Response<OpResponce>, Status> {
        let key = req.into_inner().token;
        use AuthAction;
        if !verify_key(Key {
            key,
            action: AuthAction::Stop,
        }) {
            return respond(OpResult::Denied, "Invalid token");
        }
        let mut state = STATE.lock();
        let res = state.stop();
        match res {
            Err(stop_error) => match stop_error {
                StopError::ProccesError => respond(
                    OpResult::Fail,
                    "Error occured while stopping server procces",
                ),
                _ => respond(OpResult::Fail, "Server already idle"),
            },
            Ok(_) => {
                return respond(OpResult::Success, "Server stopped successfuly");
            }
        }
    }
}

#[derive(Debug)]
pub enum ServerState {
    Idle,
    Running { procces: Child },
    BackingUp,
}

#[derive(Debug)]
pub enum StopError {
    Idle,
    Downloading,
    ProccesError,
}

#[derive(Debug)]
pub enum StartError {
    Launch,
    AlreadyRunning,
    Downloading,
}

#[derive(Debug)]
pub enum BackupError {
    ServerRunning,
    OtherBackup,
    Compression,
}

#[derive(Debug)]
pub enum CommandError {
    Idle,
    Downloading,
    ProccesError,
}

impl ServerState {
    pub fn check_stop(&mut self) {
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

    pub fn run_command(&mut self, cmd: &str) -> Result<(), CommandError> {
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
    pub fn start(&mut self) -> Result<(), StartError> {
        self.check_stop();
        match self {
            Idle => {
                let child = match Command::new("sh")
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
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
    pub fn stop(&mut self) -> Result<(), StopError> {
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
                    None => {
                        Err(StopError::ProccesError)
                    }
                }
            }
            BackingUp => Err(StopError::Downloading),
            Idle => Err(StopError::Idle),
        }
    }

    pub fn backup(&mut self) -> Result<(), BackupError> {
        match self {
            Idle => {
                *self = BackingUp;
                // Compress the file
                match Command::new("tar")
                    .arg("-czf")
                    .arg(format!("backups/{}.tar.gz", ran_letters(32)))
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
                *self = Idle;
                Ok(())
            }
            Running { procces: _ } => Err(BackupError::ServerRunning),
            BackingUp => Err(BackupError::OtherBackup),
        }
    }
}

use std::io::Write;
use std::process::{Child, Command, Stdio};
use ServerState::*;

lazy_static! {
    static ref CONFIG: crate::Config = crate::config_load();
    /// Contains the current procces of the minecraft server and it's stdin
    static ref STATE: antidote::Mutex<ServerState> = antidote::Mutex::new(Idle);
}

use magic_crypt::{new_magic_crypt, MagicCrypt256, MagicCryptTrait};
use rand::prelude::*;
use std::{collections::HashSet, sync::Mutex};
lazy_static! {
    static ref CRYPT: MagicCrypt256 = new_magic_crypt!(CONFIG.key.clone(), 256);
    static ref SOCKET: String = CONFIG.socket.clone();
    static ref KEY: String = CONFIG.key.clone();
    static ref KEYS: Mutex<HashSet<Key>> = Mutex::new(HashSet::new());
}
const KEY_BYTES: usize = 256;

#[derive(Eq, Clone, Hash, PartialEq, Debug)]
pub struct Key {
    pub action: AuthAction,
    pub key: Vec<u8>,
}

pub fn encrypt(data: Vec<u8>) -> Vec<u8> {
    CRYPT.encrypt_bytes_to_bytes(&data)
}

/// Generate some some random bytes for authentification
fn gen_bytes(key_bytes: usize) -> Vec<u8> {
    let mut rng = thread_rng();
    let mut bytes: Vec<u8> = Vec::with_capacity(key_bytes);
    for _ in 0..key_bytes {
        bytes.push(rng.gen());
    }
    bytes
}

/// Create a new key to give to our client, we will store it so it can be verified later
pub fn gen_key(action: AuthAction) -> Vec<u8> {
    // Keys must be initialised before use
    let mut set = KEYS.lock().expect("Mutex poisoned");
    let bytes = gen_bytes(KEY_BYTES);
    set.insert(Key {
        key: bytes.clone(),
        action,
    });
    bytes
}
/// Check that a key has been authored by us
pub fn verify_key(key: Key) -> bool {
    let mut set = KEYS.lock().expect("Mutex poisend");
    let res = set.remove(&key);
    res
}

fn latest_file(dir: &str) -> Option<PathBuf> {
    let files = match std::fs::read_dir(dir) {
        Ok(files) => files,
        Err(_) => return None,
    }
    .into_iter();

    match files
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
        .max_by_key(|t| t.1)
    {
        Some(tuple) => Some(tuple.0),
        None => None,
    }
}

fn ran_letters(len: usize) -> String {
    let mut string = String::with_capacity(len);
    let mut rng = thread_rng();
    for _ in 0..len {
        string.push(match rng.gen_range(0..26) {
            0 => 'a',
            1 => 'b',
            2 => 'c',
            3 => 'd',
            4 => 'e',
            5 => 'f',
            6 => 'g',
            7 => 'h',
            8 => 'i',
            9 => 'j',
            10 => 'k',
            11 => 'l',
            12 => 'm',
            13 => 'n',
            14 => 'o',
            15 => 'p',
            16 => 'q',
            17 => 'r',
            18 => 's',
            19 => 't',
            20 => 'u',
            21 => 'v',
            22 => 'w',
            23 => 'x',
            24 => 'y',
            _ => 'z',
        });
    }

    string
}
