/// CLI tool for Stress Testing 4at
use std::net::TcpStream;
use std::env;
use std::result;
use std::process::ExitCode;
use std::io::Write;
use getrandom::getrandom;
use std::thread;
use std::time::Duration;

type Result<T> = result::Result<T, ()>;

struct Command {
    name: &'static str,
    description: &'static str,
    run: fn(command_name: &str, args: &mut env::Args) -> Result<()>,
}

fn command_dragon(command_name: &str, args: &mut env::Args) -> Result<()> {
    let address = args.next().ok_or_else(|| {
        eprintln!("Usage: {command_name} <address> [token]");
        eprintln!("ERROR: no address is provided. Example: 127.0.0.1:6969");
    })?;

    let token = args.next();

    let mut server = TcpStream::connect(&address).map_err(|err| {
        eprintln!("ERROR: could not connect to {address}: {err}");
    })?;

    if let Some(token) = token {
        println!("INFO: Sending token...");
        write!(&server, "{token}").map_err(|err| {
            eprintln!("ERROR: could not authorize with the token: {err}");
        })?;
    }

    // TODO: we should not need this sleep if we just had a properly
    // defined protocol that specifies message separators
    thread::sleep(Duration::from_millis(100));

    const DRAGON_BUFFER_SIZE: usize = 1024;
    let mut buffer = vec![0; DRAGON_BUFFER_SIZE];
    loop {
        let _ = getrandom(&mut buffer).map_err(|err| {
            eprintln!("ERROR: could not generate random data: {err}");
        })?;

        let n = server.write(&buffer).map_err(|err| {
            eprintln!("ERROR: could not write to {address}: {err}");
        })?;


        if n == 0 {
            eprintln!("INFO: {address} closed the connection");
            break;
        }

        eprintln!("INFO: sent {n} bytes to {address}");
    }
    Ok(())
}

fn command_hydra(command_name: &str, args: &mut env::Args) -> Result<()> {
    let address = args.next().ok_or_else(|| {
        eprintln!("Usage: {command_name} <address>");
        eprintln!("ERROR: no address is provided. Example: 127.0.0.1:6969");
    })?;
    let mut conns = Vec::new();
    loop {
        match TcpStream::connect(&address) {
            Ok(conn) => {
                let local_addr = conn.local_addr().map_err(|err| {
                    eprintln!("ERROR: could not get local address of connection to {address}: {err}");
                })?;
                conns.push(conn);
                eprintln!("INFO: connected to {local_addr}. Opened {n} connections", n = conns.len());
            }
            Err(err) => {
                eprintln!("ERROR: could not create another connection to {address}: {err}");
                return Err(());
            }
        }
    }
}

fn command_gnome(command_name: &str, args: &mut env::Args) -> Result<()> {
    let address = args.next().ok_or_else(|| {
        eprintln!("Usage: {command_name} <address>");
        eprintln!("ERROR: no address is provided. Example: 127.0.0.1:6969");
    })?;
    loop {
        let conn = TcpStream::connect(&address).map_err(|err| {
            eprintln!("ERROR: could not create another connection: {err}");
        })?;
        let local_addr = conn.local_addr().map_err(|err| {
            eprintln!("ERROR: could not get local address of connection to {address}: {err}");
        })?;
        eprintln!("INFO: connected to {local_addr}. Disconnecting...");
    }
}

const COMMANDS: &[Command] = &[
    Command {
        name: "dragon",
        description: "Just connects and sends a lot of random data",
        run: command_dragon,
    },
    Command {
        name: "hydra",
        description: "Opens as many connections as possible",
        run: command_hydra,
    },
    Command {
        name: "gnome",
        description: "Keeps opening and closing connections",
        run: command_gnome,
    },
];

fn usage(program: &str) {
    eprintln!("Usage: {program} <command>");
    eprintln!("Commands:");
    for Command{name, description, ..} in COMMANDS.iter() {
        eprintln!("    {name} - {description}");
    }
}

fn main() -> ExitCode {
    let mut args = env::args();
    let program = args.next().expect("program");
    if let Some(command_name) = args.next() {
        if let Some(command) = COMMANDS.iter().find(|command| command.name == command_name) {
            match (command.run)(&command_name, &mut args) {
                Ok(()) => ExitCode::SUCCESS,
                Err(()) => ExitCode::FAILURE,
            }
        } else {
            usage(&program);
            eprintln!("ERROR: Unknown command {command_name}");
            ExitCode::FAILURE
        }
    } else {
        usage(&program);
        eprintln!("ERROR: No subcommand is provided");
        ExitCode::FAILURE
    }
}
