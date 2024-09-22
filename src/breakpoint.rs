use std::fmt;

use crate::{
    name_resolution,
    process::Process,
};

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct BreakpointId(pub u32);

impl fmt::Display for BreakpointId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

struct Breakpoint {
    id: BreakpointId,
    address: u64,
}

pub struct BreakpointManager {
    // TODO: determine if it's more performant to be a HashMap instead.
    breakpoints: Vec::<Breakpoint>,
}

impl BreakpointManager {
    pub fn new() -> BreakpointManager {
        BreakpointManager {
            breakpoints: Vec::new(),
        }
    }

    fn get_free_id(&self) -> BreakpointId {
        for potential_id in 0..1024 {
            if !self.breakpoints.iter().any(|x| x.id.0 == potential_id) {
                return BreakpointId(potential_id);
            }
        }
        panic!("Too many breakpoints!")
    }

    pub fn add_breakpoint(&mut self, address: u64) {
        let id = self.get_free_id();
        self.breakpoints.push(Breakpoint { id, address });
        self.breakpoints.sort_by(|a, b| a.id.cmp(&b.id));
    }

    pub fn remove_breakpoint(&mut self, id: BreakpointId) {
        self.breakpoints.retain(|x| x.id != id);
    }

    pub fn list_breakpoints(&self, process: &mut Process) {
        for breakpoint in self.breakpoints.iter() {
            if let Some(symbol) = name_resolution::resolve_address_to_name(breakpoint.address, process) {
                println!("{:3} {:#018x} ({symbol})", breakpoint.id, breakpoint.address);
            } else {
                println!("{:3} {:#018x}", breakpoint.id, breakpoint.address);
            }
        }
    }
}