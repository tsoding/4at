use std::io::{self, stdout, Read, Write, ErrorKind};
use crossterm::terminal::{self, Clear, ClearType};
use crossterm::cursor::{MoveTo};
use crossterm::style::{Print, PrintStyledContent, SetBackgroundColor, SetForegroundColor, Color, ResetColor, Stylize};
use crossterm::{QueueableCommand};
use crossterm::event::{read, poll, Event, KeyCode, KeyModifiers};
use std::time::Duration;
use std::thread;
use std::net::TcpStream;
use std::str;

struct Rect {
    x: usize, y: usize, w: usize, h: usize,
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

fn parse_command<'a>(prompt: &'a str) -> Option<(&'a str, &'a str)> {
    let prompt = prompt.strip_prefix("/")?;
    prompt.split_once(" ").or(Some((prompt, "")))
}

#[derive(Default)]
struct ChatLog {
    items: Vec<(String, Color)>,
}

impl ChatLog {
    fn push(&mut self, message: String, color: Color) {
        self.items.push((message, color))
    }

    fn render(&mut self, qc: &mut impl QueueableCommand, boundary: Rect) -> io::Result<()> {
        let n = self.items.len();
        let m = n.checked_sub(boundary.h).unwrap_or(0);
        for (dy, (line, color)) in self.items.iter().skip(m).enumerate() {
            qc.queue(MoveTo(boundary.x as u16, (boundary.y + dy) as u16))?;
            qc.queue(PrintStyledContent(line.get(0..boundary.w).unwrap_or(&line).with(*color)))?;
        }
        Ok(())
    }
}

macro_rules! chat_msg {
    ($chat:expr, $($arg:tt)*) => {
        $chat.push(format!($($arg)*), Color::White)
    }
}

macro_rules! chat_error {
    ($chat:expr, $($arg:tt)*) => {
        $chat.push(format!($($arg)*), Color::Red)
    }
}

macro_rules! chat_info {
    ($chat:expr, $($arg:tt)*) => {
        $chat.push(format!($($arg)*), Color::Blue)
    }
}

#[derive(Default)]
struct Client {
    stream: Option<TcpStream>,
    chat: ChatLog,
    quit: bool,
}

fn connect_command(client: &mut Client, argument: &str) {
    if client.stream.is_none() {
        let ip = argument.trim();
        client.stream = TcpStream::connect(&format!("{ip}:6969")).and_then(|stream| {
            stream.set_nonblocking(true)?;
            Ok(stream)
        }).map_err(|err| {
            chat_error!(&mut client.chat, "Could not connect to {ip}: {err}")
        }).ok();
    } else {
        chat_error!(&mut client.chat, "You are already connected to a server. Disconnect with /disconnect first.");
    }
}

fn disconnect_command(client: &mut Client, _argument: &str) {
    if client.stream.is_some() {
        client.stream = None;
        chat_info!(&mut client.chat, "Disconnected.");
    } else {
        chat_info!(&mut client.chat, "You are already offline ._.");
    }
}

fn quit_command(client: &mut Client, _argument: &str) {
    client.quit = true;
}

fn help_command(client: &mut Client, argument: &str) {
    let name = argument.trim();
    if name.is_empty() {
        for command in COMMANDS.iter() {
            chat_info!(client.chat, "/{name} - {description}", name = command.name, description = command.description);
        }
    } else {
        if let Some(command) = find_command(name) {
            chat_info!(client.chat, "/{name} - {description}", name = command.name, description = command.description);
        } else {
            chat_error!(&mut client.chat, "Unknown command `/{name}`");
        }
    }
}

struct Command {
    name: &'static str,
    run: fn(&mut Client, &str),
    description: &'static str,
}

const COMMANDS: [Command; 4] = [
    Command {
        name: "connect",
        run: connect_command,
        description: "Connect to a server by IP"
    },
    Command {
        name: "disconnect",
        run: disconnect_command,
        description: "Disconnect from the server you are currently connected to"
    },
    Command {
        name: "quit",
        run: quit_command,
        description: "Close the chat"
    },
    Command {
        name: "help",
        run: help_command,
        description: "Print help",
    },
];

fn find_command(name: &str) -> Option<&Command> {
    COMMANDS.iter().find(|command| command.name == name)
}

fn main() -> io::Result<()> {
    let mut client = Client::default();
    let mut stdout = stdout();
    let _raw_mode = RawMode::enable()?;
    let (mut w, mut h) = terminal::size()?;
    let mut prompt = String::new();
    let mut prompt_cursor: usize = 0;
    let mut buf = [0; 64];
    while !client.quit {
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
                                client.quit = true;
                            } else {
                                if prompt_cursor > prompt.len() {
                                    prompt_cursor = prompt.len()
                                }
                                prompt.insert(prompt_cursor, x);
                                prompt_cursor += 1;
                            }
                        }
                        // TODO: message history scrolling via up/down
                        KeyCode::Left => {
                            if prompt_cursor > 0 {
                                prompt_cursor -= 1;
                            }
                        }
                        KeyCode::Right => {
                            if prompt_cursor < prompt.len() {
                                prompt_cursor += 1;
                            }
                        }
                        KeyCode::Backspace => {
                            if prompt_cursor > 0 {
                                prompt_cursor -= 1;
                                prompt.remove(prompt_cursor);
                            }
                        }
                        KeyCode::Tab => {
                            if let Some((prefix, "")) = parse_command(&prompt[..prompt_cursor]) {
                                if let Some(command) = COMMANDS.iter().find(|command| command.name.starts_with(prefix)) {
                                    // TODO: tab autocompletion should scroll through different
                                    // variants on each TAB press
                                    prompt = format!("/{name}{rest}", name = command.name, rest = &prompt[prompt_cursor..]);
                                    prompt_cursor = command.name.len() + 1;
                                }
                            }
                        }
                        KeyCode::Enter => {
                            if let Some((name, argument)) = parse_command(&prompt) {
                                if let Some(command) = find_command(name) {
                                    (command.run)(&mut client, &argument);
                                } else {
                                    chat_error!(&mut client.chat, "Unknown command `/{name}`");
                                }
                            } else {
                                if let Some(ref mut stream) = &mut client.stream {
                                    stream.write(prompt.as_bytes())?;
                                    chat_msg!(&mut client.chat, "{prompt}");
                                } else {
                                    chat_info!(&mut client.chat, "You are offline. Use /connect <ip> to connect to a server.");
                                }
                            }
                            prompt.clear();
                            prompt_cursor = 0;
                        }
                        _ => {},
                    }
                },
                _ => {},
            }
        }

        if let Some(ref mut s) = &mut client.stream {
            match s.read(&mut buf) {
                Ok(n) => {
                    if n > 0 {
                        if let Some(line) = sanitize_terminal_output(&buf[..n]) {
                            client.chat.push(line, Color::White)
                        }
                    } else {
                        client.stream = None;
                        chat_info!(&mut client.chat, "Server closed the connection");
                    }
                }
                Err(err) => if err.kind() != ErrorKind::WouldBlock {
                    client.stream = None;
                    chat_error!(&mut client.chat, "Connection Error: {err}");
                }
            }
        }

        stdout.queue(Clear(ClearType::All))?;

        stdout.queue(MoveTo(0, 0))?;
        status_bar(&mut stdout, "4at", 0, 0, w.into())?;
        // TODO: scrolling for chat window
        client.chat.render(&mut stdout, Rect {
            x: 0,
            y: 1,
            w: w as usize,
            // TODO: make sure there is no underflow anywhere when the user intentionally make the
            // terminal very small
            h: h as usize-3,
        })?;
        if client.stream.is_some() {
            status_bar(&mut stdout, "Status: Online", 0, h as usize-2, w.into())?;
        } else {
            status_bar(&mut stdout, "Status: Offline", 0, h as usize-2, w.into())?;
        }
        stdout.queue(MoveTo(0, h-1))?;
        stdout.queue(Print(prompt.get(0..(w - 2) as usize).unwrap_or(&prompt)))?;
        stdout.queue(MoveTo(prompt_cursor as u16, h-1))?;

        // TODO: mouse selection does not work
        stdout.flush()?;

        thread::sleep(Duration::from_millis(33));
    }
    Ok(())
}
