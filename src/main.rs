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

#[derive(Debug, PartialEq, Eq)]
enum UriError {
    NotFound,
    IllegalPath,
}

#[derive(Debug)]
enum ResponseItem {
    File(File),
    Directory(PathBuf),
}

#[derive(Debug, PartialEq, Eq)]
enum Method {
    Get,
    Head,
}

fn print_usage(program: &str, opts: Options) {
    let brief = format!("Usage: {} [options]", program);
    print!("{}", opts.usage(&brief));
}

fn content_type_for(uri: &str) -> &str {
    match Path::new(uri).extension().and_then(|x| x.to_str()) {
        Some("html") | Some("htm") => "text/html",
        Some("json") => "application/json",
        Some("css") => "text/css",
        Some("js") => "text/javascript",
        _ => "text/plain",
    }
}

fn current_time_string() -> String {
    time::strftime("%a, %d %b %Y %H:%M:%S %Z", &time::now()).unwrap()
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

fn respond_header(stream: &mut TcpStream,
                  status: &str,
                  content_type: &str,
                  body_length: usize)
                  -> io::Result<()> {
    let message = format!("HTTP/1.1 {status}\r\n\
                           Date: {date}\r\n\
                           Connection: close\r\n\
                           Server: Rust Serv/0.2\r\n\
                           Allow: GET, HEAD\r\n\
                           Content-Type: {content_type}\r\n\
                           Content-Length: {length}\r\n\
                           \r\n",
                          status = status,
                          date = current_time_string(),
                          content_type = content_type,
                          length = body_length);
    stream.write(message.as_bytes()).and(Ok(()))
}

fn head(stream: &mut TcpStream, content_type: &str, body_length: usize) -> io::Result<()> {
    respond_header(stream, "200 OK", content_type, body_length)
}

fn not_allowed(stream: &mut TcpStream) -> io::Result<()> {
    respond_header(stream, "405 Method Not Allowed", "text/plain", 0)
}

fn not_found(stream: &mut TcpStream, uri: &str) -> io::Result<()> {
    let body = format!("Resource '{}' not found", uri);
    try!(respond_header(stream, "404 Not Found", "text/plain", body.len()));
    stream.write(body.as_bytes()).and(Ok(()))
}

fn not_permitted(stream: &mut TcpStream) -> io::Result<()> {
    respond_header(stream, "403 Not Permitted", "text/plain", 0)
}

fn handle_client(stream: TcpStream, index_name: &str, list_dir: bool) -> Result<(), Box<Error>> {
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

    let method = match items[0] {
        "HEAD" => Method::Head,
        "GET" => Method::Get,
        _ => {
            try!(not_allowed(&mut stream));
            return Err(From::from(format!("Method {} not allowed", items[0])));
        }
    };

    let uri = items[1];
    respond_to(&mut stream, method, uri, index_name, list_dir)
}

fn respond_file(stream: &mut TcpStream,
                method: Method,
                uri: &str,
                file: &mut File)
                -> Result<(), Box<Error>> {
    let mut data = Vec::new();
    let length = match method {
        Method::Get => try!(file.read_to_end(&mut data)),
        Method::Head => try!(file.metadata()).len() as usize,
    };

    let content_type = content_type_for(&uri);
    try!(head(stream, content_type, length));
    if method == Method::Get {
        try!(stream.write(&data[..]));
    }
    Ok(())
}

fn respond_dir(stream: &mut TcpStream,
               method: Method,
               dir: &Path,
               index: &str,
               list_dir: bool)
               -> Result<(), Box<Error>> {
    if index != "" {
        let uri = dir.join(index);
        match File::open(&uri) {
            Ok(mut file) => {
                try!(respond_file(stream, method, uri.to_str().unwrap(), &mut file));
                return Ok(());
            }
            Err(_) => {}
        }
    }

    if !list_dir {
        try!(not_found(stream, dir.to_str().unwrap()));
        return Err(From::from(
            format!("404: {}: No index found and directory listing disabled",
                    dir.to_str().unwrap())));
    }
    let members = try!(read_dir(&dir));
    let items = members.filter_map(|file| {
        file.ok().and_then(|file| file.path().to_str().map(String::from))
    });
    let items = items.map(|path| {
        format!(r#"<li><a href="{path}">{name}</a></li>"#,
                name = path.trim_left_matches("./"),
                path = path.trim_left_matches("."))
    });
    let items = items.collect::<Vec<_>>().concat();
    let content = format!("<html><head><title>{path}</title></head>\
                           <body>Index for {path}\
                           <ul>{items}</ul>\
                           </body></html>",
                          path = dir.to_str().unwrap(),
                          items = items);

    try!(head(stream, "text/html", content.len()));
    if method == Method::Get {
        try!(stream.write(content.as_bytes()));
    }
    Ok(())
}

fn respond_to(stream: &mut TcpStream,
              method: Method,
              uri: &str,
              index: &str,
              list_dir: bool)
              -> Result<(), Box<Error>> {
    let file = find_file(uri);
    match file {
        Ok(file) => {
            match file {
                ResponseItem::File(mut file) => respond_file(stream, method, uri, &mut file),
                ResponseItem::Directory(dir) => respond_dir(stream, method, &dir, index, list_dir),
            }
        }
        Err(UriError::NotFound) => Ok(try!(not_found(stream, uri))),
        Err(UriError::IllegalPath) => Ok(try!(not_permitted(stream))),
    }
}

fn main() {
    let args: Vec<_> = env::args().collect();
    let program = args[0].clone();

    let mut opts = Options::new();
    opts.optopt("p",
                "port",
                "set the port for the server (default 8000)",
                "PORT");
    opts.optopt("i",
                "index",
                "set the file for the index document (default: index.html)",
                "FILE");
    opts.optflag("n", "no-directory", "prevent directories from being listed");
    opts.optflag("h", "help", "print this help menu");
    let matches = opts.parse(&args[1..]).unwrap();

    if matches.opt_present("h") {
        print_usage(&program, opts);
        return;
    }

    let list_directory = !matches.opt_present("n");
    let port = matches.opt_str("p").and_then(|x| x.parse::<i32>().ok()).unwrap_or(8000);
    let index_name = matches.opt_str("i").unwrap_or("index.html".into());

    println!("Starting server on localhost:{}", port);
    let address = format!("localhost:{}", port);
    let listener = TcpListener::bind(&address[..]).unwrap();
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let name = index_name.clone();
                thread::spawn(move || {
                    handle_client(stream, &name[..], list_directory)
                        .map_err(|e| println!("{:?}", e))
                });
            }
            Err(e) => {
                println!("Connection failed! {}", e);
            }
        }
    }
}
