mod client;
mod common;
mod net;
use crate::{client::Client, net::ActionRequest};
use color_eyre::{Report, Result};
use lazy_regex::regex_is_match;
use net::Token;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new("./mcsc_client.toml")?;
    println!(
        "Welcome to mcsc, NOTE: these operations take time to complete so be patent Enter a command: either by name or the number next to it"
    );

    loop {
        let _ = procces_request(&client).await;
    }
}

async fn procces_request(client: &Client) -> Result<()> {
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
    let action = if regex_is_match!(r"((?i)Launch(?-i)|0)", &input) {
        // Launch the server
        ActionRequest::Launch
    } else if regex_is_match!(r"((?i)Stop(?-i)|1)", &input) {
        // Stop the server
        ActionRequest::Stop
    } else if regex_is_match!(r"((?i)Backup(?-i)|2)", &input) {
        // Take backup
        ActionRequest::Backup
    } else if regex_is_match!(r"((?i)Command(?-i)|3)", &input) {
        // Run Command
        print!("Enter command \n=> ");
        let command = read_input();
        ActionRequest::Command(command)
    } else if regex_is_match!(r"((?i)Download(?-i)|4)", &input) {
        // Download latest backup
        ActionRequest::Download
    } else {
        // No action recognised
        return Err(Report::msg("Invalid input"));
    };
    client.send_request(action).await?;

    todo!();
    Ok(())
}

fn read_input() -> String {
    let mut input = String::new();
    let _ = std::io::Write::flush(&mut std::io::stdout());
    std::io::stdin()
        .read_line(&mut input)
        .expect("Could not read input");
    input
}
