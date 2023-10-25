mod net;
use crate::net::{Request, Responce};
use tokio::net::TcpSocket;

#[tokio::main]
async fn main() {
    println!("The client tester");

    // Get a socekt
    let addy = "127.0.0.1:7878".parse().unwrap();
    let socket = TcpSocket::new_v4().unwrap();
    let mut stream = socket.connect(addy).await.unwrap();

    // Ping the server
    let (read_half, write_half) = stream.split();
    Request::Ping.send(write_half).await.unwrap();
    match Responce::recive(read_half).await.unwrap() {
        Responce::Ping => {
            println!("Got a ping back!");
        }
        Responce::Error(msg) => {
            println!("Got an error! \n{msg}");
        }
    }
}
