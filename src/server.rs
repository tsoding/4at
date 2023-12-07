use std::net::{TcpListener, TcpStream, IpAddr, SocketAddr, Shutdown};
use std::result;
use std::io::{Read, Write};
use std::fmt;
use std::rc::Rc;
use std::collections::HashMap;
use std::time::{SystemTime, Duration};
use std::str;
use getrandom::getrandom;
use std::fmt::Write as OtherWrite;
use std::fs;
use std::io;
use std::thread;

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

struct Client {
    conn: Rc<TcpStream>,
    last_message: SystemTime,
    connected_at: SystemTime,
    authed: bool,
}

enum Sinner {
    Striked(usize),
    Banned(SystemTime),
}

impl Sinner {
    fn new() -> Self {
        Self::Striked(0)
    }

    fn forgive(&mut self) {
        *self = Self::Striked(0)
    }

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

    fn client_connected(&mut self, mut author: TcpStream, author_addr: SocketAddr) {
        let now = SystemTime::now();

        if let Some(sinner) = self.sinners.get_mut(&author_addr.ip()) {
            match sinner {
                Sinner::Banned(banned_at) => {
                    let diff = now.duration_since(*banned_at).unwrap_or_else(|err| {
                        eprintln!("ERROR: ban time check on client connection: the clock might have gone backwards: {err}");
                        Duration::ZERO
                    });
                    if diff < BAN_LIMIT {
                        let secs = (BAN_LIMIT - diff).as_secs_f32();
                        // TODO: probably remove this logging, cause banned MFs may still keep connecting and overflow us with logs
                        println!("INFO: Client {author_addr} tried to connected, by that MF is banned for {secs} secs", author_addr = Sens(author_addr));
                        let _ = writeln!(author, "You are banned MF: {secs} secs left").map_err(|err| {
                            eprintln!("ERROR: could not send banned message to {author_addr}: {err}", author_addr = Sens(author_addr), err = Sens(err));
                        });
                        let _ = author.shutdown(Shutdown::Both).map_err(|err| {
                            eprintln!("ERROR: could not shutdown socket for {author_addr}: {err}", author_addr = Sens(author_addr), err = Sens(err));
                        });
                        return;
                    } else {
                        sinner.forgive()
                    }
                }
                Sinner::Striked(_) => {}
            }
        }

        println!("INFO: Client {author_addr} connected", author_addr = Sens(author_addr));
        self.clients.insert(author_addr.clone(), Client {
            conn: Rc::new(author),
            last_message: now - 2*MESSAGE_RATE,
            connected_at: now,
            authed: false,
        });
    }

    fn client_disconnected(&mut self, author_addr: SocketAddr) {
        // TODO: we need to distinguish between willful client disconnects and banned disconnects
        // Banned Sinners may try to use this to fill up all the space on the hard drive
        println!("INFO: Client {author_addr} disconnected", author_addr = Sens(author_addr));
        // TODO: if the disconnected client was not authorized we may probably want to strike their
        // IP, because they are probably constantly connecting/disconnecting trying to evade the
        // strike.
        self.clients.remove(&author_addr);
    }

    fn client_read(&mut self, author_addr: SocketAddr, bytes: &[u8]) {
        if let Some(author) = self.clients.get_mut(&author_addr) {
            let now = SystemTime::now();
            let diff = now.duration_since(author.last_message).unwrap_or_else(|err| {
                eprintln!("ERROR: message rate check on new message: the clock might have gone backwards: {err}");
                Duration::from_secs(0)
            });
            if diff >= MESSAGE_RATE {
                if let Ok(text) = str::from_utf8(&bytes) {
                    self.sinners.entry(author_addr.ip()).or_insert(Sinner::new()).forgive();
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
                            // TODO: let the user know that they were banned after this attempt
                            println!("INFO: {} failed authorization!", Sens(author_addr));
                            let _ = writeln!(author.conn.as_ref(), "Invalid token! Bruh!").map_err(|err| {
                                eprintln!("ERROR: could not notify client {} about invalid token: {}", Sens(author_addr), Sens(err));
                            });
                            let _ = author.conn.shutdown(Shutdown::Both).map_err(|err| {
                                eprintln!("ERROR: could not shutdown {}: {}", Sens(author_addr), Sens(err));
                            });
                            self.clients.remove(&author_addr);
                            // TODO: each IP strike must be properly documented in the source code giving the reasoning
                            // behind it.
                            self.strike_ip(author_addr.ip());
                        }
                    }
                } else {
                    self.strike_ip(author_addr.ip())
                }
            } else {
                self.strike_ip(author_addr.ip());
            }
        }
    }

    fn client_errored(&mut self, author_addr: SocketAddr, err: io::Error) {
        eprintln!("ERROR: could not read message from {author_addr}: {err}", author_addr = Sens(author_addr), err = Sens(err));
        self.clients.remove(&author_addr);
    }

    fn strike_ip(&mut self, ip: IpAddr) {
        let sinner = self.sinners.entry(ip).or_insert(Sinner::new());
        if sinner.strike() {
            println!("INFO: IP {ip} got banned", ip = Sens(ip));
            self.clients.retain(|addr, client| {
                if addr.ip() == ip {
                    let _ = writeln!(client.conn.as_ref(), "You are banned Sinner!").map_err(|err| {
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

    fn update(&mut self) {
        let conns: Vec<_> = self.clients.iter().map(|(&author_addr, client)| {
            (author_addr, Rc::downgrade(&client.conn))
        }).collect();

        let mut buffer = [0; 64];

        for (author_addr, stream) in conns {
            if let Some(stream) = stream.upgrade() {
                match stream.as_ref().read(&mut buffer) {
                    Ok(0) => {
                        self.client_disconnected(author_addr);
                    }
                    Ok(n) => {
                        let bytes: Vec<_> = buffer[0..n].iter().cloned().filter(|x| *x >= 32).collect();
                        self.client_read(author_addr, &bytes);
                    }
                    Err(err) => if err.kind() != io::ErrorKind::WouldBlock {
                        self.client_errored(author_addr, err);
                    }
                }
            }
        }

        // TODO: keep waiting connections in a separate hash map
        self.clients.retain(|addr, client| {
            if !client.authed {
                let now = SystemTime::now();
                let diff = now.duration_since(client.connected_at).unwrap_or_else(|err| {
                    eprintln!("ERROR: slowloris time limit check: the clock might have gone backwards: {err}");
                    SLOWLORIS_LIMIT
                });
                if diff >= SLOWLORIS_LIMIT {
                    // TODO: disconnect everyone from addr.ip()
                    self.sinners.entry(addr.ip()).or_insert(Sinner::new()).strike();
                    let _ = client.conn.shutdown(Shutdown::Both).map_err(|err| {
                        eprintln!("ERROR: could not shutdown socket for {addr}: {err}", addr = Sens(addr), err = Sens(err));
                    });
                    return false;
                }
            }
            true
        });

    }
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
    listener.set_nonblocking(true).map_err(|err| {
        eprintln!("ERROR: could not set server socket as nonblocking: {err}");
    })?;
    println!("INFO: listening to {}", Sens(address));

    let mut server = Server::from_token(token);

    loop {
        match listener.accept() {
            Ok((stream, author_addr)) => {
                if let Err(err) = stream.set_nonblocking(true) {
                    eprintln!("ERROR: could not mark connection as non-blocking: {err}");
                    break;
                }
                server.client_connected(stream, author_addr);
            }
            Err(err) => if err.kind() != io::ErrorKind::WouldBlock {
                eprintln!("ERROR: could not accept connection: {err}")
            }
        }
        server.update();
        thread::sleep(Duration::from_millis(16));
    }
    Ok(())
}
