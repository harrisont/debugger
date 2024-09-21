# debugger

A Windows executable debugger written in Rust.

To run:
```shell
cargo run -- SomeProgram.exe arg1 arg2 arg3
```

For example:
```shell
cargo run -- cmd.exe /k "echo hello"
```

Based off of Tim Misiak's [Writing a Debugger From Scratch blog posts](https://www.timdbg.com/posts/writing-a-debugger-from-scratch-part-1/).

## References

* [MSDN: Debugging Events](https://learn.microsoft.com/en-us/windows/win32/debug/debugging-events)
* [MSDN: Debugging Functions](https://learn.microsoft.com/en-us/windows/win32/debug/debugging-functions)
* [MSDN: Writing the Debugger's Main Loop](https://learn.microsoft.com/en-us/windows/win32/debug/writing-the-debugger-s-main-loop)