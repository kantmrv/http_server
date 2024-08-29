use std::{env::args, fs::{self, File}, io::{BufRead, BufReader, Read, Write}, net::{TcpListener, TcpStream}, thread};

#[derive(PartialEq)]
enum HttpMethod {
    GET,
    HEAD,
    POST,
    PUT,    
    DELETE,
    CONNECT,
    OPTIONS,
    TRACE,
    PATCH,
}
impl From<&str> for HttpMethod {
    fn from(value: &str) -> Self {
        match value {
            "HEAD" => Self::HEAD,
            "POST" => Self::POST,
            "PUT" => Self::PUT,
            "DELETE" => Self::DELETE,
            "CONNECT" => Self::CONNECT,
            "OPTIONS" => Self::OPTIONS,
            "TRACE" => Self::TRACE,
            "PATCH" => Self::PATCH,
            _ => Self::GET,
        }
    }
}

enum HttpVersion {  
    V1_1,
}
impl From<&str> for HttpVersion {
    fn from(value: &str) -> Self {
        match value {
            _ => Self::V1_1,
        }
    }
}
impl<'a> HttpVersion {
    fn as_str(&self) -> &'a str {
        match self {
            HttpVersion::V1_1 => "HTTP/1.1",
        }
    }
}

struct HttpHeaders {
    host: Option<String>,
    user_agent: Option<String>,
    content_encoding: Option<String>,
    content_length: Option<usize>,
    request_body: Option<String>,
}
impl Default for HttpHeaders {
    fn default() -> Self {
        HttpHeaders { host: None, user_agent: None, content_encoding: None, content_length: None, request_body: None }
    }
}

struct HttpRequest {
    method: HttpMethod,
    path: String,
    version: HttpVersion,
    headers: HttpHeaders,

}
impl Default for HttpRequest {
    fn default() -> Self {
        HttpRequest { method: HttpMethod::GET, path: String::from("/"), version: HttpVersion::V1_1, headers: HttpHeaders::default()}
    }
}
impl HttpRequest {
    fn from_buf(mut buf: BufReader<&TcpStream>) -> Self {
        let mut data = String::new();
        let mut line = String::new();
        loop {
            buf.read_line(&mut line).unwrap_or_default();
            match line.starts_with("\r\n") {
                true => { break; },
                false => {
                    data.push_str(&line);
                    line.clear();
                },
            } 
        }

        let mut lines = data.split("\r\n");
        if let Some(line) = lines.next() {
            let mut start_line = line.split_ascii_whitespace().into_iter();
            let (method, path, version) = (
                HttpMethod::from(start_line.next().unwrap_or_default()), 
                start_line.next().unwrap_or_default().to_owned(), 
                HttpVersion::from(start_line.next().unwrap_or_default())
            );

            let mut headers = HttpHeaders::default();
            for line in lines {
                match line {
                    line if line.starts_with("Host") => {
                        headers.host = Some(line.split_once(": ").unwrap_or_default().1.to_owned());
                    },
                    line if line.starts_with("User-Agent") => {
                        headers.user_agent = Some(line.split_once(": ").unwrap_or_default().1.to_owned());
                    },
                    line if line.starts_with("Content-Length") => {
                        headers.content_length = Some(line.split_once(": ").unwrap_or_default().1.parse().unwrap_or_default());
                    },
                    line if line.starts_with("Accept-Encoding") => {
                        headers.content_encoding = Some(line.split_once(": ").unwrap_or_default().1.to_owned());
                    },
                    _ => break,
                }
            }
            if let Some(len) = headers.content_length {
                let mut request_body = vec![0u8; len];
                buf.read_exact(&mut request_body).unwrap_or_default();
                headers.request_body = Some(String::from_utf8(request_body).unwrap_or_default());
            }
            
            Self {method: method, path: path, version: version, headers: headers}
        } else { 
            Self::default() 
        }
    }
}

enum StatusCode {
    Ok,
    Created,
    NotFound,
}
impl<'a> StatusCode {
    fn as_str(&self) -> &'a str {
        match self {
            Self::Ok => "200 OK",
            Self::Created => "201 Created",
            Self::NotFound => "404 Not Found",
        }
    }
}
fn main() {
    let listener = TcpListener::bind("127.0.0.1:4221").unwrap();

    for stream in listener.incoming() {
        thread::spawn(move || {
        match stream {
            Ok(mut _stream) => {
                let buf = BufReader::new(&_stream);
                let request = HttpRequest::from_buf(buf);

                match request.path.as_str() {
                    "/" => {
                        let message = format!("{} {}\r\n\r\n", 
                            request.version.as_str(), 
                            StatusCode::Ok.as_str(), 
                        );
                        _stream.write(message.as_bytes()).unwrap();
                    },
                    path if path.starts_with("/echo/") => {
                        let body: String = request.path.split_inclusive('/').skip(2).collect();
                        let message = format!(
                            "{} {}\r\nContent-Type: text/plain\r\n{}Content-Length: {}\r\n\r\n{}",
                            request.version.as_str(), 
                            StatusCode::Ok.as_str(),
                            if let Some(content_encoding) = request.headers.content_encoding.filter(|s| s=="gzip") { format!("Content-Encoding: {}\r\n", content_encoding) } else { String::new() },
                            body.len(),
                            body
                        );
                        println!("{}", message);
                        _stream.write(message.as_bytes()).unwrap();
                    },
                    path if path.starts_with("/files/") => {
                        let dir = args().nth(2).unwrap_or_default();
                        let filename: String = request.path.split_inclusive('/').skip(2).collect();

                        match request.method {
                            HttpMethod::GET => {
                                if let Ok(body) = fs::read_to_string([dir, filename].concat()) {
                                    let message = format!("{} {}\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\n\r\n{}", 
                                        request.version.as_str(), 
                                        StatusCode::Ok.as_str(), 
                                        body.len(),
                                        body
                                    );
                                    _stream.write(message.as_bytes()).unwrap();
                                } else {
                                    let message = format!("{} {}\r\n\r\n", 
                                        request.version.as_str(), 
                                        StatusCode::NotFound.as_str(), 
                                    );
                                    _stream.write(message.as_bytes()).unwrap();
                                }
                            },
                            HttpMethod::POST => {
                                if let Ok(mut f) = File::create([dir, filename].concat()) {
                                    f.write_all(request.headers.request_body.unwrap_or_default().as_bytes()).unwrap_or_default();
                                    let message = format!("{} {}\r\n\r\n", 
                                        request.version.as_str(), 
                                        StatusCode::Created.as_str(), 
                                    );
                                    _stream.write(message.as_bytes()).unwrap();
                                } else {
                                    let message = format!("{} {}\r\n\r\n", 
                                        request.version.as_str(), 
                                        StatusCode::NotFound.as_str(), 
                                    );
                                    _stream.write(message.as_bytes()).unwrap();
                                }
                            },
                            _ => {
                                let message = format!("{} {}\r\n\r\n", 
                                    request.version.as_str(), 
                                    StatusCode::NotFound.as_str(), 
                                );
                                _stream.write(message.as_bytes()).unwrap();
                            },
                        }
                    },
                    path if path.split_inclusive('/').nth(1)==Some("user-agent") => {
                        let body: String = request.headers.user_agent.unwrap_or_default();
                        let message = format!("{} {}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}", 
                            request.version.as_str(), 
                            StatusCode::Ok.as_str(), 
                            body.len(),
                            body
                        );
                        _stream.write(message.as_bytes()).unwrap();
                    },
                    _ => {
                        let message = format!("{} {}\r\n\r\n", 
                            request.version.as_str(), 
                            StatusCode::NotFound.as_str(), 
                        );
                        _stream.write(message.as_bytes()).unwrap();
                    },
                } 
            }
            Err(e) => {
                println!("error: {}", e);
            }
        }});
    }
    
}