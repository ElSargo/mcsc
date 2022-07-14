pub mod actions {
    tonic::include_proto!("actions");
}

use actions::controller_client::ControllerClient;
use actions::{StartRequest, StopRequest};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Enter \"Start\" to request a startup or \"Stop\" to request a shutdown");

    let mut input = String::new();
    let _ = std::io::Write::flush(&mut std::io::stdout());
    std::io::stdin()
        .read_line(&mut input)
        .expect("Did not enter a correct string");

    let responce;

    match input.as_str() {
        "Start\n" => {
            let mut client = ControllerClient::connect("http://[::1]:50051").await?;
            let request = StartRequest {
                token: "sui".to_owned(),
            };
            responce = client.start(request).await?;
        }

        "Stop\n" => {
            let mut client = ControllerClient::connect("http://[::1]:50051").await?;
            let request = StopRequest {
                token: "sui".to_owned(),
            };
            responce = client.stop(request).await?;
        }

        _ => {
            panic!("Invalid input!");
        }
    }
    let success = responce.into_inner().result;
    match success {
        0 => {
            println!("Great succes!")
        }
        1 => {
            println!("Ahh....  its fucked!")
        }
        2 => {
            println!("No sir!")
        }
        _ => {
            println!("WTF man");
        }
    }

    Ok(())
}
