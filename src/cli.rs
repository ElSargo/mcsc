use mcsc::client::{ClientConfig, McscClient, OperationResult};
use std::io::Write;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Welcome to mcsc. Operations may take a while to complete.");

    let config = ClientConfig::load_default()?;
    let client = McscClient::new(config);

    loop {
        if let Err(error) = process_request(&client).await {
            eprintln!("{error}");
        }
    }
}

async fn process_request(client: &McscClient) -> Result<(), Box<dyn std::error::Error>> {
    print!(
        "
0 | Launch   request a server launch
1 | Stop     request a shutdown
2 | Backup   create a backup
3 | Command  run an allowed console command
4 | Download download the latest backup
=> "
    );

    let input = read_input()?;
    match input.trim().to_ascii_lowercase().as_str() {
        "0" | "launch" => print_result(client.launch().await?),
        "1" | "stop" => print_result(client.stop().await?),
        "2" | "backup" => print_result(client.backup().await?),
        "3" | "command" => {
            print!("Enter command\n=> ");
            let command = read_input()?;
            print_result(client.command(command).await?);
        }
        "4" | "download" => {
            let path = client
                .download_latest_backup(std::env::current_dir()?)
                .await?;
            println!("Downloaded backup to {}", path.display());
        }
        _ => println!("Invalid input"),
    }

    Ok(())
}

fn print_result(result: OperationResult) {
    println!("{}: {}", result.label(), result.comment);
}

fn read_input() -> Result<String, std::io::Error> {
    let mut input = String::new();
    std::io::stdout().flush()?;
    std::io::stdin().read_line(&mut input)?;
    Ok(input)
}
