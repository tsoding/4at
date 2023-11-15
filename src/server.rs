use std::net::{TcpListener, TcpStream, IpAddr, SocketAddr, Shutdown};
use std::result;
use std::io::{Read, Write};
use std::fmt;
use std::thread;
use std::sync::mpsc::{Sender, Receiver, channel, RecvTimeoutError};
use std::sync::Arc;
use std::collections::HashMap;
use std::time::{SystemTime, Duration};
use std::str;
use getrandom::getrandom;
use std::fmt::Write as OtherWrite;
use std::fs;

type Result<T> = result::Result<T, ()>;

const PORT: u16 = 6969;
const SAFE_MODE: bool = false;
const BAN_LIMIT: Duration = Duration::from_secs(10*60);
const MESSAGE_RATE: Duration = Duration::from_secs(1);
const SLOWLORIS_LIMIT: Duration = Duration::from_millis(200);
const STRIKE_LIMIT: usize = 10;

struct Sens<T>(T);

impl<T: fmt::Display> fmt::Display for Sens<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self(inner) = self;
        if SAFE_MODE {
            "[REDACTED]".fmt(f)
        } else {
            inner.fmt(f)
        }
    }
}

enum Message {
    ClientConnected {
        author: Arc<TcpStream>,
        author_addr: SocketAddr,
    },
    ClientDisconnected {
        author_addr: SocketAddr,
    },
    NewMessage {
        author_addr: SocketAddr,
        bytes: Box<[u8]>
    },
}

struct Client {
    conn: Arc<TcpStream>,
    last_message: SystemTime,
    connected_at: SystemTime,
    authed: bool,
}

enum Sinner {
    Striked(usize),
    Banned(SystemTime),
}

impl Sinner {
    fn strike(&mut self) -> bool {
        match self {
            Self::Striked(x) => {
                if *x >= STRIKE_LIMIT {
                    *self = Self::Banned(SystemTime::now());
                    true
                } else {
                    *x += 1;
                    false
                }
            }
            Self::Banned(_) => true,
        }
    }
}

struct Server {
    clients: HashMap<SocketAddr, Client>,
    sinners: HashMap<IpAddr, Sinner>,
    token: String,
}

impl Server {
    fn from_token(token: String) -> Self {
        Self {
            clients: HashMap::new(),
            sinners: HashMap::new(),
            token,
        }
    }

    fn client_connected(&mut self, author: Arc<TcpStream>, author_addr: SocketAddr) {
        let now = SystemTime::now();

        if let Some(sinner) = self.sinners.get_mut(&author_addr.ip()) {
            match sinner {
                Sinner::Banned(banned_at) => {
                    let diff = now.duration_since(*banned_at).unwrap_or_else(|err| {
                        eprintln!("ERROR: ban time check on client connection: the clock might have gone backwards: {err}");
                        Duration::from_secs(0)
                    });
                    if diff < BAN_LIMIT {
                        let mut author = author.as_ref();
                        let secs = (BAN_LIMIT - diff).as_secs_f32();
                        // TODO: probably remove this logging, cause banned MFs may still keep connecting and overflow us with logs
                        // println!("INFO: Client {author_addr} tried to connected, by that MF is banned for {secs} secs", author_addr = Sens(author_addr));
                        let _ = writeln!(author, "You are banned MF: {secs} secs left").map_err(|err| {
                            eprintln!("ERROR: could not send banned message to {author_addr}: {err}", author_addr = Sens(author_addr), err = Sens(err));
                        });
                        let _ = author.shutdown(Shutdown::Both).map_err(|err| {
                            eprintln!("ERROR: could not shutdown socket for {author_addr}: {err}", author_addr = Sens(author_addr), err = Sens(err));
                        });
                        return;
                    } else {
                        *sinner = Sinner::Striked(0)
                    }
                }
                Sinner::Striked(_) => {}
            }
        }

        println!("INFO: Client {author_addr} connected", author_addr = Sens(author_addr));
        self.clients.insert(author_addr.clone(), Client {
            conn: author.clone(),
            last_message: now - 2*MESSAGE_RATE,
            connected_at: now,
            authed: false,
        });
    }

    fn client_disconnected(&mut self, author_addr: SocketAddr) {
        // TODO: we need to distinguish between willful client disconnects and banned disconnects
        // println!("INFO: Client {author_addr} disconnected", author_addr = Sens(author_addr));
        self.clients.remove(&author_addr);
    }

    fn new_message(&mut self, author_addr: SocketAddr, bytes: &[u8]) {
        if let Some(author) = self.clients.get_mut(&author_addr) {
            let sinner = self.sinners.entry(author_addr.ip()).or_insert(Sinner::Striked(0));
            let now = SystemTime::now();
            let diff = now.duration_since(author.last_message).unwrap_or_else(|err| {
                eprintln!("ERROR: message rate check on new message: the clock might have gone backwards: {err}");
                Duration::from_secs(0)
            });
            if diff >= MESSAGE_RATE {
                if let Ok(text) = str::from_utf8(&bytes) {
                    *sinner = Sinner::Striked(0);
                    author.last_message = now;
                    if author.authed {
                        println!("INFO: Client {author_addr} sent message {bytes:?}", author_addr = Sens(author_addr));
                        for (addr, client) in self.clients.iter() {
                            if *addr != author_addr && client.authed {
                                let _ = writeln!(client.conn.as_ref(), "{text}").map_err(|err| {
                                    eprintln!("ERROR: could not broadcast message to all the clients from {author_addr}: {err}", author_addr = Sens(author_addr), err = Sens(err))
                                });
                            }
                        }
                    } else {
                        if text == self.token {
                            author.authed = true;
                            println!("INFO: {} authorized!", Sens(author_addr));
                            let _ = writeln!(author.conn.as_ref(), "Welcome to the Club buddy!").map_err(|err| {
                                eprintln!("ERROR: could not send welcome message to {}: {}", Sens(author_addr), Sens(err));
                            });
                        } else {
                            println!("INFO: {} failed authorization!", Sens(author_addr));
                            let _ = writeln!(author.conn.as_ref(), "Invalid token! Bruh!").map_err(|err| {
                                eprintln!("ERROR: could not notify client {} about invalid token: {}", Sens(author_addr), Sens(err));
                            });
                            let _ = author.conn.shutdown(Shutdown::Both).map_err(|err| {
                                eprintln!("ERROR: could not shutdown {}: {}", Sens(author_addr), Sens(err));
                            });
                            self.clients.remove(&author_addr);
                        }
                    }
                } else {
                    if sinner.strike() {
                        println!("INFO: Client {author_addr} got banned", author_addr = Sens(author_addr));
                        self.clients.retain(|addr, client| {
                            if addr.ip() == author_addr.ip() {
                                let _ = writeln!(client.conn.as_ref(), "You are banned MF").map_err(|err| {
                                    eprintln!("ERROR: could not send banned message to {addr}: {err}", addr = Sens(addr), err = Sens(err));
                                });
                                let _ = client.conn.shutdown(Shutdown::Both).map_err(|err| {
                                    eprintln!("ERROR: could not shutdown socket for {addr}: {err}", addr = Sens(addr), err = Sens(err));
                                });
                                return false
                            }
                            true
                        });
                    }
                }
            } else {
                if sinner.strike() {
                    println!("INFO: Client {author_addr} got banned", author_addr = Sens(author_addr));
                    self.clients.retain(|addr, client| {
                        if addr.ip() == author_addr.ip() {
                            let _ = writeln!(client.conn.as_ref(), "You are banned MF").map_err(|err| {
                                eprintln!("ERROR: could not send banned message to {addr}: {err}", addr = Sens(addr), err = Sens(err));
                            });
                            let _ = client.conn.shutdown(Shutdown::Both).map_err(|err| {
                                eprintln!("ERROR: could not shutdown socket for {addr}: {err}", addr = Sens(addr), err = Sens(err));
                            });
                            return false
                        }
                        true
                    });
                }
            }
        }
    }
}

fn server(messages: Receiver<Message>, token: String) -> Result<()> {
    let mut server = Server::from_token(token);
    loop {
        match messages.recv_timeout(Duration::ZERO) {
            Ok(msg) => match msg {
                Message::ClientConnected{author, author_addr} => server.client_connected(author, author_addr),
                Message::ClientDisconnected{author_addr} => server.client_disconnected(author_addr),
                Message::NewMessage{author_addr, bytes} => server.new_message(author_addr, &bytes),
            },
            Err(RecvTimeoutError::Timeout) => {
                // TODO: keep waiting connections in a separate hash map
                server.clients.retain(|addr, client| {
                    if !client.authed {
                        let now = SystemTime::now();
                        let diff = now.duration_since(client.connected_at).unwrap_or_else(|err| {
                            eprintln!("ERROR: slowloris time limit check: the clock might have gone backwards: {err}");
                            SLOWLORIS_LIMIT
                        });
                        if diff >= SLOWLORIS_LIMIT {
                            // TODO: disconnect everyone from addr.ip()
                            server.sinners.entry(addr.ip()).or_insert(Sinner::Striked(0)).strike();
                            let _ = client.conn.shutdown(Shutdown::Both).map_err(|err| {
                                eprintln!("ERROR: could not shutdown socket for {addr}: {err}", addr = Sens(addr), err = Sens(err));
                            });
                            return false;
                        }
                    }
                    true
                });
            }
            Err(RecvTimeoutError::Disconnected) => {
                eprintln!("ERROR: message receiver disconnted");
                return Err(())
            }
        }
    }
}

fn client(stream: Arc<TcpStream>, messages: Sender<Message>) -> Result<()> {
    let author_addr = stream.peer_addr().map_err(|err| {
        eprintln!("ERROR: could not get peer address: {err}", err = Sens(err));
    })?;

    messages.send(Message::ClientConnected{author: stream.clone(), author_addr}).map_err(|err| {
        eprintln!("ERROR: could not send message from {author_addr} to the server thread: {err}", author_addr = Sens(author_addr), err = Sens(err))
    })?;
    let mut buffer = [0; 64];
    loop {
        let n = stream.as_ref().read(&mut buffer).map_err(|err| {
            eprintln!("ERROR: could not read message from {author_addr}: {err}", author_addr = Sens(author_addr), err = Sens(err));
            let _ = messages.send(Message::ClientDisconnected{author_addr}).map_err(|err| {
                eprintln!("ERROR: could not send message to the server thread: {err}")
            });
        })?;
        if n > 0 {
            let bytes = buffer[0..n].iter().cloned().filter(|x| *x >= 32).collect();
            messages.send(Message::NewMessage{author_addr, bytes}).map_err(|err| {
                eprintln!("ERROR: could not send message to the server thread: {err}");
            })?;
        } else {
            let _ = messages.send(Message::ClientDisconnected{author_addr}).map_err(|err| {
                eprintln!("ERROR: could not send message to the server thread: {err}")
            });
            break;
        }
    }
    Ok(())
}

fn generate_token() -> Result<String> {
    let mut buffer = [0; 16];
    let _ = getrandom(&mut buffer).map_err(|err| {
        eprintln!("ERROR: could not generate random access token: {err}");
    })?;

    let mut token = String::new();
    for x in buffer.iter() {
        let _ = write!(&mut token, "{x:02X}");
    }
    Ok(token)
}

fn main() -> Result<()> {
    let token = generate_token()?;
    let token_file_path = "./TOKEN";
    fs::write(token_file_path, token.as_bytes()).map_err(|err| {
        eprintln!("ERROR: could not create token file {token_file_path}: {err}");
    })?;

    println!("INFO: check {token_file_path} file for the token");
    let address = format!("0.0.0.0:{PORT}");
    let listener = TcpListener::bind(&address).map_err(|err| {
        eprintln!("ERROR: could not bind {address}: {err}", address = Sens(&address), err = Sens(err))
    })?;
    println!("INFO: listening to {}", Sens(address));

    let (message_sender, message_receiver) = channel();
    thread::spawn(|| server(message_receiver, token));

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let stream = Arc::new(stream);
                let message_sender = message_sender.clone();
                thread::spawn(|| client(stream, message_sender));
            }
            Err(err) => {
                eprintln!("ERROR: could not accept connection: {err}");
            }
        }
    }
    Ok(())
}
