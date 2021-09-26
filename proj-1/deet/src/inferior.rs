use crate::dwarf_data::{DwarfData, Line};
use nix::sys::ptrace;
use nix::sys::signal;
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::Pid;
use std::collections::HashMap;
use std::mem::size_of;
use std::os::unix::process::CommandExt;
use std::process::Child;
use std::process::Command;

pub enum Status {
    /// Indicates inferior stopped. Contains the signal that stopped the process, as well as the
    /// current instruction pointer that it is stopped at.
    Stopped(signal::Signal, usize),

    /// Indicates inferior exited normally. Contains the exit status code.
    Exited(i32),

    /// Indicates the inferior exited due to a signal. Contains the signal that killed the
    /// process.
    Signaled(signal::Signal),
}

/// This function calls ptrace with PTRACE_TRACEME to enable debugging on a process. You should use
/// pre_exec with Command to call this in the child process.
fn child_traceme() -> Result<(), std::io::Error> {
    ptrace::traceme().or(Err(std::io::Error::new(
        std::io::ErrorKind::Other,
        "ptrace TRACEME failed",
    )))
}

fn align_addr_to_word(addr: usize) -> usize {
    addr & (-(size_of::<usize>() as isize) as usize)
}

pub struct Inferior {
    child: Child,
    breakpoints_original_instr: HashMap<usize, u8>,
}

impl Inferior {
    /// Attempts to start a new inferior process. Returns Some(Inferior) if successful, or None if
    /// an error is encountered.
    pub fn new(target: &str, args: &Vec<String>, breakpoints: &Vec<usize>) -> Option<Inferior> {
        let mut cmd = Command::new(target);
        cmd.args(args);
        unsafe {
            cmd.pre_exec(child_traceme);
        }
        let mut inf = Inferior {
            child: cmd.spawn().ok()?,
            breakpoints_original_instr: HashMap::new(),
        };
        match inf.wait(None).ok()? {
            Status::Stopped(signal, _) => {
                if signal == signal::SIGTRAP {
                    // Install breakpoints here.
                    for addr in breakpoints {
                        inf.set_breakpoint(*addr);
                    }
                    Some(inf)
                } else {
                    None
                }
            }
            _other => None,
        }
    }

    /// Returns the pid of this inferior.
    pub fn pid(&self) -> Pid {
        nix::unistd::Pid::from_raw(self.child.id() as i32)
    }

    /// Kills the child process if running.
    pub fn kill(&mut self) {
        if let Ok(()) = self.child.kill() {
            println!("Killing running inferior (pid {})", self.pid());
            self.wait(None)
                .expect("Child process is supposed to be exited successfully");
        }
    }

    /// Calls waitpid on this inferior and returns a Status to indicate the state of the process
    /// after the waitpid call.
    pub fn wait(&self, options: Option<WaitPidFlag>) -> Result<Status, nix::Error> {
        Ok(match waitpid(self.pid(), options)? {
            WaitStatus::Exited(_pid, exit_code) => Status::Exited(exit_code),
            WaitStatus::Signaled(_pid, signal, _core_dumped) => Status::Signaled(signal),
            WaitStatus::Stopped(_pid, signal) => {
                let regs = ptrace::getregs(self.pid())?;
                Status::Stopped(signal, regs.rip as usize)
            }
            other => panic!("waitpid returned unexpected status: {:?}", other),
        })
    }

    pub fn cont(&mut self) -> Result<Status, nix::Error> {
        // Check if the child process stopped at the breakpoint
        let mut registers = ptrace::getregs(self.pid())?;
        let rip_addr = registers.rip as usize;
        let orig_instr = self.breakpoints_original_instr.get(&(rip_addr - 1));
        if orig_instr.is_some() {
            let instr = *orig_instr.unwrap();
            // Put back the original instr.
            match self.write_byte(rip_addr - 1, instr) {
                Err(err) => println!("Failed to rewind the register. {}", err),
                _ => {}
            };
            // Rewind the rip pointer back 1.
            registers.rip = (rip_addr as u64) - 1;
            ptrace::setregs(self.pid(), registers).expect("Failed to rewind the rip register");

            // Only execute the next instruction.
            ptrace::step(self.pid(), None)?;
            match self.wait(None) {
                Ok(status) => {
                    match status {
                        // Process exited.
                        Status::Exited(_) => return Ok(status),
                        Status::Signaled(signal) => println!("Signaled {}", signal),
                        Status::Stopped(signal, _rip) => {
                            if signal == signal::Signal::SIGTRAP {
                                // Restore the breakpoint at (rip_addr - 1).
                                self.set_breakpoint(rip_addr - 1);
                            } else {
                                panic!("failed to go to the next instruction. signal: {}", signal);
                            }
                        }
                    };
                }
                Err(err) => panic!("wait returned unexpected status: {:?}", err),
            };

            // Resume the rest of execution.
        }

        ptrace::cont(self.pid(), None)?;
        self.wait(None)
    }

    pub fn print_backtrace(&self, debug_data: &DwarfData) -> Result<(), nix::Error> {
        let mut instruction_ptr = ptrace::getregs(self.pid())?.rip as usize;
        let mut base_ptr = ptrace::getregs(self.pid())?.rbp as usize;

        loop {
            let line = debug_data
                .get_line_from_addr(instruction_ptr)
                .unwrap_or(Line {
                    file: "undefined".to_string(),
                    number: 0,
                    address: instruction_ptr,
                });
            let function = debug_data
                .get_function_from_addr(instruction_ptr)
                .unwrap_or("undefined".to_string());

            println!("{} ({}:{})", function, line.file, line.number);
            if function == "main" {
                break;
            }

            instruction_ptr =
                ptrace::read(self.pid(), (base_ptr + 8) as ptrace::AddressType)? as usize;
            base_ptr = ptrace::read(self.pid(), base_ptr as ptrace::AddressType)? as usize;
        }
        Ok(())
    }

    /// Set the breakpoint if the child process is running.
    pub fn set_breakpoint(&mut self, addr: usize) {
        match self.child.try_wait() {
            // Only when the child process is running, set the breakpoint.
            Ok(None) => {
                match self.write_byte(addr, 0xcc) {
                    Ok(orig_instr) => {
                        // If the address is not stored in the breakpoints_original_instr hashmap,
                        // store the original instruction.
                        if !self.breakpoints_original_instr.contains_key(&addr) {
                            self.breakpoints_original_instr.insert(addr, orig_instr);
                        }
                    }
                    Err(err) => println!("Failed to set the breakpoint at {}. Err: {}", addr, err),
                };
            }
            // If the child process is not running, do nothing.
            _ => {}
        };
    }

    fn write_byte(&mut self, addr: usize, val: u8) -> Result<u8, nix::Error> {
        let aligned_addr = align_addr_to_word(addr);
        let byte_offset = addr - aligned_addr;
        let word = ptrace::read(self.pid(), aligned_addr as ptrace::AddressType)? as u64;
        let orig_byte = (word >> 8 * byte_offset) & 0xff;
        let masked_word = word & !(0xff << 8 * byte_offset);
        let updated_word = masked_word | ((val as u64) << 8 * byte_offset);
        ptrace::write(
            self.pid(),
            aligned_addr as ptrace::AddressType,
            updated_word as *mut std::ffi::c_void,
        )?;
        Ok(orig_byte as u8)
    }
}
