mod net;
use crate::net::{Request, Responce};
use color_eyre::Result;
use magic_crypt::{new_magic_crypt, MagicCrypt256};
use std::{mem::drop, sync::Arc};
use tokio::net::{TcpListener, TcpStream};

#[tokio::main]
async fn main() {
    println!("The server tester");
    println!("Listning...");
    let config = config_load();
    let encryptor = Arc::new(new_magic_crypt!(config.key, 256));
    let listner = TcpListener::bind("127.0.0.1:7878").await.unwrap();
    loop {
        let encryptor = encryptor.clone();
        let (stream, _socket) = listner.accept().await.unwrap();
        drop(tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, &encryptor).await {
                println!("{e}");
            }
        }));
    }
}

async fn handle_connection(mut stream: TcpStream, encryptor: &MagicCrypt256) -> Result<()> {
    println!("Recived a request");
    let (read_half, write_half) = stream.split();
    let request = Request::recive(read_half, encryptor).await?;
    match request {
        Request::Ping => {
            println!("Responding to ping");
            Responce::Ping.send(write_half, encryptor).await?;
        }
        Request::Auth => {
            println!("Authing");
            Responce::AuthToken(vec![69; 3])
                .send(write_half, encryptor)
                .await?;
        }
        Request::Action(token, action) => {}
    };
    Ok(())
}

#[derive(serde_derive::Deserialize, Debug)]
struct Config {
    /// Working directory of minecraft server
    minecraft_directory: String,
    /// Where to store backups relative to minecraft dir
    backup_directory: String,
    /// Clients need to have this to authenticate their actions
    key: String,
    /// Service runs from this socket
    socket: String,
}

/// Load the config file and parse it into a convenient data structure
///
/// Panics if the config file couldn't be loaded or parsed
///
fn config_load() -> Config {
    let bytes = std::fs::read("mcsc_server.toml").expect("Unable to load config file");
    let config = std::str::from_utf8(&bytes).expect("Config file encoding error");
    toml::from_str(&config).expect("Unable to parse config, (syntax error)")
}
