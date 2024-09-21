use std::io::Write;

use codemap::CodeMap;
use codemap_diagnostic::{ColorConfig, Diagnostic, Emitter, Level, SpanLabel, SpanStyle};
use rust_sitter::errors::{ParseError, ParseErrorReason};

#[rust_sitter::grammar("command")]
pub mod grammar {
    #[rust_sitter::language]
    pub enum CommandExpr {
        Help(#[rust_sitter::leaf(text = "help")] ()),
        HelpAlias(#[rust_sitter::leaf(text = "h")] ()),
        Step(#[rust_sitter::leaf(text = "step")] ()),
        StepAlias(#[rust_sitter::leaf(text = "s")] ()),
        Continue(#[rust_sitter::leaf(text = "continue")] ()),
        ContinueAlias(#[rust_sitter::leaf(text = "c")] ()),
        DisplayRegisters(#[rust_sitter::leaf(text = "registers")] ()),
        DisplayRegistersAlias(#[rust_sitter::leaf(text = "r")] ()),
        DisplayBytes(#[rust_sitter::leaf(text = "display-bytes")] (), Box<EvalExpr>),
        DisplayBytesAlias(#[rust_sitter::leaf(text = "db")] (), Box<EvalExpr>),
        Evaluate(#[rust_sitter::leaf(text = "eval")] (), Box<EvalExpr>),
        EvaluateAlias(#[rust_sitter::leaf(text = "?")] (), Box<EvalExpr>),
        ListNearest(#[rust_sitter::leaf(text = "list-nearest")] (), Box<EvalExpr>),
        ListNearestAlias(#[rust_sitter::leaf(text = "ln")] (), Box<EvalExpr>),
        Quit(#[rust_sitter::leaf(text = "quit")] ()),
        QuitAlias(#[rust_sitter::leaf(text = "q")] ()),
    }

    #[rust_sitter::language]
    pub enum EvalExpr {
        Number(#[rust_sitter::leaf(pattern = r"(\d+|0x[0-9a-fA-F]+)", transform = parse_int)] u64),
        #[rust_sitter::prec_left(1)]
        Add(
            Box<EvalExpr>,
            #[rust_sitter::leaf(text = "+")] (),
            Box<EvalExpr>,
        )
    }

    #[rust_sitter::extra]
    struct Whitespace {
        #[rust_sitter::leaf(pattern = r"\s")]
        _whitespace: (),
    }

    fn parse_int(text: &str) -> u64 {
        let text = text.trim();
        if text.starts_with("0x") {
            let text = text.split_at(2).1;
            u64::from_str_radix(text, 16).unwrap()
        } else {
            text.parse().unwrap()
        }
    }
}

// Copied from https://github.com/hydro-project/rust-sitter/blob/main/example/src/main.rs
fn convert_parse_error_to_diagnostics(
    file_span: &codemap::Span,
    error: &ParseError,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match &error.reason {
        ParseErrorReason::MissingToken(token) => diagnostics.push(Diagnostic {
            level: Level::Error,
            message: format!("Missing token: \"{token}\""),
            code: Some(String::from("S000")),
            spans: vec![SpanLabel {
                span: file_span.subspan(error.start as u64, error.end as u64),
                style: SpanStyle::Primary,
                label: Some(format!("missing \"{token}\"")),
            }],
        }),
        ParseErrorReason::UnexpectedToken(token) => diagnostics.push(Diagnostic {
            level: Level::Error,
            message: format!("Unexpected token: \"{token}\""),
            code: Some(String::from("S000")),
            spans: vec![SpanLabel {
                span: file_span.subspan(error.start as u64, error.end as u64),
                style: SpanStyle::Primary,
                label: Some(format!("unexpected \"{token}\"")),
            }],
        }),
        ParseErrorReason::FailedNode(errors) => {
            if errors.is_empty() {
                diagnostics.push(Diagnostic {
                    level: Level::Error,
                    message: String::from("Failed to parse node"),
                    code: Some(String::from("S000")),
                    spans: vec![SpanLabel {
                        span: file_span.subspan(error.start as u64, error.end as u64),
                        style: SpanStyle::Primary,
                        label: Some(String::from("failed")),
                    }],
                })
            } else {
                for error in errors {
                    convert_parse_error_to_diagnostics(file_span, error, diagnostics);
                }
            }
        }
    }
}

pub fn print_command_help() {
    println!("Commands:
    help (h): Print command help.
    step (s): Step to the next instruction.
    continue (c): Continue the program until the next debug event.
    registers (r): Print the registers.
    display-bytes (db): Display data at a memory location. For example, `display-bytes 0x123`.
    eval (?): Add addresses. For example, `eval 0x123 + 10`.
    list-nearest (ln): List the symbol nearest to the address. For example, `list-nearest 0x123`.
    quit (q): Quit.");
}

pub fn read_command() -> grammar::CommandExpr {
    let stdin = std::io::stdin();
    loop {
        print!("\n> ");
        std::io::stdout().flush().unwrap();

        let mut input = String::new();
        stdin.read_line(&mut input).unwrap();
        let input = input.trim().to_string();

        if !input.is_empty() {
            match grammar::parse(&input) {
                Ok(expr) => return expr,
                Err(errors) => {
                    // Convert the errors to diagnostics and emit them.
                    // Copied from https://github.com/hydro-project/rust-sitter/blob/main/example/src/main.rs

                    let mut code_map = CodeMap::new();
                    let file_span = code_map.add_file(String::from("<input>"), input);
                    let mut diagnostics = vec![];
                    for error in errors {
                        convert_parse_error_to_diagnostics(&file_span.span, &error, &mut diagnostics)
                    }

                    let mut emitter = Emitter::stderr(ColorConfig::Always, Some(&code_map));
                    emitter.emit(&diagnostics);
                }
            }
        }
    }
}