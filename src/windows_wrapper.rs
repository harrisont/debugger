use std::{
    ffi::{OsStr, OsString},
    fmt::{self, Debug},
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::Path,
};

use windows::{
    core::{PCWSTR, PWSTR},
    Win32::{
        Foundation::*,
        Storage::FileSystem::*,
        System::{Diagnostics::Debug::*, Threading::*},
    },
};

use crate::memory::{self, MemorySource};
use crate::process::Process;

pub const TRAP_FLAG: u32 = 1 << 8;

pub const EXCEPTION_CODE_SINGLE_STEP: NTSTATUS = EXCEPTION_SINGLE_STEP;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct ThreadId(u32);

impl fmt::Display for ThreadId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl fmt::LowerHex for ThreadId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::LowerHex::fmt(&self.0, f)
    }
}

impl fmt::UpperHex for ThreadId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::UpperHex::fmt(&self.0, f)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct ProcessId(u32);

impl fmt::Display for ProcessId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl fmt::LowerHex for ProcessId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::LowerHex::fmt(&self.0, f)
    }
}

impl fmt::UpperHex for ProcessId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::UpperHex::fmt(&self.0, f)
    }
}

/// Gets the last platform error code and returns an error message containing the code and the message matching the code.
pub fn get_last_platform_error_message() -> String {
    let error_code = unsafe { GetLastError() } ;
    let mut error_message_buffer = Vec::<u16>::with_capacity(1024);
    unsafe {
        let error_message_buffer_uninitialized = error_message_buffer.spare_capacity_mut();
        let message_len = FormatMessageW(
            FORMAT_MESSAGE_FROM_SYSTEM | FORMAT_MESSAGE_IGNORE_INSERTS /*dwFlags*/,
            None /*lpsource*/,
            error_code.0 /*dwMessageId*/,
            0 /*dwLanguageId*/,
            PWSTR::from_raw(error_message_buffer_uninitialized.as_mut_ptr().cast()) /*lpBuffer*/,
            error_message_buffer_uninitialized.len() as u32 /*nSize*/,
            None /*Arguments*/
        );

        error_message_buffer.set_len(message_len as usize);
    }

    let error_message_os = OsString::from_wide(&error_message_buffer);
    let error_message = error_message_os.into_string()
        .unwrap_or(String::from(""));
    let trimmed_error_message = error_message.trim();
    format!("OS error {code}: {trimmed_error_message}", code = error_code.0)
}

/// Converts a `String` into a null-terminated wide (u16) encoded string.
/// This is useful for passing into Windows C APIs.
pub fn convert_string_to_u16(input: &str) -> Vec<u16> {
    let mut buffer: Vec<u16> = OsStr::new(&input)
        .encode_wide()
        .collect();

    // Add the null terminator.
    buffer.push(0);

    buffer
}

pub fn close_handle(handle: HANDLE) {
    let ret = unsafe {
        CloseHandle(handle)
    };
    ret.unwrap_or_else(|error| panic!("CloseHandle failed: {error}"));
}

/// Used to automatically close a handle when dropped.
pub struct AutoClosedHandle(HANDLE);

impl Drop for AutoClosedHandle {
    fn drop(&mut self) {
        close_handle(self.0);
    }
}

impl AutoClosedHandle {
    pub fn handle(&self) -> HANDLE {
        self.0
    }
}

pub fn open_thread(thread_id: &ThreadId) -> AutoClosedHandle {
    let handle = unsafe {
        OpenThread(
            THREAD_GET_CONTEXT | THREAD_SET_CONTEXT /*dwDesiredAccess*/,
            FALSE /*bInheritHandle*/,
            thread_id.0
        )
    };
    match handle {
        Ok(h) => AutoClosedHandle(h),
        Err(error) => panic!("OpenThread failed: {error}"),
    }
}

pub fn launch_process_for_debugging(target_command_line_args: &[String]) -> AutoClosedHandle {
    let target_command_line_buffer = target_command_line_args.join(" ");
    println!("Debugging {target_command_line_buffer}\n");
    let mut target_command_line_buffer_u16 = convert_string_to_u16(&target_command_line_buffer);

    let mut startup_info: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
    startup_info.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
    let mut process_info: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    let ret = unsafe {
        CreateProcessW(
            PCWSTR::null() /*lpApplicationName*/,
            PWSTR(target_command_line_buffer_u16.as_mut_ptr()),
            None /*lpProcessAttributes*/,
            None /*lpThreadAttributes*/,
            FALSE /*bInheritHandles*/,
            DEBUG_ONLY_THIS_PROCESS | CREATE_NEW_CONSOLE /*dwCreationFlags*/,
            None /*lpEnvironment*/,
            PCWSTR::null() /*lpCurrentDirectory*/,
            &startup_info.StartupInfo,
            &mut process_info,
        )
    };
    ret.unwrap_or_else(|error| panic!("Failed to start process \"{target_command_line_buffer}\": CreateProcessW failed: {error}"));

    close_handle(process_info.hThread);

    AutoClosedHandle(process_info.hProcess)
}

// Required because `windows::Win32::System::Diagnostics::Debug::CONTEXT` has a bug where it needs to be aligned but is not.
// The issues is tracked by https://github.com/microsoft/win32metadata/issues/1044
// Once that is fixed this can be deleted and we can use `CONTEXT` direclty.
#[repr(align(16))]
pub struct AlignedContext {
    pub context: CONTEXT,
}

pub fn get_thread_id(thread_handle: HANDLE) -> ThreadId {
    let id = unsafe { GetThreadId(thread_handle) };
    if id == 0 {
        panic!("GetThreadId failed: {}", get_last_platform_error_message());
    }
    ThreadId(id)
}

pub fn get_thread_context(thread: &AutoClosedHandle) -> AlignedContext {
    let mut ctx: AlignedContext = unsafe { std::mem::zeroed() };
    // Currently only supports AMD64 architecture, but could change to make this variable instead of constant.
    ctx.context.ContextFlags = CONTEXT_ALL_AMD64;

    let ret = unsafe { GetThreadContext(thread.handle(), &mut ctx.context) };
    ret.unwrap_or_else(|error| panic!("GetThreadContext failed: {error}"));

    ctx
}

pub fn set_thread_context(thread: &AutoClosedHandle, context: &CONTEXT) {
    let ret = unsafe { SetThreadContext(thread.handle(), context) };
    ret.unwrap_or_else(|error| panic!("SetThreadContext failed: {error}"));
}

pub enum DebugEvent {
    Exception{first_chance: bool, code: NTSTATUS},
    CreateProcess{name: Option<String>, base_addr: u64},
    ExitProcess{exit_code: u32},
    CreateThread,
    ExitThread{exit_code: u32},
    LoadDll{name: Option<String>, base_addr: u64},
    UnloadDll,
    OutputDebugString(String),
    /// System debugging error
    Rip{error: u32, info_type: RIP_INFO_TYPE},
}

pub struct DebugEventContext {
    pub process: ProcessId,
    pub thread: ThreadId,
}

pub fn wait_for_debug_event(mem_source: &dyn MemorySource) -> (DebugEventContext, DebugEvent) {
    let mut event: DEBUG_EVENT = unsafe { std::mem::zeroed() };
    let ret = unsafe {
        WaitForDebugEventEx(&mut event, INFINITE)
    };
    ret.unwrap_or_else(|error| panic!("WaitForDebugEventEx failed: {error}"));

    let context = DebugEventContext{
        process: ProcessId(event.dwProcessId),
        thread: ThreadId(event.dwThreadId),
    };

    match event.dwDebugEventCode {
        EXCEPTION_DEBUG_EVENT => {
            let data = unsafe { event.u.Exception };
            let first_chance = data.dwFirstChance != 0;
            let code: NTSTATUS = data.ExceptionRecord.ExceptionCode;
            (context, DebugEvent::Exception { first_chance, code })
        }
        CREATE_THREAD_DEBUG_EVENT => {
            let data = unsafe { event.u.CreateThread };
            let thread = get_thread_id(data.hThread);
            close_handle(data.hThread);
            assert_eq!(thread, context.thread);
            (context, DebugEvent::CreateThread)
        }
        EXIT_THREAD_DEBUG_EVENT => {
            let data = unsafe { event.u.ExitThread };
            let exit_code = data.dwExitCode;
            (context, DebugEvent::ExitThread { exit_code })
        }
        CREATE_PROCESS_DEBUG_EVENT => {
            let data = unsafe { event.u.CreateProcessInfo };

            let path = get_final_path_name_by_handle(data.hFile);
            // The handle path is the fill path, e.g. `\\?\C:\git\HelloWorld\hello.exe`.
            // It might be useful to have hte full path, but it's not available for all modules in all cases.
            // So instead use the file name.
            let name = Path::new(&path)
                .file_name()
                .map(|name| name.to_string_lossy().to_string());

            let base_addr = data.lpBaseOfImage as u64;

            (context, DebugEvent::CreateProcess { name, base_addr } )
        }
        EXIT_PROCESS_DEBUG_EVENT => {
            let data = unsafe { event.u.ExitProcess };
            let exit_code = data.dwExitCode;
            (context, DebugEvent::ExitProcess { exit_code })
        }
        LOAD_DLL_DEBUG_EVENT => {
            let data = unsafe { event.u.LoadDll };
            let base_addr = data.lpBaseOfDll as u64;

            let name = if data.lpImageName.is_null() {
                None
            } else {
                let is_wide = data.fUnicode != 0;
                Some(memory::read_memory_string_indirect(mem_source, data.lpImageName as u64, 260, is_wide))
            };
            (context, DebugEvent::LoadDll { name, base_addr } )
        }
        UNLOAD_DLL_DEBUG_EVENT => {
            (context, DebugEvent::UnloadDll)
        }
        OUTPUT_DEBUG_STRING_EVENT => {
            let data = unsafe { event.u.DebugString };
            let is_wide = data.fUnicode != 0;
            let address = data.lpDebugStringData.as_ptr() as u64;
            let len = data.nDebugStringLength as usize;
            let debug_string = memory::read_memory_string(mem_source, address, len, is_wide);
            (context, DebugEvent::OutputDebugString(debug_string) )
        }
        RIP_EVENT => {
            let data = unsafe { event.u.RipInfo };
            let error = data.dwError;
            let info_type = data.dwType;
            (context, DebugEvent::Rip { error, info_type } )
        }
        code => panic!("Unexpected debug event {code:?}"),
    }
}

pub enum DebugContinueStatus {
    Continue,
    ExceptionNotHandled,
    //ReplyLater,
}

impl DebugContinueStatus {
    fn get_win32_value(&self) -> NTSTATUS {
        match *self {
            DebugContinueStatus::Continue => DBG_CONTINUE,
            DebugContinueStatus::ExceptionNotHandled => DBG_EXCEPTION_NOT_HANDLED,
            //DebugContinueStatus::ReplyLater => DBG_REPLY_LATER,
        }
    }
}

pub fn continue_debug_event(context: DebugEventContext, continue_status: DebugContinueStatus) {
    let ret = unsafe {
        ContinueDebugEvent(
            context.process.0,
            context.thread.0,
            continue_status.get_win32_value(),
        )
    };
    ret.unwrap_or_else(|error| panic!("ContinueDebugEvent failed: {error}"));
}

pub fn get_final_path_name_by_handle(handle: HANDLE) -> String {
    let mut buffer = vec![0u16; 4096];
    let len = unsafe { GetFinalPathNameByHandleW(handle, buffer.as_mut_slice(), GETFINALPATHNAMEBYHANDLE_FLAGS(0)) } as usize;
    if len == 0 {
        panic!("GetFinalPathNameByHandleW failed: {}", get_last_platform_error_message());
    }
    OsString::from_wide(&buffer[0..len]).to_string_lossy().to_string()
}

// Bit offsets within the DR7 debug register.
// The index is the index of the hardware breakpoint.
const DR7_LEN_BIT: [usize; 4] = [19, 23, 27, 31];
const DR7_RW_BIT: [usize; 4] = [17, 21, 25, 29];
const DR7_LE_BIT: [usize; 4] = [0, 2, 4, 6];

// The size in bits of the `LEN` field (length of the breakpoint) of the DR7 debug register.
const DR7_LEN_SIZE: usize = 2;
// The size in bits of the `RW` field (read/write access type) of the DR7 debug register.
const DR7_RW_SIZE: usize = 2;

// Each bit in DR6 corresponds to if the breakpint is enabled or not.
// The index is the index of the breakpoint.
const DR6_B_BIT: [usize; 4] = [0, 1, 2, 3];

const EFLAG_RF: usize = 16;

pub struct Breakpoint {
    pub address: u64,
}

/// Set a value at a specific bit offset.
fn set_bits<T: num_traits::int::PrimInt>(
    val: &mut T,
    set_val: T,
    start_bit: usize,
    bit_count: usize
) {
    // First, mask out the relevant bits.
    let max_bits: usize = std::mem::size_of::<T>() * 8;
    let mask: T = T::max_value() << (max_bits - bit_count);
    let mask: T = mask >> (max_bits - 1 - start_bit);
    let inverted_mask = !mask;

    *val = *val & inverted_mask;
    *val = *val | (set_val << (start_bit + 1 - bit_count));
}

// Return if the bit a the given index is set.
fn get_bit<T: num_traits::int::PrimInt>(
    val: T,
    bit_index: usize
) -> bool {
    let mask = T::one() << bit_index;
    let masked_val = val & mask;
    masked_val != T::zero()
}

pub fn apply_breakpoints(
    breakpoints: &[Breakpoint],
    process: &mut Process,
    resume_thread_id: ThreadId
) {
    // There are 2 types of breakpoints: software and hardware.
    // Hardware breakpoints are simpler, so using those for now.
    //
    // On x86/AMD64 processors, the harware breakpoints are controlled via the [Debug Registers](https://wiki.osdev.org/CPU_Registers_x86-64#Debug_Registers):
    // * These are 4 hardware breakpoints.
    // * Debug registers ("DR") DR0 through DR3 specify the address of the breakpoint.
    // * Register DR6 is a status register to determien when a breakpoint is hit.
    // * Register DR7 is a control register to speicfy the attributes of each hardware breakpoint.
    //
    // Debug registers are maintained for each thread separately.
    // So theoretically we could set different breakpoints for each thread, but don't support that for now.

    for thread_id in process.iterate_threads() {
        let thread = open_thread(thread_id);
        let mut ctx = get_thread_context(&thread);

        // x86/AMD64 processors have a limit of 4 hardware breakpoints.
        for idx in 0..4 {
            if breakpoints.len() > idx {
                // The DR7_* variables are a set of constants with the correct offsets and sizes for each field of DR7.

                // `LEN` must be set to 0 for instruction breakpoints (in contrast with data breakpoints).
                set_bits(&mut ctx.context.Dr7, 0, DR7_LEN_BIT[idx], DR7_LEN_SIZE);

                // `RW` value of 0 means "break on instruction execution".
                // * A value of 1 means to break on read.
                // * A value of 2 means to break on write.
                // * A value of 3 means to break on read/write.
                set_bits(&mut ctx.context.Dr7, 0, DR7_RW_BIT[idx], DR7_RW_SIZE);

                // `LE` (local enable) value of 1 means that the breakpoint is enabled.
                set_bits(&mut ctx.context.Dr7, 1, DR7_LE_BIT[idx], 1);

                // Debug registers ("DR") DR0 through DR3 specify the address of the breakpoint.
                match idx {
                    0 => ctx.context.Dr0 = breakpoints[idx].address,
                    1 => ctx.context.Dr1 = breakpoints[idx].address,
                    2 => ctx.context.Dr2 = breakpoints[idx].address,
                    3 => ctx.context.Dr3 = breakpoints[idx].address,
                    _ => (),
                }
            } else {
                // Disable unused breakpoints.
                //
                // Assume that we own all breakpoints.
                // This will cause problems with programs that expect to control their own debug registers.
                set_bits(&mut ctx.context.Dr7, 0, DR7_LE_BIT[idx], 1);
                break;
            }
        }

        // Prevent the current thread from hitting a breakpoint on the current instruction.
        //
        // Technically we only need to do this when a breakpoint was hit, but it's simpler to always do so.
        if *thread_id == resume_thread_id {
            set_bits(&mut ctx.context.EFlags, 1, EFLAG_RF, 1);
        }

        set_thread_context(&thread, &ctx.context);
    }
}

pub fn was_breakpoint_hit(
    num_breakpoints: usize,
    thread_context: &AlignedContext
) -> Option<u32> {
    for (idx, dr6_b_bit) in DR6_B_BIT.iter().enumerate().take(num_breakpoints) {
        if get_bit(thread_context.context.Dr6, *dr6_b_bit) {
            return Some(idx as u32);
        }
    }
    None
}