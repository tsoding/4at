use std::io::{self, stdout, Read, Write, ErrorKind};
use crossterm::terminal::{self, Clear, ClearType};
use crossterm::cursor::{MoveTo};
use crossterm::style::{Print, PrintStyledContent, SetBackgroundColor, SetForegroundColor, Color, ResetColor, Stylize};
use crossterm::{QueueableCommand};
use crossterm::event::{read, poll, Event, KeyCode, KeyModifiers, KeyEventKind};
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

fn parse_command<'a>(prompt: &'a [char]) -> Option<(&'a [char], &'a [char])> {
    let prompt = prompt.strip_prefix(&['/'])?;
    let mut iter = prompt.splitn(2, |x| *x == ' ');
    let a = iter.next().unwrap_or(prompt);
    let b = iter.next().unwrap_or(&[]);
    Some((a, b))
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
struct Prompt {
    buffer: Vec<char>,
    cursor: usize,
}

impl Prompt {
    fn insert(&mut self, x: char) {
        if self.cursor > self.buffer.len() {
            self.cursor = self.buffer.len()
        }
        self.buffer.insert(self.cursor, x);
        self.cursor += 1;
    }

    fn insert_str(&mut self, text: &str) {
        for x in text.chars() {
            self.insert(x)
        }
    }

    fn left_char(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    fn right_char(&mut self) {
        if self.cursor < self.buffer.len() {
            self.cursor += 1;
        }
    }

    fn at_cursor(&self) -> char {
        self.buffer.get(self.cursor).cloned().unwrap_or('\n')
    }

    fn left_word(&mut self) {
        while self.cursor > 0 && self.at_cursor().is_whitespace() {
            self.cursor -= 1;
        }
        while self.cursor > 0 && !self.at_cursor().is_whitespace() {
            self.cursor -= 1;
        }
    }

    fn right_word(&mut self) {
        while self.cursor < self.buffer.len() && self.at_cursor().is_whitespace() {
            self.cursor += 1;
        }
        while self.cursor < self.buffer.len() && !self.at_cursor().is_whitespace() {
            self.cursor += 1;
        }
    }

    fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.buffer.remove(self.cursor);
        }
    }

    fn before_cursor(&self) -> &[char] {
        &self.buffer[..self.cursor]
    }

    fn after_cursor(&self) -> &[char] {
        &self.buffer[self.cursor..]
    }

    fn clear(&mut self) {
        self.buffer.clear();
        self.cursor = 0;
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
        let chunks: Vec<&str> = argument.split(' ').filter(|s| !s.is_empty()).collect();
        match &chunks[..] {
            &[ip, token] => {
                client.stream = TcpStream::connect(&format!("{ip}:6969"))
                    .and_then(|mut stream| {
                        stream.set_nonblocking(true)?;
                        stream.write(token.as_bytes())?;
                        Ok(stream)
                    })
                    .map_err(|err| {
                        chat_error!(&mut client.chat, "Could not connect to {ip}: {err}")
                    })
                    .ok();
            }
            _ => {
                chat_error!(&mut client.chat, "Incorrect usage of connect command. Try /connect <ip> <token>");
            }
        }
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

const COMMANDS: &[Command] = &[
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
    let mut prompt = Prompt::default();
    let mut buf = [0; 64];
    while !client.quit {
        while poll(Duration::ZERO)? {
            match read()? {
                Event::Resize(nw, nh) => {
                    w = nw;
                    h = nh;
                }
                Event::Paste(data) => prompt.insert_str(&data),
                Event::Key(event) => {
                    if event.kind == KeyEventKind::Press {
                        match event.code {
                            KeyCode::Char(x) => {
                                if x == 'c' && event.modifiers.contains(KeyModifiers::CONTROL) {
                                    client.quit = true;
                                } else {
                                    prompt.insert(x);
                                }
                            }
                            // TODO: message history scrolling via up/down
                            // TODO: basic readline navigation keybindings
                            KeyCode::Left => if event.modifiers.contains(KeyModifiers::CONTROL) {
                                prompt.left_word();
                            } else {
                                prompt.left_char();
                            }
                            KeyCode::Right => if event.modifiers.contains(KeyModifiers::CONTROL) {
                                prompt.right_word();
                            } else {
                                prompt.right_char();
                            }
                            KeyCode::Backspace => prompt.backspace(),
                            // TODO: delete current character by KeyCode::Delete
                            // TODO: delete word by Ctrl+W
                            KeyCode::Tab => {
                                if let Some((prefix, &[])) = parse_command(prompt.before_cursor()) {
                                    let prefix = prefix.iter().collect::<String>();
                                    let rest = prompt.after_cursor().iter().collect::<String>();
                                    if let Some(command) = COMMANDS.iter().find(|command| command.name.starts_with(&prefix)) {
                                        // TODO: tab autocompletion should scroll through different
                                        // variants on each TAB press
                                        prompt.clear();
                                        prompt.insert('/');
                                        prompt.insert_str(command.name);
                                        prompt.insert_str(&rest);
                                        prompt.cursor = command.name.len() + 1;
                                    }
                                }
                            }
                            KeyCode::Enter => {
                                if let Some((name, argument)) = parse_command(&prompt.buffer) {
                                    let name = name.iter().collect::<String>();
                                    let argument = argument.iter().collect::<String>();
                                    if let Some(command) = find_command(&name) {
                                        (command.run)(&mut client, &argument);
                                    } else {
                                        chat_error!(&mut client.chat, "Unknown command `/{name}`");
                                    }
                                } else {
                                    if let Some(ref mut stream) = &mut client.stream {
                                        let prompt = prompt.buffer.iter().collect::<String>();
                                        stream.write(prompt.as_bytes())?;
                                        // TODO: don't display the message if it was not delivered
                                        // Maybe the server should actually send your own message back.
                                        // Not sending it back made sense in the telnet times.
                                        chat_msg!(&mut client.chat, "{text}", text = &prompt);
                                    } else {
                                        chat_info!(&mut client.chat, "You are offline. Use /connect <ip> to connect to a server.");
                                    }
                                }
                                prompt.clear();
                            }
                            _ => {},
                        }
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
        // TODO: scrolling the prompt so the cursor is always visible
        for x in prompt.buffer.get(0..(w - 2) as usize).unwrap_or(&prompt.buffer) {
            stdout.queue(Print(x))?;
        }
        stdout.queue(MoveTo(prompt.cursor as u16, h-1))?;

        // TODO: mouse selection does not work
        stdout.flush()?;

        thread::sleep(Duration::from_millis(33));
    }
    Ok(())
}
