use std::net::{TcpListener, TcpStream, IpAddr, SocketAddr, Shutdown};
use std::result;
use std::io::{Read, Write};
use std::fmt;
use std::thread;
use std::sync::mpsc::{Sender, Receiver, channel};
use std::sync::Arc;
use std::collections::HashMap;
use std::time::{SystemTime, Duration};
use std::str;

type Result<T> = result::Result<T, ()>;

const SAFE_MODE: bool = false;
const BAN_LIMIT: Duration = Duration::from_secs(10*60);
const MESSAGE_RATE: Duration = Duration::from_secs(1);
const STRIKE_LIMIT: i32 = 10;

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
        author: Arc<TcpStream>
    },
    ClientDisconnected {
        author_addr: SocketAddr,
    },
    NewMessage {
        author_addr: SocketAddr,
        bytes: Vec<u8>
    },
}

struct Client {
    conn: Arc<TcpStream>,
    last_message: SystemTime,
    strike_count: i32,
}

fn server(messages: Receiver<Message>) -> Result<()> {
    let mut clients = HashMap::<SocketAddr, Client>::new();
    let mut banned_mfs = HashMap::<IpAddr, SystemTime>::new();
    loop {
        let msg = messages.recv().expect("The server receiver is not hung up");
        match msg {
            Message::ClientConnected{author} => {
                let author_addr = author.peer_addr().expect("TODO: cache the peer addr of the connection");
                let mut banned_at = banned_mfs.remove(&author_addr.ip());
                let now = SystemTime::now();

                banned_at = banned_at.and_then(|banned_at| {
                    let diff = now.duration_since(banned_at).expect("TODO: don't crash if the clock went backwards");
                    if diff >= BAN_LIMIT {
                        None
                    } else {
                        Some(banned_at)
                    }
                });

                if let Some(banned_at) = banned_at {
                    let diff = now.duration_since(banned_at).expect("TODO: don't crash if the clock went backwards");
                    banned_mfs.insert(author_addr.ip().clone(), banned_at);
                    let mut author = author.as_ref();
                    let secs = (BAN_LIMIT - diff).as_secs_f32();
                    println!("INFO: Client {author_addr} tried to connected, by that MF is banned for {secs} secs", author_addr = Sens(author_addr));
                    let _ = writeln!(author, "You are banned MF: {secs} secs left").map_err(|err| {
                        eprintln!("ERROR: could not send banned message to {author_addr}: {err}", author_addr = Sens(author_addr), err = Sens(err));
                    });
                    let _ = author.shutdown(Shutdown::Both).map_err(|err| {
                        eprintln!("ERROR: could not shutdown socket for {author_addr}: {err}", author_addr = Sens(author_addr), err = Sens(err));
                    });
                } else {
                    println!("INFO: Client {author_addr} connected", author_addr = Sens(author_addr));
                    clients.insert(author_addr.clone(), Client {
                        conn: author.clone(),
                        last_message: now - 2*MESSAGE_RATE,
                        strike_count: 0,
                    });
                }
            },
            Message::ClientDisconnected{author_addr} => {
                println!("INFO: Client {author_addr} disconnected", author_addr = Sens(author_addr));
                clients.remove(&author_addr);
            },
            Message::NewMessage{author_addr, bytes} => {
                if let Some(author) = clients.get_mut(&author_addr) {
                    let now = SystemTime::now();
                    let diff = now.duration_since(author.last_message).expect("TODO: don't crash if the clock went backwards");
                    if diff >= MESSAGE_RATE {
                        if let Ok(text) = str::from_utf8(&bytes) {
                            println!("INFO: Client {author_addr} sent message {bytes:?}", author_addr = Sens(author_addr));
                            for (addr, client) in clients.iter() {
                                if *addr != author_addr {
                                    let _ = writeln!(client.conn.as_ref(), "{text}").map_err(|err| {
                                        eprintln!("ERROR: could not broadcast message to all the clients from {author_addr}: {err}", author_addr = Sens(author_addr), err = Sens(err))
                                    });
                                }
                            }
                        } else {
                            author.strike_count += 1;
                            if author.strike_count >= STRIKE_LIMIT {
                                println!("INFO: Client {author_addr} got banned", author_addr = Sens(author_addr));
                                banned_mfs.insert(author_addr.ip().clone(), now);
                                let _ = writeln!(author.conn.as_ref(), "You are banned MF").map_err(|err| {
                                    eprintln!("ERROR: could not send banned message to {author_addr}: {err}", author_addr = Sens(author_addr), err = Sens(err));
                                });
                                let _ = author.conn.shutdown(Shutdown::Both).map_err(|err| {
                                    eprintln!("ERROR: could not shutdown socket for {author_addr}: {err}", author_addr = Sens(author_addr), err = Sens(err));
                                });
                                clients.remove(&author_addr);
                            }
                        }
                    } else {
                        author.strike_count += 1;
                        if author.strike_count >= STRIKE_LIMIT {
                            println!("INFO: Client {author_addr} got banned", author_addr = Sens(author_addr));
                            banned_mfs.insert(author_addr.ip().clone(), now);
                            let _ = writeln!(author.conn.as_ref(), "You are banned MF").map_err(|err| {
                                eprintln!("ERROR: could not send banned message to {author_addr}: {err}", author_addr = Sens(author_addr), err = Sens(err));
                            });
                            let _ = author.conn.shutdown(Shutdown::Both).map_err(|err| {
                                eprintln!("ERROR: could not shutdown socket for {author_addr}: {err}", author_addr = Sens(author_addr), err = Sens(err));
                            });
                            clients.remove(&author_addr);
                        }
                    }
                }
            },
        }
    }
}

fn client(stream: Arc<TcpStream>, messages: Sender<Message>) -> Result<()> {
    let author_addr = stream.peer_addr().map_err(|err| {
        eprintln!("ERROR: could not get peer address: {err}", err = Sens(err));
    })?;
    messages.send(Message::ClientConnected{author: stream.clone()}).map_err(|err| {
        eprintln!("ERROR: could not send message from {author_addr} to the server thread: {err}", author_addr = Sens(author_addr), err = Sens(err))
    })?;
    let mut buffer = Vec::new();
    buffer.resize(64, 0);
    loop {
        let n = stream.as_ref().read(&mut buffer).map_err(|err| {
            eprintln!("ERROR: could not read message from {author_addr}: {err}", author_addr = Sens(author_addr), err = Sens(err));
            let _ = messages.send(Message::ClientDisconnected{author_addr}).map_err(|err| {
                eprintln!("ERROR: could not send message to the server thread: {err}")
            });
        })?;
        if n > 0 {
            let mut bytes = Vec::new();
            for x in &buffer[0..n] {
                if *x >= 32 {
                    bytes.push(*x)
                }
            }
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

fn main() -> Result<()> {
    let address = "0.0.0.0:6969";
    let listener = TcpListener::bind(address).map_err(|err| {
        eprintln!("ERROR: could not bind {address}: {err}", address = Sens(address), err = Sens(err))
    })?;
    println!("INFO: listening to {}", Sens(address));

    let (message_sender, message_receiver) = channel();
    thread::spawn(|| server(message_receiver));

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
