use std::ffi::OsString;
use std::process::ExitCode;

use clap::error::ErrorKind;
use pgs::cmd;
use pgs::cmd::OutputMode;

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().collect();

    match cmd::parse_args(args.clone()) {
        Ok(parsed) => run_command(parsed),
        Err(e) if matches!(e.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion) => {
            e.exit()
        }
        Err(e) => {
            let output_mode = cmd::detect_output_mode(&args).unwrap_or(OutputMode::Text);
            let renderable = cmd::parse_failure(&e);
            render_error(&renderable, e.exit_code(), output_mode)
        }
    }
}

fn run_command(parsed: cmd::ParsedCli) -> ExitCode {
    let output_mode = parsed.output_mode;
    let command = parsed.command();

    match cmd::run(parsed) {
        Ok(Some(output)) => {
            let override_code = output.exit_override();
            match cmd::render(&output, output_mode) {
                Ok(rendered) => {
                    let exit = override_code.map_or(ExitCode::SUCCESS, |code| {
                        ExitCode::from(u8::try_from(code).expect("exit code fits in u8"))
                    });
                    write_stdout(&rendered, exit)
                }
                Err(e) => {
                    let renderable = cmd::runtime_failure(command, &e);
                    render_error(&renderable, e.exit_code(), output_mode)
                }
            }
        }
        Ok(None) => ExitCode::SUCCESS,
        Err(e) => {
            let renderable = cmd::runtime_failure(command, &e);
            render_error(&renderable, e.exit_code(), output_mode)
        }
    }
}

fn render_error(renderable: &cmd::RenderableError, code: i32, output_mode: OutputMode) -> ExitCode {
    let rendered =
        cmd::render_error(renderable, output_mode).expect("error output should always serialize");
    let exit = ExitCode::from(u8::try_from(code).expect("exit code fits in u8"));
    write_stdout(&rendered, exit)
}

fn write_stdout(rendered: &str, exit_code: ExitCode) -> ExitCode {
    if !rendered.is_empty() {
        println!("{rendered}");
    }

    exit_code
}
