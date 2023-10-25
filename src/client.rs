use crate::net::{ActionRequest, Responce};
use crate::{net::Request, Token};
use color_eyre::{Report, Result};
use serde_derive::Deserialize;
use std::{fs, io::Write};
use std::{net::SocketAddr, path::Path};
use tokio::net::{TcpSocket, TcpStream};
// use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use magic_crypt::{new_magic_crypt, MagicCrypt256, MagicCryptTrait};

pub struct Client {
    host: SocketAddr,
    enctyptor: MagicCrypt256,
}

impl Client {
    pub fn new(config_path: impl AsRef<Path>) -> Result<Self> {
        let config = Config::new(config_path)?;
        Ok(Self {
            host: config.ip.parse()?,
            enctyptor: new_magic_crypt!(config.key, 256),
        })
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
    async fn auth(&self) -> Result<Token> {
        let mut stream = connect(&self.host).await?;
        let (read_half, write_half) = stream.split();
        Request::Auth.send(write_half, &self.enctyptor).await?;
        match Responce::recive(read_half, &self.enctyptor).await? {
            Responce::AuthToken(token) => Ok(token),
            Responce::Error(msg) => Err(Report::msg(msg)),
            _ => Err(Report::msg("Server returned somthing weird")),
        }
    }

    pub async fn send_request(&self, action: ActionRequest) -> Result<()> {
        let token = self.auth().await?;
        let mut stream = connect(&self.host).await?;
        Request::Action(token, action)
            .send(stream.split().1, &self.enctyptor)
            .await?;
        Ok(())
    }
}

#[derive(Deserialize, Debug)]
struct Config {
    ip: String,
    key: String,
}

impl Config {
    fn new(path: impl AsRef<Path>) -> Result<Config> {
        let bytes = std::fs::read(path)?;
        let text = std::str::from_utf8(&bytes)?;
        Ok(toml::from_str(text)?)
    }
}

async fn connect(addy: &SocketAddr) -> Result<TcpStream, Report> {
    let socket = TcpSocket::new_v4()?;
    Ok(socket.connect(*addy).await?)
}

fn decrypt(data: &Vec<u8>, key: &str) -> Result<Vec<u8>, magic_crypt::MagicCryptError> {
    let key = new_magic_crypt!(key, 256);
    key.decrypt_bytes_to_bytes(data)
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
