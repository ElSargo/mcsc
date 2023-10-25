mod common;
mod net;
use crate::net::{Request, Responce};
use color_eyre::{Report, Result};
use common::ran_letters;
use lazy_regex::regex_is_match;
use magic_crypt::{new_magic_crypt, MagicCryptTrait};
use net::Token;
use serde_derive::Deserialize;
use std::{fs, io::Write};
use tokio::net::TcpSocket;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "Welcome to mcsc, NOTE: these operations take time to complete so be patent Enter a command: either by name or the number next to it"
    );

    let config: Config = {
        let bytes = std::fs::read("mcsc_client.toml").expect("No config file!");
        let text = std::str::from_utf8(&bytes).expect("Config file encoding error");
        toml::from_str(text).expect("No config file!")
    };

    loop {
        let _ = procces_request(&config).await;
    }
}

async fn procces_request(config: &Config) -> Result<()> {
    print!(
        "
0 | \'Launch\'   to request a server launch or
1 | \'Stop\'     to request a shutdown or
2 | \'Backup\'   to create a backup or
3 | \'Command\'  to run a command
4 | \'Download\' to download the latest backup
=> "
    );
    let input = read_input();

    // Get a socekt
    let addy = config.ip.parse()?;
    let socket = TcpSocket::new_v4()?;

    let mut stream = socket.connect(addy).await?;
    let (read_half, write_half) = stream.split();

    // Don't imedialty await the token as we may not need it
    let token = auth(config);

    if regex_is_match!(r"((?i)Launch(?-i)|0)", &input) {
        // Launch the server
        Request::Launch(token.await).send(write_half).await?;
    } else if regex_is_match!(r"((?i)Stop(?-i)|1)", &input) {
        // Stop the server
        Request::Stop(token.await).send(write_half).await?;
    } else if regex_is_match!(r"((?i)Backup(?-i)|2)", &input) {
        // Take backup
        Request::Backup(token.await).send(write_half).await?;
    } else if regex_is_match!(r"((?i)Command(?-i)|3)", &input) {
        // Run Command
        print!("Enter command \n=> ");
        let command = read_input();
        Request::Command(token.await, command)
            .send(write_half)
            .await?;
    } else if regex_is_match!(r"((?i)Download(?-i)|4)", &input) {
        // Download latest backup
        Request::Download(token.await).send(write_half).await?;
    } else {
        // No action recognised
        return Err(Report::msg("Invalid input"));
    };

    match Responce::recive(read_half).await? {
        Responce::Ping => println!("Pinged back!"),
        Responce::Error(e) => println!("Error: {e}"),
    }

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

async fn auth(config: &Config) -> Token {
    todo!()
}
// async fn auth(
//     client: &mut ControllerClient<Channel>,
//     action: AuthAction,
//     config: &Config,
// ) -> Result<Vec<u8>, tonic::Status> {
//     println!("[Awaiting server response...]");
//     let key = client
//         .auth(AuthRequest {
//             action: action.into(),
//         })
//         .await?
//         .into_inner();
//     println!("[Server connection status: {}]", key.comment);
//     Ok(decrypt(&key.key, &config.key).expect("Client side auth error occurred"))
// }

fn read_input() -> String {
    let mut input = String::new();
    let _ = std::io::Write::flush(&mut std::io::stdout());
    std::io::stdin()
        .read_line(&mut input)
        .expect("Could not read input");
    input
}

// async fn recive_world_download(
//     client: &mut ControllerClient<Channel>,
//     config: &Config,
// ) -> Result<(), Box<dyn std::error::Error>> {
//     // Generate file name
//     let ufid = ran_letters(32);
//     let path = format!("worldbackup-[{ufid}].tar.gz",);
//     let token = auth(client, AuthAction::Download, &config).await?;
//     let request = DownloadRequest { token };
//     // Download file
//     let mut stream = client.download(request).await?.into_inner();
//     let mut file = fs::File::create(&path)?;
//     while let Some(msg) = stream.message().await? {
//         println!("{}", msg.comment);

//         if let Some(res) = OpResult::from_i32(msg.result) {
//             if res != OpResult::Success {
//                 // Throw away redundant file to avoid confusion
//                 let _ = std::fs::remove_file(path);
//                 return Ok(());
//             }
//         }

//         file.write(&msg.data)?;
//     }
//     // Download complete, show location
//     let working_directory = std::env::current_dir();
//     match working_directory {
//         Ok(wdir) => println!("Saved as {:?} {}", wdir, path),
//         Err(_) => println!("Download saved as {}", path),
//     }
//     return Ok(());
// }
