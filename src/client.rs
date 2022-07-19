pub mod actions {
    tonic::include_proto!("actions");
}

use actions::controller_client::ControllerClient;
use actions::{DownloadRequest, StartRequest, StopRequest};
use security::encrypt;
use std::fs;
use std::thread::sleep;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Enter a command: \n\"Start\" to request a startup or \n\"Stop\" to request a shutdown or \n\"Download\" to download the world file");

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
                token: encrypt("Thats cauze all my niggas in:".to_owned()),
            };
            responce = client.start(request).await?;
        }

        "Stop\n" => {
            let mut client = ControllerClient::connect("http://[::1]:50051").await?;
            let request = StopRequest {
                token: encrypt("Thats cauze all my niggas in:".to_owned()),
            };
            responce = client.stop(request).await?;
        }

        "Download\n" => {
            let mut client = ControllerClient::connect("http://[::1]:50051").await?;
            let request = DownloadRequest {
                token: encrypt("Thats cauze all my niggas in:".to_owned()),
            };
            let file = client.download(request).await?.into_inner().data;
            fs::write("worldbackup.tar.gz", file)?;
            return Ok(());
        }

        _ => {
            println!("Invalid input");
            sleep(Duration::from_secs(1));
            panic!("");
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

    sleep(Duration::from_secs(1));
    Ok(())
}

mod security {
    use magic_crypt::{new_magic_crypt, MagicCrypt256, MagicCryptTrait};
    static mut CRYPT: Option<MagicCrypt256> = None;
    pub fn init() {
        unsafe {
            CRYPT = Some(new_magic_crypt!("Who was in paris?.....", 256));
        }
    }

    pub fn encrypt(txt: String) -> String {
        unsafe {
            match &CRYPT {
                Some(key) => {
                    return key.encrypt_str_to_base64(txt);
                }
                None => {
                    init();
                    return encrypt(txt);
                }
            }
        }
    }
}
