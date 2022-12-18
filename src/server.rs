#[macro_use]
extern crate lazy_static;
use actions::controller_server::{Controller, ControllerServer};
use actions::{
    AuthRequest, AuthResponce, BackupRequest, CommandRequest, DownloadRequest, OpResponce,
    StartRequest, StopRequest, WorldDownload,
};
use tonic::{transport::Server, Request, Response, Status};
pub mod actions {
    tonic::include_proto!("actions");
}

mod common;

use common::Actions;

#[derive(Eq, Clone, Hash, PartialEq)]
pub enum OpResult {
    Success = 0,
    Fail = 1,
    Denied = 2,
}

impl OpResult {
    pub fn code(&self) -> i32 {
        match self {
            Self::Success => 0,
            Self::Fail => 1,
            Self::Denied => 2,
        }
    }
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
    let conf = std::fs::read("server.toml").expect("Unable to load config file");
    toml::from_slice(&conf).expect("Unable to parse config, (syntax error)")
}

/// Used to set up our server, we will impl all methods outlined in proto/actions.proto on this struct
#[derive(Debug, Default)]
struct ControllerService {}

/// Shorthand for Ok(Responce::new(OpResponce{result: code, comment: comment}))
fn respond(code: OpResult, comment: &str) -> Result<Response<OpResponce>, Status> {
    Ok(Response::new(OpResponce {
        result: code.code(),
        comment: comment.to_owned(),
    }))
}

#[tonic::async_trait]
impl Controller for ControllerService {
    async fn auth(&self, req: Request<AuthRequest>) -> Result<Response<AuthResponce>, Status> {
        use Actions::*;
        let action = match req.into_inner().action {
            0 => Start,
            1 => Stop,
            2 => Command,
            3 => Download,
            _ => {
                return Ok(Response::new(AuthResponce {
                    result: OpResult::Fail.code(),
                    key: Vec::new(),
                    comment: "Invalid action".to_string(),
                }));
            }
        };
        let key = gen_key(action);
        let encypted_key = encrypt(key);
        let result = OpResult::Success.code();
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
            action: Actions::Download,
        }) {
            return Err(Status::new(tonic::Code::InvalidArgument, "Invalid token"));
        }

        todo!()
    }

    async fn command(&self, req: Request<CommandRequest>) -> Result<Response<OpResponce>, Status> {
        let req = req.into_inner();
        let key = req.token;
        if !verify_key(Key {
            key,
            action: Actions::Command,
        }) {
            return respond(OpResult::Denied, "Invalid token");
        }
        let mut state = match STATE.lock(){
          Ok(lock )=> lock,  
            Err(_) => return respond(OpResult::Fail, "Lock not aquired"),
        };
        let res = state.run_command(&req.command);
        match res {
            Err(command_error) => {
                match command_error{
                    CommandError::Idle => {
                        respond(OpResult::Fail, "Server stopped, comamnd can't be run")
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
            action: Actions::Download,
        }) {
            return Err(Status::new(tonic::Code::InvalidArgument, "Invalid token"));
        }
        let mut state = match STATE.lock(){
          Ok(lock )=> lock,  
            Err(_) => return Ok(Response::new(WorldDownload { result: OpResult::Fail.code(), comment: "Lock poisned".to_string(), data: Vec::new()})),
        };
        let world_bytes = state.wdl();
        match world_bytes {
            Ok(data) => {
                let result = OpResult::Success.code();
                let wdl = WorldDownload {
                    data,
                    result,
                    comment: "Starting Download".to_string(),
                };
                println!("Download succesful");
                Ok(Response::new(wdl))
            }
            Err(download_error) => match download_error {
                DownloadError::ServerRunning => {
                    let result = OpResult::Fail.code();
                    let wdl = WorldDownload {
                        data: Vec::new(),
                        result,
                        comment: "Server running stop to download".to_string(),
                    };
                    Ok(Response::new(wdl))
                }
                DownloadError::OtherDownload => {
                    let result = OpResult::Fail.code();
                    let wdl = WorldDownload {
                        data: Vec::new(),
                        result,
                        comment: "Service occupided by other user".to_string(),
                    };
                    Ok(Response::new(wdl))
                }
                DownloadError::FSReadError => {
                    let result = OpResult::Fail.code();
                    let wdl = WorldDownload {
                        data: Vec::new(),
                        result,
                        comment: "Unable to get world data".to_string(),
                    };
                    Ok(Response::new(wdl))
                }
                DownloadError::Compression => {
                    let result = OpResult::Fail.code();
                    let wdl = WorldDownload {
                        data: Vec::new(),
                        result,
                        comment: "Unable to compress world data".to_string(),
                    };
                    Ok(Response::new(wdl))
                }
            },
        }
    }

    ///Handle startup request
    async fn start(&self, req: Request<StartRequest>) -> Result<Response<OpResponce>, Status> {
        let key = req.into_inner().token;
        if !verify_key(Key {
            key,
            action: Actions::Start,
        }) {
            return respond(OpResult::Denied, "Invalid Token");
        }

        println!("Start request recived");
        let mut state = match STATE.lock(){
          Ok(lock )=> lock,  
            Err(_) => return respond(OpResult::Fail, "Lock not aquired"),
        };
        let res = state.start();
        match res {
            Ok(_) => {
                println!("Started minecraft server succesfully!");
                respond(OpResult::Success, "Started succesfuly")
            }
            Err(start_error) => match start_error {
                StartError::Launch => respond(OpResult::Fail, "Failed to launch server"),
                StartError::AlreadyRunning => respond(OpResult::Fail, "Server already running"),
                StartError::Downloading => respond(OpResult::Fail, "Download in proggress! Can't start"),
            },
        }
    }

    /// Handle stoping
    async fn stop(&self, req: Request<StopRequest>) -> Result<Response<OpResponce>, Status> {
        let key = req.into_inner().token;
        use common::Actions;
        if !verify_key(Key {
            key,
            action: Actions::Stop,
        }) {
            return respond(OpResult::Denied, "Invalid token");
        }
        let mut state = match STATE.lock(){
          Ok(lock )=> lock,  
            Err(_) => return respond(OpResult::Fail, "Lock not aquired"),
        };
        println!("Stop request recived");
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
                println!("Minecraft server stopped succesfully");
                return respond(OpResult::Success, "Server stopped successfuly");
            }
        }
    }
}


    #[derive(Debug)]
    pub enum ServerState {
        Idle,
        Running { procces: Child },
        Downloading,
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
    pub enum DownloadError {
        ServerRunning,
        OtherDownload,
        FSReadError,
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
                Downloading => Err(CommandError::Downloading),
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
                Downloading => Err(StartError::Downloading),
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
                            println!("Unable to stop minecraft server");
                            Err(StopError::ProccesError)
                        }
                    }
                }
                Downloading => Err(StopError::Downloading),
                Idle => Err(StopError::Idle),
            }
        }

        pub fn wdl(&mut self) -> Result<Vec<u8>, DownloadError> {
            println!("Starting world download");
            // Send world-file to client
            match self {
                Idle => {
                    *self = Downloading;
                    // Compress the file
                    match Command::new("tar")
                        .arg("-czf")
                        .arg("worldupload.tar.gz")
                        .arg("world")
                        .spawn()
                    {
                        Ok(mut child) => {
                            if let Err(_) = child.wait() {
                                return Err(DownloadError::Compression);
                            }
                        }
                        Err(_) => {
                            return Err(DownloadError::Compression);
                        }
                    }
                    let data = match std::fs::read("worldupload.tar.gz") {
                        Ok(data) => data,
                        Err(_) => return Err(DownloadError::FSReadError),
                    };
                    *self = Idle;
                    Ok(data)
                }
                Running { procces: _ } => Err(DownloadError::ServerRunning),
                Downloading => Err(DownloadError::OtherDownload),
            }
        }
    }


    use std::io::Write;
    use std::process::{Child, Command, Stdio};
    use ServerState::*;
    /// Contains the current procces of the minecraft server and it's stdin
    pub static STATE: Mutex<ServerState> = Mutex::new(Idle);

lazy_static! {
    static ref CONFIG: crate::Config = crate::config_load();
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
        pub action: Actions,
        pub key: Vec<u8>,
    }

    pub fn encrypt(data: Vec<u8>) -> Vec<u8> {
        CRYPT.encrypt_bytes_to_bytes(&data)
    }

    /// Generate some some random bytes for authentification
    fn gen_bytes() -> Vec<u8> {
        let mut rng = thread_rng();
        let mut bytes: Vec<u8> = Vec::with_capacity(KEY_BYTES);
        for _ in 0..KEY_BYTES {
            bytes.push(rng.gen());
        }
        bytes
    }

    /// Create a new key to give to our client, we will store it so it can be verified later
    pub fn gen_key(action: Actions) -> Vec<u8> {
        // Keys must be initialised before use
        let mut set = KEYS.lock().expect("Mutex poisoned");
        let bytes = gen_bytes();
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

