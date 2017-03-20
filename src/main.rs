extern crate ansi_term;
extern crate rustyline;
extern crate shlex;
extern crate libc;
extern crate errno;
extern crate regex;
extern crate nix;

#[macro_use]
extern crate nom;

use std::env;
use std::os::unix::process::CommandExt;
use std::process::Command;

// use std::thread;
// use std::time::Duration;

use ansi_term::Colour::Red;
use ansi_term::Colour::Green;
use rustyline::Editor;
use rustyline::error::ReadlineError;
use nom::IResult;
use regex::Regex;


mod jobs;
mod tools;
mod parsers;
mod builtins;
mod execute;


fn main() {
    if env::args().len() > 1 {
        println!("does not support args yet.");
        return;
    }

    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    println!("##### Welcome to RUSH v{} #####", VERSION);

    let user = env::var("USER").unwrap();
    let home = env::var("HOME").unwrap();
    let env_path = env::var("PATH").unwrap();
    let dir_bin_cargo = format!("{}/.cargo/bin", home);
    let env_path_new = ["/usr/local/bin".to_string(),
                        env_path,
                        dir_bin_cargo,
                        "/Library/Frameworks/Python.framework/Versions/3.6/bin".to_string(),
                        "/Library/Frameworks/Python.framework/Versions/3.5/bin".to_string(),
                        "/Library/Frameworks/Python.framework/Versions/3.4/bin".to_string(),
                        "/Library/Frameworks/Python.framework/Versions/2.7/bin".to_string()]
            .join(":");
    env::set_var("PATH", &env_path_new);

    let mut previous_dir = String::new();
    let mut proc_status_ok = true;
    let mut painter;
    let mut rl = Editor::<()>::new();
    loop {
        if proc_status_ok {
            painter = Green;
        } else {
            painter = Red;
        }

        let _current_dir = env::current_dir().unwrap();
        let current_dir = _current_dir.to_str().unwrap();
        let _tokens: Vec<&str> = current_dir.split("/").collect();

        let last = _tokens.last().unwrap();
        let pwd: String;
        if last.to_string() == "" {
            pwd = String::from("/");
        } else if current_dir == home {
            pwd = String::from("~");
        } else {
            pwd = last.to_string();
        }
        let prompt = format!("{}@{}: {}$ ",
                             painter.paint(user.to_string()),
                             painter.paint("RUSH"),
                             painter.paint(pwd));
        let cmd = rl.readline(&prompt);
        match cmd {
            Ok(line) => {
                let cmd: String;
                if line.trim() == "exit" {
                    break;
                } else if line.trim() == "" {
                    continue;
                } else if line.trim() == "bash" {
                    cmd = String::from("bash --rcfile ~/.bash_profile");
                } else {
                    cmd = line.to_string();
                }
                rl.add_history_entry(&cmd);

                if Regex::new(r"^ *\(* *[0-9\.]+").unwrap().is_match(line.as_str()) {
                    match parsers::expr(line.as_bytes()) {
                        IResult::Done(_, x) => {
                            println!("{:?}", x);
                        }
                        IResult::Error(x) => println!("Error: {:?}", x),
                        IResult::Incomplete(x) => println!("Incomplete: {:?}", x),
                    }
                    continue;
                }

                let args = shlex::split(cmd.trim()).unwrap();
                if args[0] == "cd" {
                    let result = builtins::cd::run(args.clone(),
                                                   home.as_str(),
                                                   current_dir,
                                                   &mut previous_dir);
                    proc_status_ok = result == 0;
                    continue;
                } else if args.iter().any(|x| x == "|") {
                    let result = execute::run_pipeline(args.clone());
                    proc_status_ok = result == 0;
                    continue;
                }

                tools::rlog(format!("run {:?}\n", args));
                let mut child;
                match Command::new(&args[0])
                          .args(&(args[1..]))
                          .before_exec(|| {
                    unsafe {
                        let pid = libc::getpid();
                        libc::setpgid(0, pid);
                    }
                    Ok(())
                })
                          .spawn() {
                    Ok(x) => child = x,
                    Err(e) => {
                        proc_status_ok = false;
                        println!("{:?}", e);
                        continue;
                    }
                }
                unsafe {
                    let pid = child.id() as i32;
                    let gid = libc::getpgid(pid);
                    tools::rlog(format!("try give term to {}\n", gid));
                    jobs::give_terminal_to(gid);
                    tools::rlog(format!("waiting pid {}\n", gid));
                }
                let ecode = child.wait().unwrap();
                proc_status_ok = ecode.success();
                tools::rlog(format!("done. ok: {}\n", proc_status_ok));
                unsafe {
                    let gid = libc::getpgid(0);
                    tools::rlog(format!("try give term to {}\n", gid));
                    jobs::give_terminal_to(gid);
                }
            }
            Err(ReadlineError::Interrupted) => {
                // Ctrl-C
                continue;
            }
            Err(ReadlineError::Eof) => {
                // Ctrl-D
                continue;
            }
            Err(err) => {
                println!("RL Error: {:?}", err);
                continue;
            }
        }
    }
}