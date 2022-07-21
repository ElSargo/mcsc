pub mod actions {
    tonic::include_proto!("actions");
}

use actions::controller_client::ControllerClient;
use actions::{AuthRequest, DownloadRequest, OpResponce, StartRequest, StopRequest};
use security::decrypt;
use std::fs;
use std::thread::sleep;
use std::time::Duration;
use tonic::Response;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Enter a command: \n\"Start\" to request a startup or \n\"Stop\" to request a shutdown or \n\"Download\" to download the world file");

    let mut input = String::new();
    let _ = std::io::Write::flush(&mut std::io::stdout());
    std::io::stdin()
        .read_line(&mut input)
        .expect("Did not enter a correct string");

    let responce: Response<OpResponce>;

    match input.as_str() {
        "Start\n" => {
            let mut client = ControllerClient::connect("http://[::1]:50051").await?;
            let key = client.auth(AuthRequest {}).await?.into_inner();
            let request = StartRequest {
                token: decrypt(key.data).expect("Client side auth error occured"),
            };
            responce = client.start(request).await?;
        }

        "Stop\n" => {
            let mut client = ControllerClient::connect("http://[::1]:50051").await?;
            let key = client.auth(AuthRequest {}).await?.into_inner();
            let request = StopRequest {
                token: decrypt(key.data).expect("Client side auth error occured"),
            };
            responce = client.stop(request).await?;
        }

        "Download\n" => {
            let mut client = ControllerClient::connect(security::ADDRES).await?;
            let key = client.auth(AuthRequest {}).await?.into_inner();
            let request = DownloadRequest {
                token: decrypt(key.data).expect("Client side auth error occured"),
            };
            let file = client.download(request).await?.into_inner().data;
            fs::write("worldbackup.tar.gz", file)?;
            let working_directory = std::env::current_dir();
            match working_directory {
                Ok(wdir) => println!("Saved as {:?} worldbackup.tar.gz", wdir),
                Err(_) => println!("Download saved as worldbackup.tar.gz"),
            }
            return Ok(());
        }

        _ => {
            println!("Invalid input");
            sleep(Duration::from_secs(1));
            panic!("");
        }
    }
    let success = responce.into_inner();
    match success.result {
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
    println!("{}", success.comment);

    sleep(Duration::from_secs(1));
    Ok(())
}

mod security {
    use magic_crypt::{new_magic_crypt, MagicCrypt256, MagicCryptTrait};
    pub const ADDRES: &str = "http://[::1]:50051";
    static mut CRYPT: Option<MagicCrypt256> = None;
    pub fn init() {
        unsafe {
            CRYPT = Some(new_magic_crypt!("Who was in paris?.....", 256));
        }
    }

    pub fn decrypt(data: Vec<u8>) -> Result<Vec<u8>, magic_crypt::MagicCryptError> {
        unsafe {
            match &CRYPT {
                Some(key) => {
                    return Ok(key.decrypt_bytes_to_bytes(&data)?);
                }
                None => {
                    init();
                    return decrypt(data);
                }
            }
        }
    }
}
