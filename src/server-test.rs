mod net;
use crate::net::{Request, Responce};
use color_eyre::Result;
use std::mem::drop;
use tokio::net::{TcpListener, TcpStream};

#[tokio::main]
async fn main() {
    println!("The server tester");
    println!("Listning...");
    let listner = TcpListener::bind("127.0.0.1:7878").await.unwrap();
    loop {
        let (stream, _socket) = listner.accept().await.unwrap();
        drop(tokio::spawn(async move {
            if let Err(e) = handle_connection(stream).await {
                println!("{e}");
            }
        }));
    }
}

async fn handle_connection(mut stream: TcpStream) -> Result<()> {
    println!("Recived a request");
    let (read_half, write_half) = stream.split();
    let request = Request::recive(read_half).await?;
    match request {
        Request::Ping => {
            println!("Responding to ping");
            Responce::Ping.send(write_half).await?;
        }
        Request::Launch(_) => todo!(),
        Request::Stop(_) => todo!(),
        Request::Restart(_) => todo!(),
        Request::Backup(_) => todo!(),
        Request::Command(_, _) => todo!(),
        Request::Download(_) => todo!(),
    };
    Ok(())
}
