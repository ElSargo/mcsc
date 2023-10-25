#![allow(dead_code)]
use color_eyre::Result;
use magic_crypt::{MagicCrypt256, MagicCryptTrait};
use serde_derive::{Deserialize, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufReader},
    net::tcp::{ReadHalf, WriteHalf},
};

pub type Token = Vec<u8>;

// Sent from client to server
#[derive(Serialize, Deserialize, Debug)]
pub enum Request {
    Ping,
    Auth,
    Action(Token, ActionRequest),
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ActionRequest {
    Launch,
    Stop,
    Restart,
    Backup,
    Command(String),
    Download,
}

// Sent from server to client
#[derive(Serialize, Deserialize, Debug)]
pub enum Responce {
    Ping,
    AuthToken(Vec<u8>),
    Error(String),
    Success,
}

impl Request {
    pub fn into_bytes(self) -> Result<Vec<u8>> {
        Ok(bincode::serialize(&self)?)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Ok(bincode::deserialize(bytes)?)
    }

    pub async fn recive(read_stream: ReadHalf<'_>, encryptor: &MagicCrypt256) -> Result<Self> {
        let data = recive_bytes(read_stream, encryptor).await?;
        Self::from_bytes(&data)
    }

    pub async fn send(self, write_stream: WriteHalf<'_>, encryptor: &MagicCrypt256) -> Result<()> {
        let bytes = self.into_bytes()?;
        send_bytes(&bytes, write_stream, encryptor).await
    }
}

impl Responce {
    pub fn into_bytes(self) -> Result<Vec<u8>> {
        Ok(bincode::serialize(&self)?)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Ok(bincode::deserialize(bytes)?)
    }

    pub async fn recive(read_stream: ReadHalf<'_>, encryptor: &MagicCrypt256) -> Result<Self> {
        let data = recive_bytes(read_stream, encryptor).await?;
        Self::from_bytes(&data)
    }

    pub async fn send(self, write_stream: WriteHalf<'_>, encryptor: &MagicCrypt256) -> Result<()> {
        let bytes = self.into_bytes()?;
        send_bytes(&bytes, write_stream, encryptor).await
    }
}

async fn recive_bytes(
    stream: ReadHalf<'_>,
    encryptor: &MagicCrypt256,
) -> Result<Vec<u8>, color_eyre::Report> {
    let mut reader = BufReader::new(stream);
    let mut data = vec![];
    reader.read_to_end(&mut data).await?;
    Ok(encryptor.decrypt_bytes_to_bytes(&data)?)
}

async fn send_bytes(
    bytes: &[u8],
    mut wire: WriteHalf<'_>,
    encryptor: &MagicCrypt256,
) -> Result<()> {
    wire.write_all(&encryptor.encrypt_to_bytes(bytes)).await?;
    wire.flush().await?;
    wire.shutdown().await?;
    Ok(())
}
