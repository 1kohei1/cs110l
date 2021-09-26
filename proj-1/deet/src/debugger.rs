use crate::debugger_command::DebuggerCommand;
use crate::dwarf_data::{DwarfData, Error as DwarfError};
use crate::inferior::Inferior;
use crate::inferior::Status;
use rustyline::error::ReadlineError;
use rustyline::Editor;

pub struct Debugger {
    target: String,
    history_path: String,
    readline: Editor<()>,
    inferior: Option<Inferior>,
    dwarf_data: DwarfData,
    breakpoints: Vec<usize>,
}

fn parse_address(addr: &str) -> Option<usize> {
    let addr_without_0x = if addr.to_lowercase().starts_with("0x") {
        &addr[2..]
    } else {
        &addr
    };
    usize::from_str_radix(addr_without_0x, 16).ok()
}

impl Debugger {
    /// Initializes the debugger.
    pub fn new(target: &str) -> Debugger {
        let dwarf_data = match DwarfData::from_file(target) {
            Ok(val) => val,
            Err(DwarfError::ErrorOpeningFile) => {
                println!("Could not open file {}", target);
                std::process::exit(1);
            }
            Err(DwarfError::DwarfFormatError(err)) => {
                println!("Could not debugging symbols {}: {:?}", target, err);
                std::process::exit(1);
            }
        };

        let history_path = format!("{}/.deet_history", std::env::var("HOME").unwrap());
        let mut readline = Editor::<()>::new();
        // Attempt to load history from ~/.deet_history if it exists
        let _ = readline.load_history(&history_path);

        Debugger {
            target: target.to_string(),
            history_path,
            readline,
            inferior: None,
            dwarf_data,
            breakpoints: Vec::new(),
        }
    }

    fn print_inferior_run_result(&self, result: Result<Status, nix::Error>) {
        match result {
            Ok(status) => {
                match status {
                    Status::Stopped(signal, rip) => {
                        println!("Child stopped (signal {})", signal);
                        if let Some(line) = &self.dwarf_data.get_line_from_addr(rip) {
                            println!("Stopped at {}:{}", line.file, line.number);
                        }
                    }
                    Status::Exited(code) => {
                        println!("Child exited (status {})", code)
                    }
                    Status::Signaled(signal) => println!("Signaled {}", signal),
                };
            }
            Err(err) => println!("Error continuing the program. {}", err),
        }
    }

    pub fn run(&mut self) {
        loop {
            match self.get_next_command() {
                DebuggerCommand::Run(args) => {
                    // If run command is executed while a child process is running (this
                    // happens when child process is paused by Ctrl-C and r/run command is entered
                    // to DEET.
                    if self.inferior.is_some() {
                        self.inferior.as_mut().unwrap().kill();
                    }
                    if let Some(inferior) = Inferior::new(&self.target, &args) {
                        // Create the inferior
                        self.inferior = Some(inferior);
                        let result = self.inferior.as_mut().unwrap().cont();
                        self.print_inferior_run_result(result);
                    } else {
                        println!("Error starting subprocess");
                    }
                }
                DebuggerCommand::Cont => {
                    match &self.inferior {
                        Some(inf) => self.print_inferior_run_result(inf.cont()),
                        None => println!("No child process under debugging"),
                    };
                }
                DebuggerCommand::Backtrace => {
                    match &self.inferior {
                        Some(inf) => {
                            inf.print_backtrace(&self.dwarf_data).ok();
                        }
                        None => println!("No child process under debugging"),
                    };
                }
                DebuggerCommand::BreakPoint(breakpoint) => {
                    println!(
                        "Set breakpoint {} at {}",
                        self.breakpoints.len(),
                        breakpoint
                    );

                    let addr = parse_address(&breakpoint);
                    if addr.is_none() {
                        println!("Failed to parse a breakpoint");
                        return;
                    }
                    self.breakpoints.push(addr.unwrap());
                }
                DebuggerCommand::Quit => {
                    if self.inferior.is_some() {
                        self.inferior.as_mut().unwrap().kill();
                    }
                    return;
                }
            }
        }
    }

    /// This function prompts the user to enter a command, and continues re-prompting until the user
    /// enters a valid command. It uses DebuggerCommand::from_tokens to do the command parsing.
    ///
    /// You don't need to read, understand, or modify this function.
    fn get_next_command(&mut self) -> DebuggerCommand {
        loop {
            // Print prompt and get next line of user input
            match self.readline.readline("(deet) ") {
                Err(ReadlineError::Interrupted) => {
                    // User pressed ctrl+c. We're going to ignore it
                    println!("Type \"quit\" to exit");
                }
                Err(ReadlineError::Eof) => {
                    // User pressed ctrl+d, which is the equivalent of "quit" for our purposes
                    return DebuggerCommand::Quit;
                }
                Err(err) => {
                    panic!("Unexpected I/O error: {:?}", err);
                }
                Ok(line) => {
                    if line.trim().len() == 0 {
                        continue;
                    }
                    self.readline.add_history_entry(line.as_str());
                    if let Err(err) = self.readline.save_history(&self.history_path) {
                        println!(
                            "Warning: failed to save history file at {}: {}",
                            self.history_path, err
                        );
                    }
                    let tokens: Vec<&str> = line.split_whitespace().collect();
                    if let Some(cmd) = DebuggerCommand::from_tokens(&tokens) {
                        return cmd;
                    } else {
                        println!("Unrecognized command.");
                    }
                }
            }
        }
    }
}
