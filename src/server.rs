#[allow(dead_code, unused_variables)]
use tonic::{transport::Server, Request, Response, Status};

use actions::controller_server::{Controller, ControllerServer};
use actions::{OpResponce, StartRequest, StopRequest};
use core::result::Result;
use std::error::Error;
use std::io::Write;
use std::process::{Child, Command, Stdio};
pub mod actions {
    tonic::include_proto!("actions");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::Path::new("/home/oliver/minecraft_server_controller/minecraft");
    assert!(std::env::set_current_dir(&root).is_ok());
    let addy = "[::1]:50051".parse()?;
    let cs = ControllerService::default();
    Server::builder()
        .add_service(ControllerServer::new(cs))
        .serve(addy)
        .await?;

    Ok(())
}

#[derive(Debug, Default)]
struct ControllerService {}

#[tonic::async_trait]
impl Controller for ControllerService {
    async fn start(&self, req: Request<StartRequest>) -> Result<Response<OpResponce>, Status> {
        println!("Start");
        if poll() == false {
            let res = start();
            match res {
                Ok(_) => Ok(Response::new(OpResponce { result: 0 })),
                Err(e) => {
                    println!("{:?}", e);
                    Ok(Response::new(OpResponce { result: 1 }))
                }
            }
        } else {
            println!("Already Running");
            Ok(Response::new(OpResponce { result: 2 }))
        }
    }

    async fn stop(&self, req: Request<StopRequest>) -> Result<Response<OpResponce>, Status> {
        if poll() == true {
            println!("stop");
            let res = stop();
            match res {
                Err(_) => Ok(Response::new(OpResponce { result: 1 })),
                Ok(_) => Ok(Response::new(OpResponce { result: 0 })),
            }
        } else {
            println!("Already stopped");
            Ok(Response::new(OpResponce { result: 2 }))
        }
    }
}

static mut STATE: Option<Child> = None;

fn poll() -> bool {
    unsafe {
        match &mut STATE {
            Some(c) => {
                let res = c.try_wait();
                match res {
                    Ok(bruh) => match bruh {
                        Some(_) => {
                            //Procces finished
                            STATE = None;
                            false
                        }
                        //Procces running
                        None => true,
                    },
                    //Procces has no stdin, potential issue
                    Err(_) => false,
                }
            }
            //No procces
            None => false,
        }
    }
}

fn start() -> Result<(), Box<dyn Error>> {
    //screen -dmS "minecraft" java -Xmx1024M -Xms1024M -jar minecraft_server.jar nogui
    let child = Command::new("java")
        .stdin(Stdio::piped())
        .arg("-Xmx1024M")
        .arg("-Xms1024M")
        .arg("-jar")
        .arg("fabric-server-launch.jar")
        .arg("nogui")
        .spawn()?;
    unsafe {
        STATE = Some(child);
        println!("{:?}", STATE);
    }
    Ok(())
}

fn stop() -> Result<(), Box<dyn Error>> {
    unsafe {
        match &mut STATE {
            Some(child) => {
                let c_stdin = child.stdin.as_mut();
                match c_stdin {
                    Some(buff) => {
                        buff.write_all(b"stop\n")?;
                        STATE = None;
                        child.wait();
                        ()
                    }
                    None => {
                        println!("Didn't stop");
                        ()
                    }
                };
            }
            None => (),
        }
    }
    print!("stop");
    Ok(())
}
