use actions::controller_server::{Controller, ControllerServer};
use actions::{
    AuthKey, AuthRequest, DownloadRequest, OpResponce, StartRequest, StopRequest, WorldDownload,
};
use tonic::{transport::Server, Request, Response, Status};

pub mod actions {
    //Import the types defined for grpc
    tonic::include_proto!("actions");
}

/// Create a server that will allow users to start, stop a minecraft server as well as download the world file
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    {
        // Change working dir to ./minecraft
        // This is required for java to load the minecraft sever properly
        let current_working_directory =
            std::env::current_dir().expect("Couldn't load current working directory");
        let minecraft_directory = std::path::Path::new("minecraft");
        let mut working_directory = current_working_directory.to_path_buf();
        working_directory.push(minecraft_directory);
        std::env::set_current_dir(&working_directory)
            .expect(format!("Unable to set workingdir to {:?}", working_directory).as_ref());
    }
    //TODO change to real socket once on the actuall server
    let socket = security::SOCKET.parse()?;
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

fn respond(code: i32, comment: &str) -> Result<Response<OpResponce>, Status> {
    Ok(Response::new(OpResponce {
        result: code,
        comment: comment.to_owned(),
    }))
}
#[tonic::async_trait]
impl Controller for ControllerService {
    /// Shorthand for Ok(Responce::new(OpResponce{result: code, comment: comment}))

    ///Handle startup request
    async fn start(&self, req: Request<StartRequest>) -> Result<Response<OpResponce>, Status> {
        let key = req.into_inner().token;
        if !security::verify_key(key) {
            return respond(2, "Invalid Token");
        }

        println!("Start request recived");
        if server_managing::poll() == false {
            let res = server_managing::start();
            match res {
                Ok(_) => {
                    println!("Started minecraft server succesfully!");
                    return respond(0, "Started succesfuly");
                }
                Err(e) => {
                    println!("Error starting minecraft server: \n{:?}", e);
                    return respond(1, "Failed to start server");
                }
            }
        } else {
            println!("Minecraft server already running");
            return respond(1, "Server already running");
        }
    }

    /// Handle stoping
    async fn stop(&self, req: Request<StopRequest>) -> Result<Response<OpResponce>, Status> {
        let key = req.into_inner().token;
        if !security::verify_key(key) {
            return respond(2, "Invalid token");
        }

        if server_managing::poll() == true {
            println!("Stop request recived");
            let res = server_managing::stop();
            match res {
                Err(err) => {
                    println!("Error stopping minecraft server: \n {}", err);
                    return respond(1, "Error occured while stopping server");
                }
                Ok(_) => {
                    println!("Minecraft server stopped succesfully");
                    return respond(0, "Server stopped successfuly");
                }
            }
        } else {
            println!("Minecraft server already stopped");
            return respond(1, "Server already stopped");
        }
    }
    /// Requset to download the worldfile
    async fn download(
        &self,
        req: Request<DownloadRequest>,
    ) -> Result<Response<WorldDownload>, Status> {
        let key = req.into_inner().token;
        if !security::verify_key(key) {
            return Err(Status::new(tonic::Code::InvalidArgument, "Invalid token"));
        }
        if !download::poll_wdl() {
            println!("Starting world download");
            let fs_read = download::get_world_bytes();
            match fs_read {
                Ok(data) => {
                    let wdl = WorldDownload { data };
                    println!("Download succesful");
                    Ok(Response::new(wdl))
                }
                Err(_) => Err(Status::new(
                    tonic::Code::Internal,
                    "Error getting world data",
                )),
            }
        } else {
            println!("Can't downlaod bc another person is doing that already");
            Err(Status::new(
                tonic::Code::Internal,
                "Another download is already in proggress",
            ))
        }
    }

    async fn auth(&self, _req: Request<AuthRequest>) -> Result<Response<AuthKey>, Status> {
        let key = security::gen_key();
        let encypted_key = security::encrypt(key);
        Ok(Response::new(AuthKey { data: encypted_key }))
    }
}

/// Start and stop the minecraft server
mod server_managing {
    use std::error::Error;
    use std::io::Write;
    use std::process::{Child, Command, Stdio};
    /// Contains the current procces of the minecraft server and it's stdin
    static mut MINECRAFT_SERVER_STATE: Option<Child> = None;

    /// Returns true if minecraft is running
    pub fn poll() -> bool {
        unsafe {
            match &mut MINECRAFT_SERVER_STATE {
                Some(c) => {
                    let res = c.try_wait();
                    match res {
                        Ok(possible_exit_code) => match possible_exit_code {
                            Some(_exit_code) => {
                                //Procces finished
                                MINECRAFT_SERVER_STATE = None;
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
                None => false,
            }
        }
    }
    /// Spawn a new java procces and store it in MINECRAFT_SERVER_STATE
    pub fn start() -> Result<(), Box<dyn Error>> {
        let child = Command::new("sh")
            .stdin(Stdio::piped())
            .arg("launch.sh")
            .spawn()?;
        unsafe {
            MINECRAFT_SERVER_STATE = Some(child);
        }
        Ok(())
    }

    /// Stop the running procces by entering stop into the stdin
    pub fn stop() -> Result<(), Box<dyn Error>> {
        unsafe {
            match &mut MINECRAFT_SERVER_STATE {
                Some(child) => {
                    let c_stdin = child.stdin.as_mut();
                    match c_stdin {
                        Some(buff) => {
                            buff.write_all(b"stop\n")?;
                            MINECRAFT_SERVER_STATE = None;
                            match child.wait() {
                                Ok(_) => {}
                                Err(_) => {}
                            }
                            ()
                        }
                        None => {
                            println!("Unable to stop minecraft server");
                            ()
                        }
                    };
                }
                None => (),
            }
        }
        Ok(())
    }
}
mod download {
    // Send world-file to client
    use std::error::Error;
    use std::process::Command;

    static mut WORLD_DOWNLOADER_STATE: bool = false;

    pub fn poll_wdl() -> bool {
        unsafe {
            return WORLD_DOWNLOADER_STATE;
        }
    }

    fn set_wdl(status: bool) {
        unsafe {
            WORLD_DOWNLOADER_STATE = status;
        }
    }

    fn create_tarball() -> Result<(), Box<dyn Error>> {
        Command::new("tar")
            .arg("-czf")
            .arg("worldupload.tar.gz")
            .arg("world")
            .spawn()?
            .wait()?;
        Ok(())
    }

    pub fn get_world_bytes() -> Result<Vec<u8>, Box<dyn Error>> {
        set_wdl(true);
        create_tarball()?;
        set_wdl(false);
        return Ok(std::fs::read("worldupload.tar.gz")?);
    }
}

mod security {
    use magic_crypt::{new_magic_crypt, MagicCrypt256, MagicCryptTrait};
    use rand::prelude::*;
    use std::collections::HashSet;
    static mut KEYS: Option<HashSet<Vec<u8>>> = None;
    const KEY_BYTES: usize = 256;
    static mut CRYPT: Option<MagicCrypt256> = None;
    pub const SOCKET: &str = "[::1]:50051";
    // Don't change
    fn init_crypt() {
        unsafe {
            // Do not change...
            CRYPT = Some(new_magic_crypt!("Who was in paris?.....", 256));
        }
    }

    pub fn encrypt(data: Vec<u8>) -> Vec<u8> {
        unsafe {
            match &CRYPT {
                Some(key) => {
                    return key.encrypt_bytes_to_bytes(&data);
                }
                None => {
                    init_crypt();
                    return encrypt(data);
                }
            }
        }
    }
    /// Initailse the key set
    fn init_keys() {
        unsafe { KEYS = Some(HashSet::new()) }
    }

    /// Generate some some random bytes for authentification
    fn gen_bytes() -> Vec<u8> {
        let mut rng = thread_rng();
        let mut bytes = Vec::<u8>::new();
        for _ in 0..KEY_BYTES {
            bytes.push(rng.gen());
        }
        bytes
    }

    /// Create a new key to give to our client, we will store it so it can be verified later
    pub fn gen_key() -> Vec<u8> {
        unsafe {
            match &KEYS {
                Some(map) => {
                    let bytes = gen_bytes();
                    let mut new_map = map.clone();
                    new_map.insert(bytes.clone());
                    KEYS = Some(new_map);
                    return bytes;
                }
                None => {
                    init_keys();
                    return gen_key();
                }
            }
        }
    }
    /// Check that a key has been authored by us
    pub fn verify_key(key: Vec<u8>) -> bool {
        unsafe {
            match &KEYS {
                Some(map) => {
                    let mut new_map = map.clone();
                    let res = new_map.remove(&key);
                    if new_map.len() < 100 {
                        KEYS = Some(new_map);
                    } else {
                        KEYS = Some(HashSet::new());
                    }
                    return res;
                }
                None => {
                    init_keys();
                    return false;
                }
            }
        }
    }
}
