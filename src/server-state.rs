#[derive(Debug)]
struct ServerState {
    state: RwLock<ServerStates>,
    config: Config,
    tokens: RollingSet<Token>,
}

#[derive(Debug)]
enum ServerStates {
    Idle,
    Startup { procces: Child },
    Running { procces: Child },
    ShutingDown { procces: Child },
    BackingUp,
}

impl ServerState {
        use ServerStates::*;
    async fn backup(&self) -> Result<()> {
        let mut state = self.state.write().await;

        match *state {
            Idle => {
                *state = BackingUp;
                // Compress the file
                Command::new("tar")
                    .arg("-czf")
                    .arg(format!(
                        "{}/{}.tar.gz",
                        &self.config.backup_directory,
                        common::ran_letters(32)
                    ))
                    .arg("world")
                    .spawn()?
                    .wait()
                    .await?;
                // remove_oldest_backup("minecraft/backups");
                {
                    let mut num_backups = std::fs::read_dir(&self.config.backup_directory)
                        .into_iter()
                        .flatten()
                        .count();
                    while num_backups > 10 {
                        num_backups -= 1;
                        remove_oldest_backup(&self.config.backup_directory);
                    }
                }
                *state = Idle;
                Ok(())
            }
            Running { procces: _ } | Startup { procces: _ } | ShutingDown { procces: _ } => {
                Err(Report::msg("Server still running"))
            }
            BackingUp => Err(Report::msg("Another backup is in progress")),
        }
    }

    fn check_stop(&mut self) {
        if let Running { procces: c } = self {
            let res = c.try_wait();
            if let Ok(possible_exit_code) = res {
                if let Some(_exit_code) = possible_exit_code {
                    //Procces finished
                    *self = Idle;
                }
            }
        }
    }

    async fn run_command(&mut self, cmd: &str) -> Result<()> {
        match self {
            Running { procces } => {
                let pstdin = procces.stdin.as_mut();
                match pstdin {
                    Some(buff) => {
                        buff.write_all(&format!("\n{}\n", cmd).into_bytes()).await?;
                        Ok(())
                    }
                    None => Err(CommandError::ProccesError),
                }
            }
            Idle | BackingUp | Startup { procces: _ } | ShutingDown { procces: _ } => {
                Err(Report::msg("Server not ready to recive commands"))
            }
        }
    }

    /// Spawn a new java procces and store it in MINECRAFT_SERVER_STATE
    fn launch(&mut self) -> Result<(), LaunchError> {
        self.check_stop();
        match self {
            Idle => {
                let child = match Command::new("sh")
                    .stdin(Stdio::piped())
                    // .stdout(Stdio::piped())
                    .arg("launch.sh")
                    .spawn()
                {
                    Ok(child) => child,
                    Err(_c) => return Err(LaunchError::Launch),
                };
                *self = Running { procces: child };
                Ok(())
            }
            BackingUp => Err(LaunchError::Downloading),
            Running { procces: _ } => Err(LaunchError::AlreadyRunning),
        }
    }

    /// Stop the running procces by entering stop into the stdin
    async fn stop(&mut self) -> Result<()> {
        self.check_stop();
        match self {
            Running { procces: child }
            | ShutingDown { procces: child }
            | Startup { procces: child } => {
                let child_input = child.stdin.as_mut();
                if let Some(buff) = child_input {
                    buff.write_all(b"\nstop\n").await?;
                    child.wait().await;
                    *self = Idle;
                    Ok(())
                } else {
                    Err(Report::msg("Child has no stdin"))
                }
            }
            BackingUp | Idle => Ok(()),
        }
    }
}

