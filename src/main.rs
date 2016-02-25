extern crate time;

use std::str;
use std::io::{Read, Write, ErrorKind};
use std::time::Duration;
use std::net::{TcpListener, TcpStream};
use std::thread;

fn handle_client(mut stream: TcpStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut data = [0; 4];
    match stream.read(&mut data) {
        Ok(_) => {
            if str::from_utf8(&data).unwrap() != "GET " {
                return;
            }
        }
        Err(e) => {
            match e.kind() {
                ErrorKind::TimedOut | ErrorKind::WouldBlock => return,
                _ => panic!("{}", e),
            }
        }
    }

    let body = "Hello, World!";
    let date = time::strftime("%a, %d %b %Y %H:%M:%S %Z", &time::now()).unwrap();
    let message = format!("HTTP/1.1 200 OK\n\r\
                           Date: {}\r\n\
                           Server: Rust Serv/0.1\r\n\
                           Content-Type: text/plain\r\n\
                           Content-Length: {}\r\n\
                           Connection: close\r\n\
                           \r\n{}",
                          date,
                          body.len(),
                          body);

    let _ = stream.write(message.as_bytes());
}

fn main() {
    let listener = TcpListener::bind("127.0.0.1:8000").unwrap();
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                thread::spawn(move || handle_client(stream));
            }
            Err(e) => {
                println!("Connection failed! {}", e);
            }
        }
    }
}
