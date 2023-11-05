use std::io::{self, stdout, Read, Write, ErrorKind};
use crossterm::terminal::{self, Clear, ClearType};
use crossterm::cursor::{MoveTo};
use crossterm::style::{Print, SetBackgroundColor, SetForegroundColor, Color, ResetColor};
use crossterm::{QueueableCommand};
use crossterm::event::{read, poll, Event, KeyCode, KeyModifiers};
use std::time::Duration;
use std::thread;
use std::net::TcpStream;
use std::str;

struct Rect {
    x: usize, y: usize, w: usize, h: usize,
}

fn chat_window(qc: &mut impl QueueableCommand, chat: &[String], boundary: Rect) -> io::Result<()> {
    let n = chat.len();
    let m = n.checked_sub(boundary.h).unwrap_or(0);
    for (dy, line) in chat.iter().skip(m).enumerate() {
        qc.queue(MoveTo(boundary.x as u16, (boundary.y + dy) as u16))?;
        qc.queue(Print(line.get(0..boundary.w).unwrap_or(&line)))?;
    }
    Ok(())
}

struct RawMode;

impl RawMode {
    fn enable() -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        Ok(RawMode)
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode().map_err(|err| {
            eprintln!("ERROR: disable raw mode: {err}")
        });
    }
}

fn sanitize_terminal_output(bytes: &[u8]) -> Option<String> {
    let bytes: Vec<u8> = bytes.iter().cloned().filter(|x| *x >= 32).collect();
    if let Ok(result) = str::from_utf8(&bytes) {
        Some(result.to_string())
    } else {
        None
    }
}

fn status_bar(qc: &mut impl QueueableCommand, label: &str, x: usize, y: usize, w: usize) -> io::Result<()> {
    if label.len() <= w {
        qc.queue(MoveTo(x as u16, y as u16))?;
        qc.queue(SetBackgroundColor(Color::White))?;
        qc.queue(SetForegroundColor(Color::Black))?;
        qc.queue(Print(label))?;
        for _ in 0..w as usize-label.len() {
            qc.queue(Print(" "))?;
        }
        qc.queue(ResetColor)?;
    }
    Ok(())
}

fn main() -> io::Result<()> {
    let mut stream: Option<TcpStream> = None;
    let mut stdout = stdout();
    let _raw_mode = RawMode::enable()?;
    let (mut w, mut h) = terminal::size()?;
    let mut quit = false;
    let mut prompt = String::new();
    let mut chat = Vec::new();
    let mut buf = [0; 64];
    while !quit {
        while poll(Duration::ZERO)? {
            match read()? {
                Event::Resize(nw, nh) => {
                    w = nw;
                    h = nh;
                }
                Event::Paste(data) => {
                    prompt.push_str(&data);
                }
                Event::Key(event) => {
                    match event.code {
                        KeyCode::Char(x) => {
                            if x == 'c' && event.modifiers.contains(KeyModifiers::CONTROL) {
                                quit = true;
                            } else {
                                prompt.push(x);
                            }
                        }
                        KeyCode::Esc => {
                            prompt.clear();
                        }
                        KeyCode::Enter => {
                            if let Some(ref mut stream) = &mut stream {
                                stream.write(prompt.as_bytes())?;
                                chat.push(prompt.clone());
                            } else {
                                if let Some(ip) = prompt.strip_prefix("/connect ") {
                                    stream = TcpStream::connect(&format!("{ip}:6969")).and_then(|stream| {
                                        stream.set_nonblocking(true)?;
                                        Ok(stream)
                                    }).map_err(|err| {
                                        chat.push(format!("ERROR: Could not connect to {ip}: {err}"))
                                    }).ok();
                                    // TODO: handle /connect when you are already connected
                                    // TODO: implement /disconnect
                                    // TODO: implement /quit
                                } else {
                                    chat.push("You are offline. Use /connect <ip> to connect to a server.".to_string());
                                }
                            }
                            prompt.clear();
                        }
                        _ => {},
                    }
                },
                _ => {},
            }
        }

        if let Some(ref mut s) = &mut stream {
            match s.read(&mut buf) {
                Ok(n) => {
                    if n > 0 {
                        if let Some(line) = sanitize_terminal_output(&buf[..n]) {
                            chat.push(line)
                        }
                    } else {
                        stream = None;
                        chat.push(format!("Server closed the connection"));
                    }
                }
                Err(err) => if err.kind() != ErrorKind::WouldBlock {
                    stream = None;
                    chat.push(format!("ERROR: {err}"));
                }
            }
        }

        stdout.queue(Clear(ClearType::All))?;

        stdout.queue(MoveTo(0, 0))?;
        status_bar(&mut stdout, "4at", 0, 0, w.into())?;
        chat_window(&mut stdout, &chat, Rect {
            x: 0,
            y: 1,
            w: w as usize,
            h: h as usize-3,
        })?;
        if stream.is_some() {
            status_bar(&mut stdout, "Status: Online", 0, h as usize-2, w.into())?;
        } else {
            status_bar(&mut stdout, "Status: Offline", 0, h as usize-2, w.into())?;
        }
        stdout.queue(MoveTo(0, h-1))?;
        stdout.queue(Print(prompt.get(0..(w - 2) as usize).unwrap_or(&prompt)))?;

        // TODO: mouse selection does not work
        stdout.flush()?;

        thread::sleep(Duration::from_millis(33));
    }
    Ok(())
}
