use core::ffi::c_void;

use windows::{
    Win32::Foundation::HANDLE,
    Win32::System::Diagnostics::Debug::ReadProcessMemory,
};

pub trait MemorySource {
    /// Read up to `len` bytes, and return `Option<u8>` to represent what bytes are available in the range.
    fn _read_memory(&self, address: u64, len: usize) -> Result<Vec<Option<u8>>, String>;

    /// Read up to `len` bytes, and stop at the first failure.
    fn read_raw_memory(&self, address: u64, len: usize) -> Vec<u8>;
}

/// Reads up to `max_count` items
pub fn read_memory_array<T: Sized + Default>(
    source: &dyn MemorySource,
    address: u64,
    max_count: usize,
) -> Vec<T> {
    let element_size = ::core::mem::size_of::<T>();
    let max_bytes = max_count * element_size;
    let raw_bytes = source.read_raw_memory(address, max_bytes);
    let mut data = Vec::<T>::new();
    let mut offset: usize = 0;
    while offset + element_size <= raw_bytes.len() {
        let mut item = T::default();
        let dst: *mut u8 = &mut item as *mut T as *mut u8;
        let src = &raw_bytes[offset] as *const u8;
        unsafe { std::ptr::copy_nonoverlapping(src, dst, element_size) };
        data.push(item);
        offset += element_size;
    }
    data
}

/// Reads exactly `count` items, or returns an error.
pub fn read_memory_full_array<T: Sized + Default>(
    source: &dyn MemorySource,
    address: u64,
    count: usize,
) -> Result<Vec<T>, &'static str> {
    let arr = read_memory_array(source, address, count);
    if arr.len() != count {
        Err("Could not read all items")
    } else {
        Ok(arr)
    }
}

pub fn read_memory_data<T: Sized + Default + Copy>(
    source: &dyn MemorySource,
    address: u64,
) -> T {
    let data = read_memory_array::<T>(source, address, 1);
    data[0]
}

/// Read a null-terminated string from memory.
pub fn read_memory_string(
    source: &dyn MemorySource,
    address: u64,
    max_count: usize,
    is_wide: bool,
) -> String {
    if is_wide {
        let mut words = read_memory_array::<u16>(source, address, max_count);
        let maybe_null_pos = words.iter().position(|&v| v == 0);
        if let Some(null_pos) = maybe_null_pos {
            words.truncate(null_pos);
        }
        String::from_utf16_lossy(&words)
    } else {
        let mut bytes = read_memory_array::<u8>(source, address, max_count);
        let maybe_null_pos = bytes.iter().position(|&v| v == 0);
        if let Some(null_pos) = maybe_null_pos {
            bytes.truncate(null_pos);
        }
        // TODO: this is not quite right. Technically most strings read here are encoded as ASCII.
        String::from_utf8(bytes).unwrap()
    }
}

/// Reads a string whose address is at `address`.
pub fn read_memory_string_indirect(
    source: &dyn MemorySource,
    address: u64,
    max_count: usize,
    is_wide: bool,
) -> String {
    let string_addr = read_memory_data::<u64>(source, address);
    read_memory_string(source, string_addr, max_count, is_wide)
}

// Could have other memory sources in the future, like for dump files.
struct LiveMemorySource {
    process: HANDLE,
}

pub fn make_live_memory_source(process: HANDLE) -> Box<dyn MemorySource> {
    Box::new(LiveMemorySource { process })
}

impl MemorySource for LiveMemorySource {
    fn _read_memory(&self, address: u64, len: usize) -> Result<Vec<Option<u8>>, String> {
        let mut buffer: Vec<u8> = vec![0; len];
        let mut data: Vec<Option<u8>> = vec![None; len];
        let mut offset: usize = 0;

        while offset < len {
            let mut bytes_read: usize = 0;
            let len_left = len - offset;
            let cur_address = address + (offset as u64);

            let result = unsafe {
                ReadProcessMemory(
                    self.process,
                    cur_address as *const c_void,
                    buffer.as_mut_ptr() as *mut c_void,
                    len_left,
                    Some(&mut bytes_read as *mut usize),
                )
            };
            result.unwrap_or_else(|error| panic!("ReadProcessMemory failed: {error}"));

            #[allow(clippy::needless_range_loop)]
            for index in 0..bytes_read {
                let data_index = offset + index;
                data[data_index] = Some(buffer[index]);
            }

            if bytes_read > 0 {
                offset += bytes_read;
            } else {
                // TODO: is this the right way to handle reading 0 bytes?
                offset += 1;
            }
        }

        Ok(data)
    }

    fn read_raw_memory(&self, address: u64, len: usize) -> Vec<u8> {
        let mut buffer: Vec<u8> = vec![0; len];
        let mut bytes_read: usize = 0;

        let result = unsafe {
            ReadProcessMemory(
                self.process,
                address as *const c_void,
                buffer.as_mut_ptr() as *mut c_void,
                len,
                Some(&mut bytes_read as *mut usize),
            )
        };

        if result.is_err() {
            bytes_read = 0;
        }

        buffer.truncate(bytes_read);
        buffer
    }
}