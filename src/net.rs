use color_eyre::Result;
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
    Launch(Token),
    Stop(Token),
    Restart(Token),
    Backup(Token),
    Command(Token, String),
    Download(Token),
}

// Sent from server to client
#[derive(Serialize, Deserialize, Debug)]
pub enum Responce {
    Ping,
    Error(String),
}

impl Request {
    pub fn into_bytes(self) -> Result<Vec<u8>> {
        Ok(bincode::serialize(&self)?)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Ok(bincode::deserialize(bytes)?)
    }

    pub async fn recive(read_stream: ReadHalf<'_>) -> Result<Self> {
        let data = recive_bytes(read_stream).await?;
        Self::from_bytes(&data)
    }

    pub async fn send(self, write_stream: WriteHalf<'_>) -> Result<()> {
        let bytes = self.into_bytes()?;
        send_bytes(&bytes, write_stream).await
    }
}

impl Responce {
    pub fn into_bytes(self) -> Result<Vec<u8>> {
        Ok(bincode::serialize(&self)?)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Ok(bincode::deserialize(bytes)?)
    }

    pub async fn recive(read_stream: ReadHalf<'_>) -> Result<Self> {
        let data = recive_bytes(read_stream).await?;
        Self::from_bytes(&data)
    }

    pub async fn send(self, write_stream: WriteHalf<'_>) -> Result<()> {
        let bytes = self.into_bytes()?;
        send_bytes(&bytes, write_stream).await
    }
}

async fn recive_bytes(stream: ReadHalf<'_>) -> Result<Vec<u8>, color_eyre::Report> {
    let mut reader = BufReader::new(stream);
    let mut data = vec![];
    reader.read_to_end(&mut data).await?;
    Ok(data)
}

async fn send_bytes(bytes: &[u8], mut wire: WriteHalf<'_>) -> Result<()> {
    wire.write_all(bytes).await?;
    wire.flush().await?;
    wire.shutdown().await?;
    Ok(())
}
