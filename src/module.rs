
use std::{
    fs::File,
    mem::size_of,
};

use windows::Win32::System::{
    SystemServices::*,
    Diagnostics::Debug::*,
};

use pdb::PDB;

use crate::memory::{*, self};

type ModuleName = String;
type PdbName = String;
type PdbLoadError = String;

pub struct Module {
    pub name: String,
    pub address: u64,
    pub size: u64,
    pub exports: Vec::<Export>,
    #[allow(dead_code)]
    pub pdb_name: Option<String>,
    #[allow(dead_code)]
    pub pdb_info: Option<PdbInfo>,
    pub pdb: Result<PDB<'static, File>, PdbLoadError>,
}

pub struct Export {
    pub name: Option<String>,
    /// This is the "biased" ordinal.
    pub ordinal: u32,
    pub target: ExportTarget,
}

impl ToString for Export {
    fn to_string(&self) -> String {
        if let Some(str) = &self.name {
            str.to_string()
        } else {
            format!("Ordinal{}", self.ordinal)
        }
    }
}

pub enum ExportTarget {
    /// Relative Virtual Address
    RVA(u64),

    /// Will be forwarded to the export in a target DLL
    /// Explanation: https://devblogs.microsoft.com/oldnewthing/20060719-24/?p=30473
    #[allow(dead_code)]
    Forwarder(String),
}

#[derive(Copy, Clone, Debug)]
#[repr(C)]
pub struct PdbInfo {
    pub signature: u32,
    pub guid: windows::core::GUID,
    pub age: u32,
    // Null terminated name goes after the end.
}

impl Default for PdbInfo {
    fn default() -> Self {
        unsafe { ::core::mem::zeroed() }
    }
}

impl Module {
    pub fn from_memory_view(
        module_address: u64,
        module_name: Option<String>,
        memory_source: &dyn MemorySource,
    ) -> Result<Module, String> {
        let dos_header: IMAGE_DOS_HEADER = memory::read_memory_data(memory_source, module_address);

        // TODO: We assume that the headers are accurate, even if it means we could read outside the bounds of the module.
        //       Ideally this would do a bounds check.
        let pe_header_addr = module_address + dos_header.e_lfanew as u64;

        // TODO: This should be `IMAGE_NT_HEADERS32` on x86 processes.
        let pe_header: IMAGE_NT_HEADERS64 = memory::read_memory_data(memory_source, pe_header_addr);

        let (pdb_info, pdb_name, pdb) = Module::read_debug_info(&pe_header, module_address, memory_source);
        let (exports, export_table_module_name) = Module::read_exports(&pe_header, module_address, memory_source)?;

        let module_name = module_name
            .or(export_table_module_name)
            .unwrap_or_else(|| format!("module_{module_address:X}"));

        Ok(Module {
            name: module_name,
            address: module_address,
            size: pe_header.OptionalHeader.SizeOfImage as u64,
            exports,
            pdb_name,
            pdb_info,
            pdb,
        })
    }

    pub fn contains_address(&self, address: u64) -> bool {
        let end = self.address + self.size;
        self.address <= address && address < end
    }

    fn read_debug_info(
        pe_header: &IMAGE_NT_HEADERS64,
        module_address: u64,
        memory_source: &dyn MemorySource,
    ) -> (Option<PdbInfo>, Option<PdbName>, Result<PDB<'static, File>, PdbLoadError>) {
        let mut pdb_info_result: Option<PdbInfo> = None;
        let mut pdb_name_result: Option<PdbName> = None;
        let mut pdb_result: Result<PDB<File>, PdbLoadError> = Err(String::from("No matching PDB"));

        let debug_table_info = pe_header.OptionalHeader.DataDirectory[IMAGE_DIRECTORY_ENTRY_DEBUG.0 as usize];
        if debug_table_info.VirtualAddress != 0 {
            let dir_size = size_of::<IMAGE_DEBUG_DIRECTORY>() as u64;
            // We'll arbitrarily limit to 20 entries to keep it sane.
            let count = std::cmp::min(debug_table_info.Size as u64 / dir_size, 20);
            for dir_index in 0..count {
                let debug_dir_addr = module_address + (debug_table_info.VirtualAddress as u64) + (dir_index * dir_size);
                let debug_dir: IMAGE_DEBUG_DIRECTORY = memory::read_memory_data(memory_source, debug_dir_addr);
                if debug_dir.Type == IMAGE_DEBUG_TYPE_CODEVIEW {
                    let pdb_info_addr = module_address + debug_dir.AddressOfRawData as u64;
                    let pdb_info: PdbInfo = memory::read_memory_data(memory_source, pdb_info_addr);
                    // TODO: verify that `pdb_info.signature` is `RSDS`.
                    let pdb_name_addr = pdb_info_addr + size_of::<PdbInfo>() as u64;
                    let pdb_name_max_size = debug_dir.SizeOfData as usize - size_of::<PdbInfo>();
                    let pdb_name = memory::read_memory_string(memory_source, pdb_name_addr, pdb_name_max_size, false);

                    // TODO: Attempt to download the symbols from a symbol server or symbol cache.
                    //       For now, assume that the name points to an absolute path on disk.
                    pdb_result = match File::open(&pdb_name) {
                        Ok(pdb_file) => {
                            match PDB::open(pdb_file) {
                                Ok(pdb_data) => {
                                    Ok(pdb_data)
                                }
                                Err(err) => {
                                    Err(err.to_string())
                                }
                            }
                        }
                        Err(err) => {
                            Err(err.to_string())
                        }
                    };

                    pdb_info_result = Some(pdb_info);
                    pdb_name_result = Some(pdb_name);
                }
            }
        }

        (pdb_info_result, pdb_name_result, pdb_result)
    }

    fn read_exports(
        pe_header: &IMAGE_NT_HEADERS64,
        module_address: u64,
        memory_source: &dyn MemorySource,
    ) -> Result<(Vec::<Export>, Option<ModuleName>), &'static str> {
        let mut exports = Vec::<Export>::new();
        let mut module_name: Option<ModuleName> = None;

        let export_table_info = pe_header.OptionalHeader.DataDirectory[IMAGE_DIRECTORY_ENTRY_EXPORT.0 as usize];
        if export_table_info.VirtualAddress != 0 {
            let export_table_addr = module_address + export_table_info.VirtualAddress as u64;
            let export_table_end = export_table_addr + export_table_info.Size as u64;
            let export_directory: IMAGE_EXPORT_DIRECTORY = memory::read_memory_data(memory_source, export_table_addr);

            // This is a fallback that lets us find a name if none was available.
            if export_directory.Name != 0 {
                let name_addr = module_address + export_directory.Name as u64;
                module_name = Some(memory::read_memory_string(memory_source, name_addr, 512, false));
            }

            // Read the name table first, which is essentially a list of (ordinal, name) pairs that give names
            // to some or all of the exports. The table is stored as parallel arrays of ordinals and name pointers.
            let ordinal_array_addr = module_address + export_directory.AddressOfNameOrdinals as u64;
            let ordinal_array = memory::read_memory_full_array::<u16>(memory_source, ordinal_array_addr, export_directory.NumberOfNames as usize)?;
            let name_array_addr = module_address + export_directory.AddressOfNames as u64;
            let name_array = memory::read_memory_full_array::<u32>(memory_source, name_array_addr, export_directory.NumberOfNames as usize)?;

            let address_table_addr = module_address + export_directory.AddressOfFunctions as u64;
            let address_table = memory::read_memory_full_array::<u32>(memory_source, address_table_addr, export_directory.NumberOfFunctions as usize)?;

            for (unbiased_ordinal, function_addr) in address_table.iter().enumerate() {
                let ordinal = export_directory.Base + unbiased_ordinal as u32;
                let target_addr = module_address + *function_addr as u64;

                let name_index = ordinal_array.iter().position(|&o| o == unbiased_ordinal as u16);
                let export_name = name_index.and_then(|idx| {
                    let name_addr = module_address + name_array[idx] as u64;
                    Some(memory::read_memory_string(memory_source, name_addr, 4096, false))
                });

                // An address that falls inside the export directory is actually a forwarder.
                let target = if target_addr >= export_table_addr && target_addr < export_table_end {
                    // Unsure if there is a max size for a forwarder name, but 4K is probably reasonable.
                    let forwarding_name = memory::read_memory_string(memory_source, target_addr, 4096, false);
                    ExportTarget::Forwarder(forwarding_name)
                } else {
                    ExportTarget::RVA(target_addr)
                };
                exports.push(Export { name: export_name, ordinal, target });
            }
        }

        Ok((exports, module_name))
    }
}