use crate::actions::{
    controller_client::ControllerClient, AuthAction, BackupRequest, CommandRequest,
    DownloadRequest, LaunchRequest, OpResponse, OpResult, StopRequest,
};
use crate::common;
use serde::Deserialize;
use std::io::Write;
use std::path::{Path, PathBuf};
use tonic::transport::Channel;

#[derive(Clone, Debug, Deserialize)]
pub struct ClientConfig {
    pub ip: String,
    pub key: String,
}

impl ClientConfig {
    pub fn load_default() -> Result<Self, ClientError> {
        Self::load_from_path("mcsc_client.toml")
    }

    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self, ClientError> {
        let bytes = std::fs::read(path)?;
        let text = std::str::from_utf8(&bytes)?;
        Ok(toml::from_str(text)?)
    }
}

#[derive(Clone, Debug)]
pub struct McscClient {
    config: ClientConfig,
}

impl McscClient {
    pub fn new(config: ClientConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    pub async fn launch(&self) -> Result<OperationResult, ClientError> {
        let mut client = self.connect().await?;
        let response = client
            .launch(LaunchRequest {
                auth: Some(self.auth(AuthAction::Launch, &[])?),
            })
            .await?
            .into_inner();

        Ok(response.into())
    }

    pub async fn stop(&self) -> Result<OperationResult, ClientError> {
        let mut client = self.connect().await?;
        let response = client
            .stop(StopRequest {
                auth: Some(self.auth(AuthAction::Stop, &[])?),
            })
            .await?
            .into_inner();

        Ok(response.into())
    }

    pub async fn backup(&self) -> Result<OperationResult, ClientError> {
        let mut client = self.connect().await?;
        let response = client
            .backup(BackupRequest {
                auth: Some(self.auth(AuthAction::Backup, &[])?),
            })
            .await?
            .into_inner();

        Ok(response.into())
    }

    pub async fn command(&self, command: String) -> Result<OperationResult, ClientError> {
        let command = command.trim().to_owned();
        if command.is_empty() {
            return Err(ClientError::InvalidInput(
                "command cannot be empty".to_owned(),
            ));
        }

        let mut client = self.connect().await?;
        let response = client
            .command(CommandRequest {
                auth: Some(self.auth(AuthAction::Command, command.as_bytes())?),
                command,
            })
            .await?
            .into_inner();

        Ok(response.into())
    }

    pub async fn download_latest_backup(&self, directory: PathBuf) -> Result<PathBuf, ClientError> {
        let mut client = self.connect().await?;
        let request = DownloadRequest {
            auth: Some(self.auth(AuthAction::Download, &[])?),
        };
        let mut stream = client.download(request).await?.into_inner();

        std::fs::create_dir_all(&directory)?;
        let file_name = format!("worldbackup-{}.tar.gz", common::random_letters(32)?);
        let path = directory.join(file_name);
        let mut file = std::fs::File::create(&path)?;

        while let Some(msg) = stream.message().await? {
            let result = op_result(msg.result);
            if result != OpResult::Success {
                let _ = std::fs::remove_file(&path);
                return Err(ClientError::Server(msg.comment));
            }

            file.write_all(&msg.data)?;
        }

        Ok(path)
    }

    async fn connect(&self) -> Result<ControllerClient<Channel>, ClientError> {
        Ok(ControllerClient::connect(self.config.ip.clone()).await?)
    }

    fn auth(
        &self,
        action: AuthAction,
        body: &[u8],
    ) -> Result<crate::actions::AuthToken, ClientError> {
        Ok(common::signed_token(&self.config.key, action, body)?)
    }
}

#[derive(Clone, Debug)]
pub struct OperationResult {
    pub result: OpResult,
    pub comment: String,
}

impl OperationResult {
    pub fn is_success(&self) -> bool {
        self.result == OpResult::Success
    }

    pub fn label(&self) -> &'static str {
        match self.result {
            OpResult::Success => "Success",
            OpResult::Fail => "Failed",
            OpResult::Denied => "Denied",
        }
    }
}

impl From<OpResponse> for OperationResult {
    fn from(response: OpResponse) -> Self {
        Self {
            result: op_result(response.result),
            comment: response.comment,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("config file is not valid UTF-8: {0}")]
    ConfigEncoding(#[from] std::str::Utf8Error),
    #[error("config file is not valid TOML: {0}")]
    Config(#[from] toml::de::Error),
    #[error("could not connect to server: {0}")]
    Transport(#[from] tonic::transport::Error),
    #[error("server returned a gRPC error: {0}")]
    Status(#[from] tonic::Status),
    #[error("auth error: {0}")]
    Auth(#[from] common::AuthError),
    #[error("unable to get secure random bytes: {0}")]
    Random(#[from] getrandom::Error),
    #[error("server rejected download: {0}")]
    Server(String),
    #[error("{0}")]
    InvalidInput(String),
}

fn op_result(result: i32) -> OpResult {
    OpResult::try_from(result).unwrap_or(OpResult::Fail)
}
