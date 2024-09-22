use crate::{
    name_resolution,
    process::Process,
};

struct Breakpoint {
    address: u64,
}

pub struct BreakpointManager {
    // TODO: determine if it's better to use a HashMap instead.
    breakpoints: Vec::<Breakpoint>,
}

impl BreakpointManager {
    pub fn new() -> BreakpointManager {
        BreakpointManager {
            breakpoints: Vec::new(),
        }
    }

    pub fn add_breakpoint(&mut self, address: u64) {
        self.breakpoints.push(Breakpoint { address });
    }

    pub fn remove_breakpoint(&mut self, address: u64) {
        self.breakpoints.retain(|x| x.address != address);
    }

    pub fn list_breakpoints(&self, process: &mut Process) {
        for breakpoint in self.breakpoints.iter() {
            if let Some(symbol) = name_resolution::resolve_address_to_name(breakpoint.address, process) {
                println!("{:#018x} ({symbol})", breakpoint.address);
            } else {
                println!("{:#018x}", breakpoint.address);
            }
        }
    }
}