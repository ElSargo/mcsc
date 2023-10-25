mod backups;
mod common;
mod net;
mod server_state;
use color_eyre::Result;
use magic_crypt::{new_magic_crypt, MagicCrypt256};
use net::{Request, Responce, Token};
use rand::prelude::*;
use rolling_set::RollingSet;
use server_state::ServerState;
use std::{
    io::{BufRead, Write},
    mem::drop,
    path::Path,
    sync::Arc,
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::RwLock,
};

use crate::{net::ActionRequest, server_state::ServerStates};

///////////////////////////////////////////////////////////////////////////////////////////////////////////////
// Server setup
///////////////////////////////////////////////////////////////////////////////////////////////////////////////

/// Create a server that will allow users to launch, stop a minecraft server as well as download the world file
// #[tokio::main]
#[tokio::main(flavor = "current_thread")] // no need to use many threads as traffic will be very low
async fn main() -> Result<()> {
    let config = config_load();
    let encryptor = Arc::new(new_magic_crypt!(&config.key, 256));
    change_working_directory(&config.minecraft_directory)?;
    println!("Listning...");
    let listner = TcpListener::bind(&config.socket).await.unwrap();
    let server = Arc::new(ServerState {
        state: ServerStates::Idle.into(),
        config,
        tokens: RollingSet::new(2048).into(),
    });
    loop {
        let encryptor = encryptor.clone();
        let server = server.clone();
        let (stream, _socket) = listner.accept().await.unwrap();
        drop(tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, &encryptor, &server).await {
                println!("{e}");
            }
        }));
    }
}

async fn handle_connection(
    mut stream: TcpStream,
    encryptor: &MagicCrypt256,
    server: &ServerState,
) -> Result<()> {
    println!("Recived a request");
    let (read_half, write_half) = stream.split();
    let request = Request::recive(read_half, encryptor).await?;
    match request {
        Request::Ping => {
            println!("Responding to ping");
            let server_state = server.get_state().await;
            match server_state {
                server_state::ServerStateNames::Idle => "Server idle",
                server_state::ServerStateNames::Startup => "Server starting",
                server_state::ServerStateNames::Running => "Server running",
                server_state::ServerStateNames::ShutingDown => "Server shutting-down",
                server_state::ServerStateNames::BackingUp => "Server backing up",
            };
            Responce::Ping.send(write_half, encryptor).await?;
        }
        Request::Auth => {
            println!("Authing");
            Responce::AuthToken(authorize_token(&server.tokens).await)
                .send(write_half, encryptor)
                .await?;
        }
        Request::Action(token, action) => {
            if verify_token(token, &server.tokens).await {
                server.check_stop().await;
                if let Err(e) = match action {
                    ActionRequest::Launch => server.launch().await,
                    ActionRequest::Stop => server.stop().await,
                    ActionRequest::Restart => server.restart().await,
                    ActionRequest::Backup => server.backup().await,
                    ActionRequest::Command(command) => server.run_command(&command).await,
                    ActionRequest::Download => todo!(),
                } {
                    Responce::Error(e.to_string())
                        .send(write_half, encryptor)
                        .await?;
                } else {
                    Responce::Success.send(write_half, encryptor).await?;
                }
            } else {
                Responce::Error("Not verified".to_string())
                    .send(write_half, encryptor)
                    .await?;
            }
        }
    };
    Ok(())
}

/*
async fn download() -> Result<()> {
    let key = req.into_inner().token;
    if !verify_token(Token {
        key,
        action: AuthAction::Download,
    }) {
        return Err(Status::new(tonic::Code::InvalidArgument, "Invalid token"));
    }

    let file = match latest_file(&CONFIG.backup_directory) {
        Some(path) => match File::open(path) {
            Ok(handle) => handle,
            Err(_) => return Err(Status::not_found("No backups")),
        },
        None => return Err(Status::not_found("No backups")),
    };

    // Create iterator that yields WorldDownload
    let wdl = match WorldDownloadIterator::new(file) {
        Some(dl) => dl,
        None => return Err(Status::aborted("Unable to fetch file metadata")),
    };

    let mut stream = Box::pin(tokio_stream::iter(wdl));

    let (send_channel, receive_channel) = mpsc::channel(128);
    tokio::spawn(async move {
        while let Some(item) = stream.next().await {
            match send_channel.send(Result::<_, Status>::Ok(item)).await {
                Ok(_) => {
                    // item (server response) was queued to be send to client
                }
                Err(_item) => {
                    // output_stream was build from receive_channel and both are dropped
                    break;
                }
            }
        }
        println!("\tclient disconnected");
    });

    let output_stream = ReceiverStream::new(receive_channel);
    Ok(Response::new(
        Box::pin(output_stream) as Self::DownloadStream
    ))
}
*/
/// Handle launch request

fn gen_bytes(key_bytes: usize) -> Vec<u8> {
    let mut bytes: Vec<u8> = vec![0; key_bytes];
    thread_rng().fill_bytes(&mut bytes[..]);
    bytes
}

/// Create a new key to give to our client, and store it so it can be verified later
async fn authorize_token(tokens: &RwLock<RollingSet<Token>>) -> Vec<u8> {
    let mut set = tokens.write().await;
    let bytes = gen_bytes(256);
    set.insert(bytes.clone());
    bytes
}

/// Check that a key has been authored by us
async fn verify_token(token: Token, tokens: &RwLock<RollingSet<Token>>) -> bool {
    let mut set = tokens.write().await;
    set.remove(&token)
}

///////////////////////////////////////////////////////////////////////////////////////////////////////////////
// Backup stuff
///////////////////////////////////////////////////////////////////////////////////////////////////////////////

///////////////////////////////////////////////////////////////////////////////////////////////////////////////
// World Download types
///////////////////////////////////////////////////////////////////////////////////////////////////////////////

/// Produces an iterator of WorldDownload for streaming to the client
/*
struct WorldDownloadIterator {
    file_reader: BufReader<File>,
    error: bool,
    read: usize,
    size: usize,
}

impl WorldDownloadIterator {
    fn new(file: File) -> Option<Self> {
        Some(Self {
            size: match file.metadata() {
                Ok(data) => data,
                Err(_) => return None,
            }
            .len() as usize,
            file_reader: BufReader::with_capacity(1024 * 1024, file),
            read: 0,
            error: false,
        })
    }
}

// bruh

impl Iterator for WorldDownloadIterator {
    type Item = Vec<u8>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.error {
            return None;
        }
        let bytes: Vec<u8> = self.file_reader.fill_buf().ok()?;
        self.file_reader.consume(bytes.len());
        self.read += bytes.len();
        let progress = (self.read as f64 / self.size as f64 * 100.) as u64;
        if !bytes.is_empty() {
            Some(bytes)
        } else {
            None
        }
    }
}
*/
///////////////////////////////////////////////////////////////////////////////////////////////////////////////
// Config
///////////////////////////////////////////////////////////////////////////////////////////////////////////////

/// Contains config info
#[derive(serde_derive::Deserialize, Debug)]
pub struct Config {
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
    toml::from_str(config).expect("Unable to parse config, (syntax error)")
}

fn change_working_directory(path: impl AsRef<Path>) -> Result<()> {
    // Change working dir that of .minecraft
    // This is required for java to load the minecraft sever properly
    let current_working_directory =
        std::env::current_dir().expect("Couldn't load current working directory");
    let mut working_directory = current_working_directory;
    working_directory.push(path);
    Ok(std::env::set_current_dir(&working_directory)?)
}
