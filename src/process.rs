use crate::{
    memory::MemorySource,
    module::Module,
};

pub struct Process {
    module_list: Vec<Module>,
}

impl Process {
    pub fn new() -> Process {
        Process {
            module_list: Vec::new(),
        }
    }

    pub fn add_module(
        &mut self,
        address: u64,
        name: Option<String>,
        memory_source: &dyn MemorySource
    ) -> Result<&Module, String> {
        let module = Module::from_memory_view(address, name, memory_source)?;
        self.module_list.push(module);
        Ok(self.module_list.last().unwrap())
    }

    pub fn _get_containing_module(&self, address: u64) -> Option<&Module> {
        self.module_list.iter().find(|&module| module.contains_address(address))
    }

    pub fn get_containing_module_mut(&mut self, address: u64) -> Option<&mut Module> {
        self.module_list.iter_mut().find(|module| module.contains_address(address))
    }

    pub fn get_module_by_name_mut(&mut self, module_name: &str) -> Option<&mut Module> {
        let mut potential_trimmed_match = None;

        for module in self.module_list.iter_mut() {
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