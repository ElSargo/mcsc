mod common;
pub mod actions {
    tonic::include_proto!("actions");
}

use actions::{
    controller_client::ControllerClient, AuthAction, AuthRequest, BackupRequest, CommandRequest,
    DownloadRequest, LaunchRequest, StopRequest,
};
use common::ran_letters;
use magic_crypt::{new_magic_crypt, MagicCryptTrait};
use regex::Regex;
use serde_derive::Deserialize;
use std::{fs, io::Write};
use tonic::transport::Channel;

use crate::actions::OpResult;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config: Config = {
        let text = std::fs::read("mcsc_client.toml").expect("No config file!");
        toml::from_slice(&text).expect("No config file!")
    };

    print!(
        "
Welcome to mcsc, NOTE: these operations take time to complete so be patent
Enter a command: either by name or the number next to it
0|\"Launch\" to request a server launch or 
1|\"Stop\" to request a shutdown or 
2|\"Backup\" to create a backup or 
3|\"Command\" to run a command
4|\"Download\" to download the latest backup
=> "
    );
    let regex_match = |reg, str| Regex::new(reg).unwrap().is_match(str);
    let input = read_input();
    let mut client = ControllerClient::connect(config.ip.to_owned()).await?;
    // Launch the server
    let response = if regex_match(r"((?i)Launch(?-i)|0)", &input) {
        let token = auth(&mut client, AuthAction::Launch, &config).await?;
        let request = LaunchRequest { token };
        client.launch(request).await?
    // Stop the server
    } else if regex_match(r"((?i)Stop(?-i)|1)", &input) {
        let token = auth(&mut client, AuthAction::Stop, &config).await?;
        let request = StopRequest { token };
        client.stop(request).await?
    // Take backup
    } else if regex_match(r"((?i)Backup(?-i)|2)", &input) {
        let token = auth(&mut client, AuthAction::Backup, &config).await?;
        let request = BackupRequest { token };
        client.backup(request).await?
    // Run Command
    } else if regex_match(r"((?i)Command(?-i)|3)", &input) {
        print!("Enter command \n=> ");
        let command = read_input();
        let token = auth(&mut client, AuthAction::Command, &config).await?;
        let request = CommandRequest {
            token,
            command: command.to_owned(),
        };
        client.command(request).await?
    // Download latest backup
    } else if regex_match(r"((?i)Download(?-i)|4)", &input) {
        // Generate file name
        let uuid = ran_letters(32);
        let path = format!("worldbackup-[{uuid}].tar.gz",);
        let token = auth(&mut client, AuthAction::Download, &config).await?;
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
    else {
        println!("Invalid input");
        std::process::exit(0);
    };
    let success = response.into_inner();
    let status_msg = match success.result {
        0 => "Success!",
        1 => "Failed!",
        2 => "Denied!",
        _ => "Something fucked up!",
    };
    println!("{status_msg}, server comment: {}", success.comment);

    Ok(())
}

#[derive(Deserialize, Debug)]
struct Config {
    ip: String,
    key: String,
}

fn decrypt(data: &Vec<u8>, key: &str) -> Result<Vec<u8>, magic_crypt::MagicCryptError> {
    let key = new_magic_crypt!(key, 256);
    Ok(key.decrypt_bytes_to_bytes(data)?)
}

async fn auth(
    client: &mut ControllerClient<Channel>,
    action: AuthAction,
    config: &Config,
) -> Result<Vec<u8>, tonic::Status> {
    println!("[Awaiting server response...]");
    let key = client
        .auth(AuthRequest {
            action: action.into(),
        })
        .await?
        .into_inner();
    println!("[Server connection status: {}]", key.comment);
    Ok(decrypt(&key.key, &config.key).expect("Client side auth error occurred"))
}

fn read_input() -> String {
    let mut input = String::new();
    let _ = std::io::Write::flush(&mut std::io::stdout());
    std::io::stdin()
        .read_line(&mut input)
        .expect("Could not read input");
    input
}
