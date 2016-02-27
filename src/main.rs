extern crate time;
extern crate getopts;

use getopts::Options;
use std::env;
use std::error::Error;
use std::io;
use std::io::{Read, Write, BufRead, BufReader};
use std::time::Duration;
use std::convert::From;
use std::net::{TcpListener, TcpStream};
use std::fs::{File, read_dir};
use std::path::{Path, PathBuf, Component};
use std::thread;

#[derive(Debug)]
enum UriError {
    NotFound,
    IllegalPath,
}

#[derive(Debug)]
enum ResponseItem {
    File(File),
    Directory(PathBuf),
}

fn print_usage(program: &str, opts: Options) {
    let brief = format!("Usage: {} [options]", program);
    print!("{}", opts.usage(&brief));
}

fn find_file(uri: &str) -> Result<ResponseItem, UriError> {
    let path = Path::new(uri);
    let mut clean_path = PathBuf::from(".");
    for component in path.components() {
        match component {
            Component::ParentDir => {
                if !clean_path.pop() {
                    return Err(UriError::IllegalPath);
                }
            }
            Component::Normal(name) => {
                clean_path.push(name);
            }
            _ => {}
        }
    }
    if clean_path.is_dir() {
        Ok(ResponseItem::Directory(clean_path))
    } else {
        File::open(clean_path)
            .or(Err(UriError::NotFound))
            .map(|f| ResponseItem::File(f))
    }
}

fn current_time_string() -> String {
    time::strftime("%a, %d %b %Y %H:%M:%S %Z", &time::now()).unwrap()
}

fn head(stream: &mut TcpStream, content_type: &str, body_length: usize) -> io::Result<()> {
    let message = format!("HTTP/1.1 200 OK\r\n\
                           Date: {}\r\n\
                           Connection: close\r\n\
                           Server: Rust Serv/0.1.1\r\n\
                           Content-Type: {}\r\n\
                           Content-Length: {}\r\n\
                           \r\n",
                          current_time_string(),
                          content_type,
                          body_length);
    try!(stream.write(message.as_bytes()));
    Ok(())
}

fn not_allowed(stream: &mut TcpStream) -> io::Result<()> {
    let message = format!("HTTP/1.1 405 Method Not Allowed\r\n\
                           Date: {}\r\n\
                           Connection: close\r\n\
                           Server: Rust Serv/0.1.1\r\n\
                           Allow: GET, HEAD\r\n\
                           Content-Length: 0\r\n\
                           \r\n",
                          current_time_string());
    try!(stream.write(message.as_bytes()));
    Ok(())
}

fn not_found(stream: &mut TcpStream, uri: &str) -> io::Result<()> {
    let body = format!("Resource '{}' not found", uri);
    let message = format!("HTTP/1.1 404 Not Found\r\n\
                           Date: {}\r\n\
                           Connection: close\r\n\
                           Server: Rust Serv/0.1.1\r\n\
                           Content-Length: {}\r\n\
                           \r\n\
                           {}",
                          current_time_string(),
                          body.len(),
                          body);
    try!(stream.write(message.as_bytes()));
    Ok(())
}

fn not_permitted(stream: &mut TcpStream) -> io::Result<()> {
    let message = format!("HTTP/1.1 403 Not Permitted\r\n\
                           Date: {}\r\n\
                           Connection: close\r\n\
                           Server: Rust Serv/0.1.1\r\n\
                           Content-Length: 0\r\n\
                           \r\n",
                          current_time_string());
    try!(stream.write(message.as_bytes()));
    Ok(())
}

fn handle_client(stream: TcpStream) -> Result<(), Box<Error>> {
    try!(stream.set_read_timeout(Some(Duration::from_secs(5))));
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    try!(reader.read_line(&mut line));

    let items = line.split_whitespace().collect::<Vec<_>>();
    if items.len() < 3 {
        return Err(From::from("Not enough items in HTTP request line!"));
    }

    let protocol = items[2];
    if protocol != "HTTP/1.1" {
        return Err(From::from(format!("Unsupported protocol {}", protocol)));
    }

    let mut stream = reader.into_inner();

    let method = items[0];
    match method {
        "HEAD" | "GET" => {}
        _ => {
            try!(not_allowed(&mut stream));
            return Err(From::from(format!("Method {} not allowed", method)));
        }
    }

    let uri = items[1];
    let file = find_file(uri);
    match file {
        Ok(file) => {
            let mut data = Vec::new();

            let path = Path::new(uri);
            let mut content_type = match path.extension().and_then(|x| x.to_str()) {
                Some("html") | Some("htm") => "text/html",
                Some("json") => "application/json",
                _ => "text/plain",
            };

            let length = match file {
                ResponseItem::File(mut file) => {
                    match method {
                        "GET" => try!(file.read_to_end(&mut data)),
                        "HEAD" => try!(file.metadata()).len() as usize,
                        _ => unreachable!(),
                    }
                }
                ResponseItem::Directory(dir) => {
                    let members = try!(read_dir(&dir));
                    let items = members.filter_map(|file| {
                        file.ok().and_then(|file| file.path().to_str().map(String::from))
                    });
                    let items = items.map(|path| {
                        format!(r#"<li><a href="{path}">{name}</a></li>"#,
                                name = if path.starts_with("./") {
                                    &path[2..]
                                } else {
                                    &path[..]
                                },
                                path = if path.starts_with("./") {
                                    &path[1..]
                                } else {
                                    &path[..]
                                })
                    });
                    let items = items.collect::<Vec<_>>().concat();
                    let content = format!("<html><head><title>{path}</title></head>\
                                           <body>Index for {path}\
                                           <ul>{items}</ul>\
                                           </body></html>",
                                          path = dir.to_str().unwrap(),
                                          items = items);
                    content_type = "text/html";
                    data.extend_from_slice(content.as_bytes());
                    data.len()
                }
            };

            try!(head(&mut stream, content_type, length));
            if method == "GET" {
                try!(stream.write(&data[..]));
            }
        }
        Err(UriError::NotFound) => {
            try!(not_found(&mut stream, uri));
        }
        Err(UriError::IllegalPath) => {
            try!(not_permitted(&mut stream));
        }
    }
    Ok(())
}

fn main() {
    let args: Vec<_> = env::args().collect();
    let program = args[0].clone();

    let mut opts = Options::new();
    opts.optopt("p",
                "port",
                "set the port for the server (default 8000)",
                "PORT");
    opts.optflag("h", "help", "print this help menu");
    let matches = match opts.parse(&args[1..]) {
        Ok(m) => m,
        Err(f) => panic!(f.to_string()),
    };

    if matches.opt_present("h") {
        print_usage(&program, opts);
        return;
    }

    let port = matches.opt_str("p").and_then(|x| x.parse::<i32>().ok()).unwrap_or(8000);

    let address = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(&address[..]).unwrap();
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                thread::spawn(move || {
                    handle_client(stream).map_err(|e| println!("{}", e.description()))
                });
            }
            Err(e) => {
                println!("Connection failed! {}", e);
            }
        }
    }
}
