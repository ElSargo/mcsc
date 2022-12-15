#[macro_use]
extern crate lazy_static;
use actions::controller_server::{Controller, ControllerServer};
use actions::{
    AuthRequest, AuthResponce, CommandRequest, DownloadRequest, OpResponce, StartRequest,
    StopRequest, WorldDownload,
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
    ///Handle startup request
    async fn start(&self, req: Request<StartRequest>) -> Result<Response<OpResponce>, Status> {
        use security::{verify_key, Key};
        let key = req.into_inner().token;
        if !verify_key(Key {
            key,
            action: Actions::Start,
        }) {
            return respond(OpResult::Denied, "Invalid Token");
        }

        println!("Start request recived");
        let mut state = server_managing::STATE.lock();
        if state.poll() == false {
            let res = state.start();
            match res {
                Ok(_) => {
                    println!("Started minecraft server succesfully!");
                    return respond(OpResult::Success, "Started succesfuly");
                }
                Err(e) => {
                    println!("Error starting minecraft server: \n{:?}", e);
                    return respond(OpResult::Fail, "Failed to start server");
                }
            }
        } else {
            println!("Minecraft server already running");
            return respond(OpResult::Fail, "Server already running");
        }
    }

    /// Handle stoping
    async fn stop(&self, req: Request<StopRequest>) -> Result<Response<OpResponce>, Status> {
        let key = req.into_inner().token;
        use common::Actions;
        use security::{verify_key, Key};
        if !verify_key(Key {
            key,
            action: Actions::Stop,
        }) {
            return respond(OpResult::Denied, "Invalid token");
        }
        let mut state = server_managing::STATE.lock();
        if state.poll() == true {
            println!("Stop request recived");
            let res = state.stop();
            match res {
                Err(_) => {
                    println!("Error stopping minecraft server");
                    return respond(OpResult::Fail, "Error occured while stopping server");
                }
                Ok(_) => {
                    println!("Minecraft server stopped succesfully");
                    return respond(OpResult::Success, "Server stopped successfuly");
                }
            }
        } else {
            println!("Minecraft server already stopped");
            return respond(OpResult::Fail, "Server already stopped");
        }
    }

    /// Requset to download the worldfile
    async fn download(
        &self,
        req: Request<DownloadRequest>,
    ) -> Result<Response<WorldDownload>, Status> {
        let key = req.into_inner().token;
        if !security::verify_key(security::Key {
            key,
            action: Actions::Download,
        }) {
            return Err(Status::new(tonic::Code::InvalidArgument, "Invalid token"));
        }
        let mut state = server_managing::STATE.lock();
        if !state.poll_wdl(){
            println!("Starting world download");
            let fs_read = download::get_world_bytes(&mut state);
            match fs_read {
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
                Err(_) => {
                    let result = OpResult::Fail.code();
                    let wdl = WorldDownload {
                        data: Vec::new(),
                        result,
                        comment: "Unable to get world data".to_string(),
                    };
                    println!("Download succesful");
                    Ok(Response::new(wdl))
                }
            }
        } else {
            let result = OpResult::Fail.code();
            let wdl = WorldDownload {
                data: Vec::new(),
                result,
                comment: "Service occupided by other user".to_string(),
            };
            Ok(Response::new(wdl))
        }
    }

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
        let key = security::gen_key(action);
        let encypted_key = security::encrypt(key);
        let result = OpResult::Success.code();
        Ok(Response::new(AuthResponce {
            result,
            key: encypted_key,
            comment: "Succces".to_string(),
        }))
    }

    async fn command(&self, req: Request<CommandRequest>) -> Result<Response<OpResponce>, Status> {
        let req = req.into_inner();
        let key = req.token;
        use security::{verify_key, Key};
        if !verify_key(Key {
            key,
            action: Actions::Command,
        }) {
            return respond(OpResult::Denied, "Invalid token");
        }
        let mut state = server_managing::STATE.lock();
        if state.poll() == true {
            let res = state.run_command(&req.command);
            match res {
                Err(_) => {
                    return respond(OpResult::Fail, "Error running command");
                }
                Ok(_) => {
                    return respond(OpResult::Success, "Command ran succesfully! note this does not nessisarly mean the command was valid only that it's execution was attempted");
                }
            }
        } else {
            return respond(OpResult::Fail, "Server stopped, comamnd can't be run");
        }
    }
}

/// Start and stop the minecraft server
mod server_managing {

    #[derive(Debug)]
    pub enum ServerState {
        Stoped,
        Running { procces: Child },
        Downloading,
    }

    #[derive(Debug)]
    struct StateError {
        needed: ServerState,
        got: String,
    }

    impl Display for StateError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(
                f,
                "StateError: nedded: {:?}, got: {:?}",
                self.needed, self.got
            )
        }
    }

    impl std::error::Error for StateError {}

    impl ServerState {
        pub fn get_child(&mut self) -> Option<&mut Child> {
            match self {
                Running { procces } => Some(procces),
                _ => None,
            }
        }

        pub fn poll_wdl(&self) -> bool {
            match self {
                ServerState::Downloading => true,
                _ => false,
            }
        }

        pub fn set_wdl(&mut self, status: bool) {
            if status {
                match self {
                    ServerState::Stoped => *self = ServerState::Downloading,
                    _ => {}
                }
            } else {
                match self {
                    ServerState::Downloading => *self = ServerState::Stoped,
                    _ => {}
                }
            }
        }

        pub fn run_command(&mut self, cmd: &str) -> Result<(), ()> {
            match self {
                Running { procces } => {
                    let pstdin = procces.stdin.as_mut();
                    match pstdin {
                        Some(buff) => match buff.write_all(&format!("\n{}\n", cmd).into_bytes()) {
                            Err(_) => Err(()),
                            _ => Ok(()),
                        },
                        None => Err(()),
                    }
                }
                _ => Err(()),
            }
        }

        /// Stop the running procces by entering stop into the stdin
        pub fn stop(&mut self) -> Result<(), ()> {
            match self.get_child() {
                Some(child) => {
                    let child_input = child.stdin.as_mut();
                    match child_input {
                        Some(buff) => {
                            match buff.write_all(b"\nstop\n") {
                                Err(_) => return Err(()),
                                _ => {}
                            };
                            match child.wait() {
                                Ok(_) => {}
                                Err(_) => {}
                            }
                            *self = Stoped;
                            Err(())
                        }
                        None => {
                            println!("Unable to stop minecraft server");
                            Err(())
                        }
                    }
                }
                _ => Err(()),
            }
        }

        /// Spawn a new java procces and store it in MINECRAFT_SERVER_STATE
        pub fn start(&mut self) -> Result<(), Box<dyn Error>> {
            match self {
                Stoped => {
                    let child = Command::new("sh")
                        .stdin(Stdio::piped())
                        .arg("launch.sh")
                        .spawn()?;
                    *self = Running { procces: child };
                    Ok(())
                }
                _ => Err(Box::new(StateError {
                    needed: ServerState::Stoped,
                    got: format!("{:?}", self),
                })),
            }
        }

        pub fn poll(&mut self) -> bool {
            match self {
                Running { procces: c } => {
                    let res = c.try_wait();
                    match res {
                        Ok(possible_exit_code) => match possible_exit_code {
                            Some(_exit_code) => {
                                //Procces finished
                                *self = Stoped;
                                false
                            }
                            //Procces running
                            None => true,
                        },
                        //Procces has no stdin, potential issue
                        //TODO investigate possible condtions for this branch to match and consequnces
                        Err(_) => false,
                    }
                }
                //No procces
                Stoped => false,
                Downloading => false,
            }
        }
    }
    use parking_lot::Mutex;
    use std::error::Error;
    use std::fmt::Display;
    use std::io::Write;
    use std::process::{Child, Command, Stdio};
    use ServerState::*;
    /// Contains the current procces of the minecraft server and it's stdin
    pub static STATE: Mutex<ServerState> = Mutex::new(Stoped);
}
mod download {
    // Send world-file to client
    use std::error::Error;
    use std::process::Command;

    use crate::server_managing::ServerState;
    
    //TODO world backup scheme
    fn create_tarball() -> Result<(), Box<dyn Error>> {
        Command::new("tar")
            .arg("-czf")
            .arg("worldupload.tar.gz")
            .arg("world")
            .spawn()?
            .wait()?;
        Ok(())
    }

    pub fn get_world_bytes(state: &mut ServerState) -> Result<Vec<u8>, Box<dyn Error>> {
        state.set_wdl(true);
        create_tarball()?;
        state.set_wdl(false);
        return Ok(std::fs::read("worldupload.tar.gz")?);
    }
}

lazy_static! {
    static ref CONFIG: crate::Config = crate::config_load();
}

mod security {
    use crate::common::Actions;
    use crate::CONFIG;
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
}
