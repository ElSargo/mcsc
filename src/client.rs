mod common;
pub mod actions {
    tonic::include_proto!("actions");
}

use actions::{
    controller_client::ControllerClient, AuthRequest, DownloadRequest, OpResponce, OpResult,
    StartRequest, StopRequest, AuthAction, BackupRequest, CommandRequest
};
use common::ran_letters;
use std::{fs, io::Write, thread::sleep, time::Duration};
use tonic::Response;
use serde_derive::Deserialize;
use magic_crypt::{new_magic_crypt, MagicCryptTrait};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = read_config();
    println!("Config: {:?}", config);

    print!(
        "
Enter a command: either by name or the number next to it
0|\"Start\" to request a startup or 
1|\"Stop\" to request a shutdown or 
2|\"Backup\" to create a backup or 
3|\"Command\" to run a command
4|\"Download\" to download the latest backup
=>");

    let mut input = String::new();
    let _ = std::io::Write::flush(&mut std::io::stdout());
    std::io::stdin()
        .read_line(&mut input)
        .expect("Could not read input");

    let responce: Response<OpResponce>;

    match input.as_str() {

        // Starts the server, does not wait for it to be ready or for a launch fail
        "0\n" | "Start\n" => {
            let mut client = ControllerClient::connect(config.ip).await?;
            let key = client
                .auth(AuthRequest {
                    action: AuthAction::Start.into(),
                })
                .await?
                .into_inner();
            println!("[Server connection status: {}]", key.comment);
            let token = decrypt(key.key, config.key).expect("Client side auth error occured");
            let request = StartRequest { token };
            responce = client.start(request).await?;
        }

        // Stops the server by sending the stop command to stdin
        "1\n" | "Stop\n" => {
            let mut client = ControllerClient::connect(config.ip).await?;
            let key = client
                .auth(AuthRequest {
                    action: AuthAction::Stop.into(),
                })
                .await?
                .into_inner();
            println!("[Server connection status: {}]", key.comment);
            let token = decrypt(key.key, config.key).expect("Client side auth error occured");
            let request = StopRequest { token };
            responce = client.stop(request).await?;
        }

        // Signals the server to backup the world to a compressed archive
        "2\n" | "Backup\n" => {
            let mut client = ControllerClient::connect(config.ip).await?;
            let key = client
                .auth(AuthRequest {
                    action: AuthAction::Backup.into(),
                })
                .await?
                .into_inner();
            println!("[Server connection status: {}]", key.comment);
            let token = decrypt(key.key, config.key)?;
            let request = BackupRequest { token };
            responce = client.backup(request).await?;
        }

        // Attemps to run a minecraft command
        "3\n" | "Command\n" => {
            print!("Enter command \n=> ");
            let _ = std::io::Write::flush(&mut std::io::stdout());
            let mut command = String::new();
            if let Err(_) = std::io::stdin().read_line(&mut command) {
                println!("Error reading input");
                return Ok(());
            }
            let mut client = ControllerClient::connect(config.ip).await?;
            let key = client
                .auth(AuthRequest {
                    action: AuthAction::Command.into(),
                })
                .await?
                .into_inner();
            println!("[Server connection status: {}]", key.comment);
            let token = decrypt(key.key, config.key)?;
            let request = CommandRequest {
                token,
                command: command.to_string(),
            };
            responce = client.command(request).await?;
        }

        // Downloads the latest backup from the server    
        "4\n" | "Download\n" => {
            // Generate file name
            let path = format!("worldbackup-[{}].tar.gz", ran_letters(32));

            // Authorize
            let mut client = ControllerClient::connect(config.ip).await?;
            let key = client
                .auth(AuthRequest {
                    action: AuthAction::Download.into(),
                })
                .await?
                .into_inner();
            println!("[Server connection status: {}]", key.comment);
            let token = decrypt(key.key, config.key)?;
            let request = DownloadRequest { token };
            // Download file
            let mut stream = client.download(request).await?.into_inner();
            let mut file = fs::File::create(&path)?;
            while let Some(msg) = stream.message().await? {
                println!("{}", msg.comment);

                // Check for errors
                match OpResult::from_i32(msg.result) {
                    Some(res) => match res {
                        OpResult::Success => {}
                        _ => {
                            // Throw away redundant file to avoid confusion
                            let _ = std::fs::remove_file(path);
                            return Ok(());
                        }
                    },
                    _ => {}
                }

                file.write(&msg.data)?;
            }
            // Downlaod complete, show loaction
            let working_directory = std::env::current_dir();
            match working_directory {
                Ok(wdir) => println!("Saved as {:?} {}", wdir, path),
                Err(_) => println!("Download saved as {}", path),
            }
            return Ok(());
        }

        // No action recgnised
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

#[derive(Deserialize, Debug)]
struct Config {
    ip: String,
    key: String,
}

fn read_config() -> Config {
    let text = std::fs::read("mcsc_client.toml").expect("No config file!");
    toml::from_slice(&text).expect("No config file!")
}

pub fn decrypt(data: Vec<u8>, key: String) -> Result<Vec<u8>, magic_crypt::MagicCryptError> {
    let key = new_magic_crypt!(key, 256);
    Ok(key.decrypt_bytes_to_bytes(&data)?)
}
