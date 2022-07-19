use actions::controller_server::{Controller, ControllerServer};
use actions::{DownloadRequest, OpResponce, StartRequest, StopRequest, WorldDownload};
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
    let socket = "[::1]:50051".parse()?;
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

#[tonic::async_trait]
impl Controller for ControllerService {
    ///Handle startup request
    async fn start(&self, req: Request<StartRequest>) -> Result<Response<OpResponce>, Status> {
        let token = req.into_inner().token;
        let encrypted_data = security::decrypt(token.to_owned());
        match encrypted_data {
            Ok(key) if key == security::TOKEN => {
                println!("Token is valid");
            }
            _ => {
                println!("Invalid token {token}");
                return Err(Status::new(tonic::Code::InvalidArgument, "Acces denied"));
            }
        }
        println!("Start request recived");
        if server_managing::poll() == false {
            let res = server_managing::start();
            match res {
                Ok(_) => {
                    println!("Started minecraft server succesfully!");
                    return Ok(Response::new(OpResponce { result: 0 }));
                }
                Err(e) => {
                    println!("Error starting minecraft server: \n{:?}", e);
                    return Ok(Response::new(OpResponce { result: 1 }));
                }
            }
        } else {
            println!("Minecraft server already running");
            Ok(Response::new(OpResponce { result: 2 }))
        }
    }

    /// Handle stoping
    async fn stop(&self, req: Request<StopRequest>) -> Result<Response<OpResponce>, Status> {
        let token = req.into_inner().token;
        let encrypted_data = security::decrypt(token.to_owned());
        match encrypted_data {
            Ok(key) if key == security::TOKEN => {
                println!("Token is valid");
            }
            _ => {
                println!("Invalid token {token}");
                return Err(Status::new(tonic::Code::InvalidArgument, "Acces denied"));
            }
        }
        if server_managing::poll() == true {
            println!("Stop request recived");
            let res = server_managing::stop();
            match res {
                Err(err) => {
                    println!("Error stopping minecraft server: \n {}", err);
                    return Ok(Response::new(OpResponce { result: 1 }));
                }
                Ok(_) => {
                    println!("Minecraft server stopped succesfully");
                    return Ok(Response::new(OpResponce { result: 0 }));
                }
            }
        } else {
            println!("Minecraft server already stopped");
            Ok(Response::new(OpResponce { result: 2 }))
        }
    }
    /// Requset to download the worldfile
    async fn download(
        &self,
        req: Request<DownloadRequest>,
    ) -> Result<Response<WorldDownload>, Status> {
        let token = req.into_inner().token;
        let encrypted_data = security::decrypt(token.to_owned());
        match encrypted_data {
            Ok(key) if key == security::TOKEN => {
                println!("Token is valid");
            }
            _ => {
                println!("Invalid token {token}");
                return Err(Status::new(tonic::Code::InvalidArgument, "Acces denied"));
            }
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
        // minecraft" java -Xmx1024M -Xms1024M -jar fabric-server-launch.jar nogui
        // TODO use bash scrpit to launch server
        let child = Command::new("java")
            .stdin(Stdio::piped())
            .arg("-Xmx1024M")
            .arg("-Xms1024M")
            .arg("-jar")
            .arg("fabric-server-launch.jar")
            .arg("nogui")
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
    // Send worldfile to client
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
    static mut CRYPT: Option<MagicCrypt256> = None;
    pub const TOKEN: &str = "Thats cauze all my niggas in:";
    fn init() {
        unsafe {
            CRYPT = Some(new_magic_crypt!("Who was in paris?.....", 256));
        }
    }

    pub fn decrypt(txt: String) -> Result<String, magic_crypt::MagicCryptError> {
        unsafe {
            match &CRYPT {
                Some(key) => {
                    return key.decrypt_base64_to_string(txt);
                }
                None => {
                    init();
                    return decrypt(txt);
                }
            }
        }
    }
}
