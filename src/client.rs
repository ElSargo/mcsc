mod common;
pub mod actions {
    tonic::include_proto!("actions");
}

use actions::{
    controller_client::ControllerClient, AuthAction, AuthRequest, BackupRequest, CommandRequest,
    DownloadRequest, OpResponce, OpResult, StartRequest, StopRequest,
};
use common::ran_letters;
use magic_crypt::{new_magic_crypt, MagicCryptTrait};
use serde_derive::Deserialize;
use std::{fs, io::Write, thread::sleep, time::Duration};
use tonic::Response;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = read_config();

    print!(
        "
Welcome to mcsc, NOTE: these operations take time to complete so be patent
Enter a command: either by name or the number next to it
0|\"Start\" to request a startup or 
1|\"Stop\" to request a shutdown or 
2|\"Backup\" to create a backup or 
3|\"Command\" to run a command
4|\"Download\" to download the latest backup
=> "
    );

    let mut input = String::new();
    let _ = std::io::Write::flush(&mut std::io::stdout());
    std::io::stdin()
        .read_line(&mut input)
        .expect("Could not read input");

    let response: Response<OpResponce>;

    match input.as_str() {
        // Starts the server, does not wait for it to be ready or for a launch fail
        "0\n" | "Start\n" | "0\r\n" | "Start\r\n" => {
            println!("[Awaiting server response...]");
            let mut client = ControllerClient::connect(config.ip).await?;
            let key = client
                .auth(AuthRequest {
                    action: AuthAction::Start.into(),
                })
                .await?
                .into_inner();
            println!("[Server connection status: {}]", key.comment);
            let token = decrypt(key.key, config.key).expect("Client side auth error occurred");
            let request = StartRequest { token };
            response = client.start(request).await?;
        }

        // Stops the server by sending the stop command to stdin
        "1\n" | "Stop\n" | "1\r\n" | "Stop\r\n" => {
            println!("[Awaiting server response...]");
            let mut client = ControllerClient::connect(config.ip).await?;
            let key = client
                .auth(AuthRequest {
                    action: AuthAction::Stop.into(),
                })
                .await?
                .into_inner();
            println!("[Server connection status: {}]", key.comment);
            let token = decrypt(key.key, config.key).expect("Client side auth error occurred");
            let request = StopRequest { token };
            response = client.stop(request).await?;
        }

        // Signals the server to backup the world to a compressed archive
        "2\n" | "Backup\n" | "2\r\n" | "Backup\r\n" => {
            println!("[Awaiting server response...]");
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
            response = client.backup(request).await?;
        }

        // Attempts to run a minecraft command
        "3\n" | "Command\n" | "3\r\n" | "Command\r\n" => {
            print!("Enter command \n=> ");
            let _ = std::io::Write::flush(&mut std::io::stdout());
            let mut command = String::new();
            if let Err(_) = std::io::stdin().read_line(&mut command) {
                println!("Error reading input");
                return Ok(());
            }
            println!("[Awaiting server response...]");
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
            response = client.command(request).await?;
        }

        // Downloads the latest backup from the server
        "4\n" | "Download\n" | "4\r\n" | "Download\r\n" => {
            println!("[Awaiting server response...]");
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
            // Download complete, show location
            let working_directory = std::env::current_dir();
            match working_directory {
                Ok(wdir) => println!("Saved as {:?} {}", wdir, path),
                Err(_) => println!("Download saved as {}", path),
            }
            return Ok(());
        }

        // No action recognised
        _ => {
            println!("Invalid input");
            sleep(Duration::from_secs(1));
            std::process::exit(0);
        }
    }
    let success = response.into_inner();
    match success.result {
        0 => {
            println!("Success!, server comment: {}", success.comment)
        }
        1 => {
            println!("Failed!, server comment: {}", success.comment)
        }
        2 => {
            println!("Denied!, server comment: {}", success.comment)
        }
        _ => {
            println!("Something fucked up!, server comment: {}", success.comment)
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
