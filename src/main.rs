extern crate time;

use std::str;
use std::io::{Read, Write, ErrorKind};
use std::time::Duration;
use std::net::{TcpListener, TcpStream};
use std::thread;

fn current_time_string() -> String {
    time::strftime("%a, %d %b %Y %H:%M:%S %Z", &time::now()).unwrap()
}

fn head(stream: &mut TcpStream, body: &[u8]) {
    let message = format!("HTTP/1.1 200 OK\r\n\
                           Date: {}\r\n\
                           Connection: close\r\n\
                           Server: Rust Serv/0.1\r\n\
                           Content-Type: text/plain\r\n\
                           Content-Length: {}\r\n\
                           \r\n",
                          current_time_string(),
                          body.len());
    let _ = stream.write(message.as_bytes());
}

fn not_allowed(stream: &mut TcpStream) {
    let message = format!("HTTP/1.1 405 Method Not Allowed\r\n\
                           Date: {}\r\n\
                           Connection: close\r\n\
                           Server: Rust Serv/0.1\r\n\
                           Allow: GET, HEAD\r\n\
                           Content-Length: 0\r\n\
                           \r\n",
                          current_time_string());
    let _ = stream.write(message.as_bytes());
}

fn handle_client(mut stream: TcpStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut data = [0; 4];
    match stream.read(&mut data) {
        Ok(_) => {
            let body = "Hello, World!";
            match str::from_utf8(&data) {
                Ok("HEAD") => {
                    head(&mut stream, body.as_bytes());
                }
                Ok("GET ") => {
                    head(&mut stream, body.as_bytes());
                    let _ = stream.write(body.as_bytes());
                }
                Ok(_) => {
                    not_allowed(&mut stream);
                }
                _ => return,
            }
        }
        Err(e) => {
            match e.kind() {
                ErrorKind::TimedOut | ErrorKind::WouldBlock => return,
                _ => panic!("{}", e),
            }
        }
    }
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
