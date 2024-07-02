use ntp::NtpSync;
use std::{env, process::exit};

mod ntp;
mod util;

enum ExitCode {
    Success = 0x0,
    InvalidArguments = 0x1,
    NtpError = 0x2,
}

fn print_usage_and_exit(args: &[String]) {
    eprintln!("Usage : {} <server>", args[0]);
    exit(ExitCode::InvalidArguments as i32);
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Invalid number of arguments");
        print_usage_and_exit(&args);
    }

    let server = &args[1];

    let mut ntp = match NtpSync::new(server.as_str()) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("Failed to initialize the SNTP client");
            eprintln!("{}", e);
            exit(ExitCode::NtpError as i32);
        }
    };

    match ntp.sync() {
        Ok(result) => {
            println!("{}", result);
        }
        Err(e) => {
            eprintln!("Query failed");
            eprintln!("{}", e);
        }
    }

    exit(ExitCode::Success as i32);
}
