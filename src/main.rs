use std::{
    collections::HashMap,
    env,
};

use memory::MemorySource;
use windows::{
    AutoClosedHandle,
    DebugContinueStatus,
    DebugEvent,
    ThreadId,
    ProcessId,
};

mod command;
mod eval;
mod memory;
mod module;
mod name_resolution;
mod process;
mod registers;
mod windows;

use command::grammar::CommandExpr;
use process::Process;

#[derive(Debug)]
struct ThreadState {
    expect_step_exception: bool,
}

impl ThreadState {
    pub fn new() -> Self {
        ThreadState{
            expect_step_exception: false,
        }
    }
}

fn show_usage() {
    let command_line_args: Vec<String> = env::args().collect();

    // The 1st argument is the name of the program
    let program_name = &command_line_args[0];

    println!("Usage: {program_name} <Command-Line>");
}

fn load_module_at_address(
    process: &mut Process,
    memory_source: &dyn MemorySource,
    base_address: u64,
    module_name: Option<String>,
) {
    let module = process.add_module(base_address, module_name, memory_source).unwrap();
    println!("LoadModule: {base_address:#x}   {name}", name = module.name);
}

fn main_debugger_loop(process_handle: AutoClosedHandle) {
    let mut thread_states = HashMap::<(ProcessId, ThreadId), ThreadState>::new();
    let mem_source = memory::make_live_memory_source(process_handle.handle());
    let mut process = Process::new();

    loop {
        let (event_context, debug_event) = windows::wait_for_debug_event(mem_source.as_ref());
        let mut continue_status = DebugContinueStatus::Continue;

        match debug_event {
            DebugEvent::Exception { first_chance, code } => {
                let chance_string = if first_chance {
                    "second chance"
                } else {
                    "first chance"
                };

                // Assume that the first EXCEPTION_SINGLE_STEP exception from a thread after we step (via trap) is from our trap.
                let thread_state = thread_states.get_mut(&(event_context.process, event_context.thread))
                    .unwrap_or_else(|| panic!("Exception code {code_num:#x} ({chance_string}) for unknown process {process_id:#x}, thread {thread_id:#x}", code_num = code.0, process_id = event_context.process, thread_id = event_context.thread));
                if thread_state.expect_step_exception && code == windows::EXCEPTION_CODE_SINGLE_STEP {
                    thread_state.expect_step_exception = false;
                } else {
                    println!("Exception code {code_num:#x} ({chance_string})", code_num = code.0);
                    continue_status = DebugContinueStatus::ExceptionNotHandled;
                }
            }
            DebugEvent::CreateThread => {
                println!("CreateThread");

                // Register the thread.
                assert!(!thread_states.contains_key(&(event_context.process, event_context.thread)));
                thread_states.insert((event_context.process, event_context.thread), ThreadState::new());
            }
            DebugEvent::ExitThread { exit_code } => {
                println!("ExitThread code: {exit_code} process: {process_id:#x}, thread: {thread_id:#x}", process_id = event_context.process, thread_id = event_context.thread);

                // Unregister the thread.
                assert!(thread_states.contains_key(&(event_context.process, event_context.thread)));
                thread_states.remove(&(event_context.process, event_context.thread));
            }
            DebugEvent::CreateProcess { name, base_addr } => {
                // Register the thread.
                assert!(!thread_states.contains_key(&(event_context.process, event_context.thread)));
                thread_states.insert((event_context.process, event_context.thread), ThreadState::new());

                load_module_at_address(&mut process, mem_source.as_ref(), base_addr, name);
            }
            DebugEvent::ExitProcess { exit_code } => {
                println!("ExitProcess: code: {exit_code} process: {process_id:#x}", process_id = event_context.process);

                // Unregister the thread.
                assert!(thread_states.contains_key(&(event_context.process, event_context.thread)));
                thread_states.remove(&(event_context.process, event_context.thread));

                // Exit the debug loop.
                break;
            }
            DebugEvent::LoadDll { name, base_addr } => {
                load_module_at_address(&mut process, mem_source.as_ref(), base_addr, name);
            }
            DebugEvent::UnloadDll => {
                println!("UnloadDll")
            }
            DebugEvent::OutputDebugString(debug_string) => {
                println!("DebugOut: {debug_string}");
            }
            DebugEvent::Rip { error, info_type } => println!("RipEvent: error: {error}, type: {}", info_type.0),
        }

        let thread = windows::open_thread(&event_context.thread);
        let mut thread_context = windows::get_thread_context(&thread);

        let mut continue_execution = false;
        while !continue_execution {
            if let Some(sym) = name_resolution::resolve_address_to_name(thread_context.context.Rip, &mut process) {
                // Print the thread and symbol.
                println!("Thread: {:#x} {sym}", event_context.thread.value());
            } else {
                // Print the thread and instruction pointer.
                println!("[Thread: {:#x}, IP: {:#018x}]", event_context.thread.value(), thread_context.context.Rip);
            }

            match command::read_command() {
                CommandExpr::Help(_) | CommandExpr::HelpAlias(_) => {
                    command::print_command_help();
                }
                CommandExpr::Step(_) | CommandExpr::StepAlias(_) => {
                    // Set the trap flag context, which will throw an EXCEPTION_SINGLE_STEP exception after executing the next instruction.
                    thread_context.context.EFlags |= windows::TRAP_FLAG;
                    windows::set_thread_context(&thread, &thread_context.context);

                    let thread_state = thread_states.get_mut(&(event_context.process, event_context.thread))
                        .unwrap_or_else(|| panic!("Cannot step because missing thread state for process {process_id:#x}, thread {thread_id:#x}", process_id = event_context.process, thread_id = event_context.thread));
                    thread_state.expect_step_exception = true;
                    continue_execution = true;
                }
                CommandExpr::Continue(_) | CommandExpr::ContinueAlias(_) => {
                    continue_execution = true;
                }
                CommandExpr::DisplayRegisters(_) | CommandExpr::DisplayRegistersAlias(_) => {
                    registers::display_all(thread_context.context);
                }
                CommandExpr::DisplayBytes(_, expr) | CommandExpr::DisplayBytesAlias(_, expr) => {
                    let address = eval::evaluate_expression(*expr);
                    let bytes = mem_source.read_raw_memory(address, 16);
                    for byte in bytes {
                        print!("{byte:02X} ");
                    }
                    println!();
                }
                CommandExpr::Evaluate(_, expr) | CommandExpr::EvaluateAlias(_, expr) => {
                    let val = eval::evaluate_expression(*expr);
                    println!(" = {val:#x}");
                }
                CommandExpr::ListNearest(_, expr) | CommandExpr::ListNearestAlias(_, expr) => {
                    let val = eval::evaluate_expression(*expr);
                    if let Some(sym) = name_resolution::resolve_address_to_name(val, &mut process) {
                        println!("{sym}");
                    } else {
                        println!("No symbol found");
                    }
                }
                CommandExpr::Quit(_) | CommandExpr::QuitAlias(_) => {
                    // The process will be terminated since we didn't detach.
                    return;
                }
            }
        }

        windows::continue_debug_event(event_context, continue_status);
    }
}

fn launch_and_debug_process(target_command_line_args: &[String]) {
    let process = windows::launch_process_for_debugging(target_command_line_args);
    main_debugger_loop(process);
}

fn main() {
    let full_command_line_args: Vec<String> = env::args().collect();
    // The 1st argument is the name of the program
    let target_command_line_args = &full_command_line_args[1..];

    if target_command_line_args.is_empty() {
        show_usage();
        return;
    };

    launch_and_debug_process(target_command_line_args)
}