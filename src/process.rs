use crate::{
    memory::MemorySource,
    module::Module,
    windows::ThreadId,
};

pub struct Process {
    modules: Vec<Module>,
    threads: Vec<ThreadId>,
}

impl Process {
    pub fn new() -> Process {
        Process {
            modules: Vec::new(),
            threads: Vec::new(),
        }
    }

    pub fn add_module(
        &mut self,
        address: u64,
        name: Option<String>,
        memory_source: &dyn MemorySource
    ) -> Result<&Module, String> {
        let module = Module::from_memory_view(address, name, memory_source)?;
        self.modules.push(module);
        Ok(self.modules.last().unwrap())
    }

    pub fn add_thread(&mut self, thread: ThreadId) {
        self.threads.push(thread);
    }

    pub fn remove_thread(&mut self, thread: ThreadId) {
        self.threads.retain(|x| *x != thread);
    }

    pub fn _iterate_threads(&self) -> core::slice::Iter<'_, ThreadId> {
        self.threads.iter()
    }

    pub fn _get_containing_module(&self, address: u64) -> Option<&Module> {
        self.modules.iter().find(|&module| module.contains_address(address))
    }

    pub fn get_containing_module_mut(&mut self, address: u64) -> Option<&mut Module> {
        self.modules.iter_mut().find(|module| module.contains_address(address))
    }

    pub fn get_module_by_name_mut(&mut self, module_name: &str) -> Option<&mut Module> {
        let mut potential_trimmed_match = None;

        for module in self.modules.iter_mut() {
            // Exact match
            if module.name == module_name {
                return Some(module);
            }

            // Trimmed match: the file part of the path matches
            // Keep looping even if we find a trimmed match, because an exact match is higher priority.
            if potential_trimmed_match.is_none() {
                let trimmed = module.name.rsplit('\\').next().unwrap_or(&module.name);
                if trimmed.to_lowercase() == module_name.to_lowercase() {
                    potential_trimmed_match = Some(module)
                }
            }
        }

        potential_trimmed_match
    }
}