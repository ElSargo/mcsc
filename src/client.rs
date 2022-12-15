pub mod actions {
    tonic::include_proto!("actions");
}
mod common;
use actions::controller_client::ControllerClient;
use actions::{AuthRequest, DownloadRequest, OpResponce, StartRequest, StopRequest};
use common::Actions;
use security::decrypt;
use std::fs;
use std::thread::sleep;
use std::time::Duration;
use tonic::Response;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = read_config();
    println!("Config: {:?}", config);

    println!("Enter a command: \n\"Start\" to request a startup or \n\"Stop\" to request a shutdown or \n\"Download\" to download the world file");

    let mut input = String::new();
    let _ = std::io::Write::flush(&mut std::io::stdout());
    std::io::stdin()
        .read_line(&mut input)
        .expect("Could not read input");

    let responce: Response<OpResponce>;

    match input.as_str() {
        "Start\n" => {
            let mut client = ControllerClient::connect(config.ip).await?;
            let key = client
                .auth(AuthRequest {
                    action: Actions::Start.code(),
                })
                .await?
                .into_inner();
            println!("[Server responce] {}", key.comment);
            let token = decrypt(key.key, config.key).expect("Client side auth error occured");
            let request = StartRequest { token };
            responce = client.start(request).await?;
        }

        "Stop\n" => {
            let mut client = ControllerClient::connect(config.ip).await?;
            let key = client
                .auth(AuthRequest {
                    action: Actions::Stop.code(),
                })
                .await?
                .into_inner();
            println!("[Server responce] {}", key.comment);
            let token = decrypt(key.key, config.key).expect("Client side auth error occured");
            let request = StopRequest { token };
            responce = client.stop(request).await?;
        }

        "Download\n" => {
            let mut client = ControllerClient::connect(config.ip).await?;
            let key = client
                .auth(AuthRequest {
                    action: Actions::Download.code(),
                })
                .await?
                .into_inner();
            println!("[Server responce] {}", key.comment);
            let token = decrypt(key.key, config.key).expect("Client side auth error occured");
            let request = DownloadRequest { token };
            let file = client.download(request).await?.into_inner().data;
            fs::write("worldbackup.tar.gz", file)?;
            let working_directory = std::env::current_dir();
            match working_directory {
                Ok(wdir) => println!("Saved as {:?} worldbackup.tar.gz", wdir),
                Err(_) => println!("Download saved as worldbackup.tar.gz"),
            }
            return Ok(());
        }

        _ => {
            println!("Invalid input");
            sleep(Duration::from_secs(1));
            std::process::exit(0);
        }
    }
    let success = responce.into_inner();
    match success.result {
        0 => {
            println!("Succes!, server comment: {}", success.comment)
        }
        1 => {
            println!("Failed!, server comment: {}", success.comment)
        }
        2 => {
            println!("Denied!, server comment: {}", success.comment)
        }
        _ => {
            println!("Somthing fucked up!, server comment: {}", success.comment)
        }
    }

    sleep(Duration::from_secs(1));
    Ok(())
}

use serde_derive::Deserialize;

#[derive(Deserialize, Debug)]
struct Config {
    ip: String,
    key: String,
}

fn read_config() -> Config {
    let text = std::fs::read("config.toml").expect("No config file!");
    toml::from_slice(&text).expect("No config file!")
}

mod security {
    use magic_crypt::{new_magic_crypt, MagicCryptTrait};

    pub fn decrypt(data: Vec<u8>, key: String) -> Result<Vec<u8>, magic_crypt::MagicCryptError> {
        let key = new_magic_crypt!(key, 256);
        Ok(key.decrypt_bytes_to_bytes(&data)?)
    }
}
