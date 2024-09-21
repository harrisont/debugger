use pdb::FallibleIterator;

use crate::{
    process::Process,
    module::{
        Export,
        ExportTarget,
        Module,
    },
};

enum AddressMatch<'a> {
    None,
    Export(&'a Export),
    Public(String),
}

impl AddressMatch<'_> {
    fn is_none(&self) -> bool {
        matches!(self, AddressMatch::None)
    }
}

pub fn resolve_name_to_address(symbol: &str, process: &mut Process) -> Result<u64, String> {
    match symbol.chars().position(|c| c == '!') {
        None => {
            // Search all modules
            Err(String::from("Searching all modules for a symbol is not yet implmented"))
        }
        Some(pos) => {
            let module_name = &symbol[..pos];
            let func_name = &symbol[pos + 1..];
            if let Some(module) = process.get_module_by_name_mut(module_name) {
                if let Some(addr) = resolve_function_in_module(module, func_name) {
                    Ok(addr)
                } else {
                    Err(format!("Could not find {func_name} in module {module_name}"))
                }
            } else {
                Err(format!("Could not find module {module_name}"))
            }
        }
    }
}

pub fn resolve_function_in_module(module: &mut Module, func: &str) -> Option<u64> {
    // Search exports first and then private symbols.
    for export in module.exports.iter() {
        if let Some(export_name) = &export.name {
            if *export_name == *func {
                return match export.target {
                    ExportTarget::Rva(export_addr) => Some(export_addr),
                    ExportTarget::Forwarder(_) => todo!(),
                };
            }
        }
    }
    None
}

pub fn resolve_address_to_name(address: u64, process: &mut Process) -> Option<String> {
    let module = match process.get_containing_module_mut(address) {
        Some(module) => module,
        None => return None
    };

    // Do a linear search for the export with the closest address that comes before the address we're looking for.
    // TODO: keep in sorted order to search faster.
    let mut closest: AddressMatch = AddressMatch::None;
    let mut closest_addr: u64 = 0;
    for export in module.exports.iter() {
        if let ExportTarget::Rva(export_addr) = export.target {
            if export_addr <= address && (closest.is_none() || closest_addr < export_addr) {
                closest = AddressMatch::Export(export);
                closest_addr = export_addr;
            }
        }
    }

    // Do a linear search for the symbol in the PDB with the closest address that comes before the address we're looking for.
    // TODO: handle errors.
    if let Ok(pdb) = module.pdb.as_mut() {
        if let Ok(symbol_table) = pdb.global_symbols() {
            if let Ok(address_map) = pdb.address_map() {
                let mut symbols = symbol_table.iter();
                while let Ok(Some(symbol)) = symbols.next() {
                    match symbol.parse() {
                        Ok(pdb::SymbolData::Public(data)) if data.function => {
                            let rva = data.offset.to_rva(&address_map).unwrap_or_default();
                            let global_addr = module.address + rva.0 as u64;
                            if global_addr <= address && (closest.is_none() || closest_addr <= global_addr) {
                                // TODO: Take a reference to the data instead of copying it?
                                closest = AddressMatch::Public(data.name.to_string().to_string());
                                closest_addr = global_addr;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    if let AddressMatch::Export(closest) = closest {
        let offset = address - closest_addr;
        let sym_with_offset = if offset == 0 {
            format!("{}!{}", &module.name, closest)
        } else {
            format!("{}!{}+{:#x}", &module.name, closest, offset)
        };
        return Some(sym_with_offset);
    }

    if let AddressMatch::Public(closest) = closest {
        let offset = address - closest_addr;
        let sym_with_offset = if offset == 0 {
            format!("{}!{}", &module.name, closest)
        } else {
            format!("{}!{}+{:#x}", &module.name, closest, offset)
        };
        return Some(sym_with_offset);
    }

    None
}