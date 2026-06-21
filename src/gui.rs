use iced::widget::{button, column, container, row, scrollable, text, text_input, Button};
use iced::{Element, Fill, Size, Task, Theme};
use mcsc::client::{ClientConfig, McscClient, OperationResult};
use std::path::PathBuf;

pub fn main() -> iced::Result {
    iced::application(McscGui::new, update, view)
        .title("MCSC")
        .theme(Theme::TokyoNight)
        .window(iced::window::Settings {
            size: Size::new(760.0, 560.0),
            min_size: Some(Size::new(640.0, 460.0)),
            ..Default::default()
        })
        .centered()
        .run()
}

#[derive(Debug)]
struct McscGui {
    endpoint: String,
    key: String,
    command: String,
    busy: bool,
    log: Vec<String>,
}

impl McscGui {
    fn new() -> Self {
        match ClientConfig::load_default() {
            Ok(config) => Self {
                endpoint: config.ip,
                key: config.key,
                command: String::new(),
                busy: false,
                log: vec!["Loaded mcsc_client.toml".to_owned()],
            },
            Err(error) => Self {
                endpoint: "http://127.0.0.1:7878".to_owned(),
                key: String::new(),
                command: String::new(),
                busy: false,
                log: vec![
                    "Using default endpoint because mcsc_client.toml could not be loaded"
                        .to_owned(),
                    error.to_string(),
                ],
            },
        }
    }

    fn client(&self) -> Result<McscClient, String> {
        let endpoint = self.endpoint.trim().to_owned();
        if endpoint.is_empty() {
            return Err("endpoint cannot be empty".to_owned());
        }

        if self.key.len() < 32 {
            return Err("secret must be at least 32 characters".to_owned());
        }

        Ok(McscClient::new(ClientConfig {
            ip: endpoint,
            key: self.key.clone(),
        }))
    }

    fn push_log(&mut self, line: impl Into<String>) {
        self.log.push(line.into());
        if self.log.len() > 100 {
            self.log.remove(0);
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
    EndpointChanged(String),
    KeyChanged(String),
    CommandChanged(String),
    Run(OperationKind),
    Finished(Result<ActionResult, String>),
}

#[derive(Debug, Clone, Copy)]
enum OperationKind {
    Launch,
    Stop,
    Backup,
    Command,
    Download,
}

impl OperationKind {
    fn label(self) -> &'static str {
        match self {
            OperationKind::Launch => "Launch",
            OperationKind::Stop => "Stop",
            OperationKind::Backup => "Backup",
            OperationKind::Command => "Command",
            OperationKind::Download => "Download",
        }
    }
}

#[derive(Debug, Clone)]
enum ActionResult {
    Operation(OperationKind, OperationResult),
    Downloaded(PathBuf),
}

fn update(state: &mut McscGui, message: Message) -> Task<Message> {
    match message {
        Message::EndpointChanged(endpoint) => {
            state.endpoint = endpoint;
            Task::none()
        }
        Message::KeyChanged(key) => {
            state.key = key;
            Task::none()
        }
        Message::CommandChanged(command) => {
            state.command = command;
            Task::none()
        }
        Message::Run(kind) => start_action(state, kind),
        Message::Finished(result) => {
            state.busy = false;
            match result {
                Ok(ActionResult::Operation(kind, result)) => {
                    state.push_log(format!(
                        "{}: {} - {}",
                        kind.label(),
                        result.label(),
                        result.comment
                    ));
                }
                Ok(ActionResult::Downloaded(path)) => {
                    state.push_log(format!("Download saved to {}", path.display()));
                }
                Err(error) => {
                    state.push_log(format!("Error: {error}"));
                }
            }
            Task::none()
        }
    }
}

fn start_action(state: &mut McscGui, kind: OperationKind) -> Task<Message> {
    if state.busy {
        return Task::none();
    }

    let client = match state.client() {
        Ok(client) => client,
        Err(error) => {
            state.push_log(format!("Error: {error}"));
            return Task::none();
        }
    };

    let command = state.command.clone();
    state.busy = true;
    state.push_log(format!("Starting {}", kind.label()));

    Task::perform(
        async move {
            match kind {
                OperationKind::Launch => client
                    .launch()
                    .await
                    .map(|result| ActionResult::Operation(kind, result))
                    .map_err(|error| error.to_string()),
                OperationKind::Stop => client
                    .stop()
                    .await
                    .map(|result| ActionResult::Operation(kind, result))
                    .map_err(|error| error.to_string()),
                OperationKind::Backup => client
                    .backup()
                    .await
                    .map(|result| ActionResult::Operation(kind, result))
                    .map_err(|error| error.to_string()),
                OperationKind::Command => client
                    .command(command)
                    .await
                    .map(|result| ActionResult::Operation(kind, result))
                    .map_err(|error| error.to_string()),
                OperationKind::Download => match std::env::current_dir() {
                    Ok(directory) => client
                        .download_latest_backup(directory)
                        .await
                        .map(ActionResult::Downloaded)
                        .map_err(|error| error.to_string()),
                    Err(error) => Err(error.to_string()),
                },
            }
        },
        Message::Finished,
    )
}

fn view(state: &McscGui) -> Element<'_, Message> {
    let endpoint_input = text_input("http://127.0.0.1:7878", &state.endpoint)
        .on_input(Message::EndpointChanged)
        .padding(10)
        .width(Fill);

    let key_input = text_input("shared secret", &state.key)
        .on_input(Message::KeyChanged)
        .secure(true)
        .padding(10)
        .width(Fill);

    let command_input = text_input("say Server restart in five minutes", &state.command)
        .on_input(Message::CommandChanged)
        .on_submit(Message::Run(OperationKind::Command))
        .padding(10)
        .width(Fill);

    let action_row = row![
        action_button("Launch", OperationKind::Launch, state.busy),
        action_button("Stop", OperationKind::Stop, state.busy),
        action_button("Backup", OperationKind::Backup, state.busy),
        action_button("Download", OperationKind::Download, state.busy),
    ]
    .spacing(8);

    let command_row = row![
        command_input,
        action_button("Send", OperationKind::Command, state.busy),
    ]
    .spacing(8);

    let log = state.log.iter().fold(column![].spacing(6), |log, line| {
        log.push(text(line).size(14))
    });

    let body = column![
        text("MCSC").size(28),
        text("Minecraft server control").size(15),
        text("Endpoint").size(14),
        endpoint_input,
        text("Shared secret").size(14),
        key_input,
        action_row,
        text("Console command").size(14),
        command_row,
        text(if state.busy { "Busy" } else { "Ready" }).size(14),
        scrollable(log).height(Fill),
    ]
    .spacing(12)
    .width(Fill);

    container(body).padding(20).width(Fill).height(Fill).into()
}

fn action_button(label: &'static str, kind: OperationKind, busy: bool) -> Button<'static, Message> {
    button(text(label))
        .padding([8, 12])
        .on_press_maybe((!busy).then_some(Message::Run(kind)))
}
