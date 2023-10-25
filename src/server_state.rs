use crate::{backups::remove_oldest_backup, common, net::Token, Config};
use color_eyre::{Report, Result};
use rolling_set::RollingSet;
use std::process::Stdio;
use strum::EnumDiscriminants;
use tokio::io::AsyncWriteExt;
use tokio::process::ChildStdin;
use tokio::{
    process::{Child, Command},
    sync::RwLock,
};
use ServerStates::*;

#[derive(Debug)]
pub struct ServerState {
    pub state: RwLock<ServerStates>,
    pub config: Config,
    pub tokens: RwLock<RollingSet<Token>>,
}

#[derive(Debug, EnumDiscriminants)]
#[strum_discriminants(name(ServerStateNames))]
pub enum ServerStates {
    Idle,
    Startup { procces: Child },
    Running { procces: Child },
    ShutingDown { procces: Child },
    BackingUp,
}

impl ServerState {
    pub async fn backup(&self) -> Result<()> {
        let mut state = self.state.write().await;

        match *state {
            Idle => {
                *state = BackingUp;
                // Compress the file
                let res = do_backup(&self.config.backup_directory).await;
                *state = Idle;
                res?;
                Ok(())
            }
            Running { procces: _ } | Startup { procces: _ } | ShutingDown { procces: _ } => {
                Err(Report::msg("Server still running"))
            }
            BackingUp => Err(Report::msg("Another backup is in progress")),
        }
    }

    pub async fn check_stop(&self) {
        let mut state = self.state.write().await;
        if let Running { procces: c } = &mut *state {
            let res = c.try_wait();
            if let Ok(possible_exit_code) = res {
                if let Some(_exit_code) = possible_exit_code {
                    //Procces finised
                    *state = Idle;
                }
            }
        }
    }

    pub async fn run_command(&self, cmd: &str) -> Result<()> {
        let mut state = self.state.write().await;
        match &mut *state {
            Running { procces } => {
                let pstdin = get_stdin(procces)?;
                Ok(())
            }
            Idle | BackingUp | Startup { procces: _ } | ShutingDown { procces: _ } => {
                Err(Report::msg("Server not ready to recive commands"))
            }
        }
    }

    pub async fn launch(&self) -> Result<()> {
        let mut state = self.state.write().await;
        match &mut *state {
            Idle => {
                let child = Command::new("sh")
                    .stdin(Stdio::piped())
                    .arg("launch.sh")
                    .spawn()?;
                *state = Running { procces: child };
                Ok(())
            }
            BackingUp => Err(Report::msg("Server performing backup")),
            ShutingDown { procces: _ } => Err(Report::msg("Server shuting down")),
            Running { procces: _ } | Startup { procces: _ } => Ok(()),
        }
    }

    /// Stop the running procces by entering stop into the stdin
    pub async fn stop(&self) -> Result<()> {
        let mut state = self.state.write().await;
        match &mut *state {
            Running { procces: child }
            | ShutingDown { procces: child }
            | Startup { procces: child } => {
                let child_input = get_stdin(child)?;
                child_input.write_all(b"\nstop\n").await?;
                child.wait().await.ignore();
                *state = Idle;
                Ok(())
            }
            BackingUp | Idle => Ok(()),
        }
    }

    pub async fn get_state(&self) -> ServerStateNames {
        ServerStateNames::from(&*self.state.read().await)
    }

    pub async fn restart(&self) -> Result<()> {
        self.stop().await?;
        self.launch().await
    }
}

fn get_stdin(child: &mut Child) -> Result<&mut ChildStdin> {
    child
        .stdin
        .as_mut()
        .ok_or(Report::msg("Unable to get childs stdin"))
}

async fn do_backup(backup_directory: &str) -> Result<(), Report> {
    Command::new("tar")
        .arg("-czf")
        .arg(format!(
            "{}/{}.tar.gz",
            backup_directory,
            common::ran_letters(32) // TODO use checksum
        ))
        .arg("world")
        .spawn()?
        .wait()
        .await?;
    let num_backups = std::fs::read_dir(backup_directory)
        .into_iter()
        .flatten()
        .count();
    for _ in 10..num_backups {
        remove_oldest_backup(backup_directory).await.ignore();
    }
    Ok(())
}

trait Ignore {
    fn ignore(&self) {}
}

impl<G, E> Ignore for std::result::Result<G, E> {
    fn ignore(&self) {}
}
