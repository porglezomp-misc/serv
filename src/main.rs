extern crate time;

use std::io::{Write, BufRead, BufReader};
use std::time::Duration;
use std::net::{TcpListener, TcpStream};
use std::thread;

fn current_time_string() -> String {
    time::strftime("%a, %d %b %Y %H:%M:%S %Z", &time::now()).unwrap()
}

fn head(stream: &mut TcpStream, body_length: usize) {
    let message = format!("HTTP/1.1 200 OK\r\n\
                           Date: {}\r\n\
                           Connection: close\r\n\
                           Server: Rust Serv/0.1\r\n\
                           Content-Type: text/plain\r\n\
                           Content-Length: {}\r\n\
                           \r\n",
                          current_time_string(),
                          body_length);
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

fn handle_client(stream: TcpStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let _ = reader.read_line(&mut line);

    let items = line.split_whitespace().collect::<Vec<_>>();
    if items.len() < 3 {
        return;
    }

    let protocol = items[2];
    if protocol != "HTTP/1.1" {
        return;
    }

    let mut stream = reader.into_inner();

    let method = items[0];
    match method {
        "HEAD" | "GET" => { }
        _ => not_allowed(&mut stream),
    }

    let uri = items[1];
    head(&mut stream, uri.len());
    if method == "GET" {
        let _ = stream.write(uri.as_bytes());
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
