use std::io::{self, stdout, Read, Write, ErrorKind};
use std::env;
use crossterm::terminal::{self, Clear, ClearType};
use crossterm::cursor::{MoveTo};
use crossterm::{QueueableCommand};
use crossterm::event::{read, poll, Event, KeyCode, KeyModifiers};
use std::time::Duration;
use std::thread;
use std::net::TcpStream;
use std::str;

struct Rect {
    x: usize, y: usize, w: usize, h: usize,
}

fn chat_window(stdout: &mut impl Write, chat: &[String], boundary: Rect) -> io::Result<()> {
    let n = chat.len();
    let m = n.checked_sub(boundary.h).unwrap_or(0);
    for (dy, line) in chat.iter().skip(m).enumerate() {
        stdout.queue(MoveTo(boundary.x as u16, (boundary.y + dy) as u16))?;
        let bytes = line.as_bytes();
        stdout.write(bytes.get(0..boundary.w).unwrap_or(bytes))?;
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

fn main() -> io::Result<()> {
    let mut args = env::args();
    let _program = args.next().expect("program name");
    let ip = args.next().expect("provide ip mf");

    let mut stream = TcpStream::connect(&format!("{ip}:6969"))?;
    let _ = stream.set_nonblocking(true)?;

    let mut stdout = stdout();
    let _raw_mode = RawMode::enable()?;
    let (mut w, mut h) = terminal::size()?;
    let bar_char = "═";
    let mut bar = bar_char.repeat(w as usize);
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
                    bar = bar_char.repeat(w as usize);
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
                            stream.write(prompt.as_bytes())?;
                            chat.push(prompt.clone());
                            prompt.clear();
                        }
                        _ => {},
                    }
                },
                _ => {},
            }
        }

        match stream.read(&mut buf) {
            Ok(n) => {
                if n > 0 {
                    if let Some(line) = sanitize_terminal_output(&buf[..n]) {
                        chat.push(line)
                    }
                } else {
                    quit = true;
                }
            }
            Err(err) => if err.kind() != ErrorKind::WouldBlock {
                return Err(err);
            }
        }

        stdout.queue(Clear(ClearType::All))?;

        chat_window(&mut stdout, &chat, Rect {
            x: 0,
            y: 0,
            w: w as usize,
            h: h as usize-2,
        })?;

        stdout.queue(MoveTo(0, h-2))?;
        stdout.write(bar.as_bytes())?;

        stdout.queue(MoveTo(0, h-1))?;
        {
            let bytes = prompt.as_bytes();
            stdout.write(bytes.get(0..w as usize).unwrap_or(bytes))?;
        }

        stdout.flush()?;

        thread::sleep(Duration::from_millis(33));
    }
    Ok(())
}
