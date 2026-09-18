use std::io::{self, Write};

use kale::render::Format;
use kale::{Options, RenderMode, input, render_image};

const USAGE: &str = "kale — true-color terminal image renderer

Usage:
  kale <image> [--mode <mode>] [--width <columns>] [--format ansi|html] [--background <#rrggbb>]

Options:
  -w, --width       Terminal columns (default: 80)
  -m, --mode        half (default), quadrant, braille, or glyph
  -f, --format      ansi (default) or html
  -b, --background  Flatten partial transparency onto this color (default: #000000)
      --opaque      Paint every cell, including fully transparent ones
      --font        Font family used to calibrate glyph mode (default: monospace)
  -V, --version     Show version
  -h, --help        Show this help
";

enum Command {
    Help,
    Version,
    Run(Options),
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => {}
        Err(message) => {
            eprintln!("kale: {message}");
            std::process::exit(1);
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let options = match parse_args(args)? {
        Command::Help => {
            print!("{USAGE}");
            return Ok(());
        }
        Command::Version => {
            println!("kale {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Command::Run(options) => options,
    };

    let output = render_image(&options)?;

    let stdout = io::stdout();
    let mut handle = io::BufWriter::new(stdout.lock());
    // A closed pipe (for example `kale ... | head`) is not an error worth reporting.
    if let Err(error) = handle
        .write_all(output.as_bytes())
        .and_then(|()| handle.write_all(b"\n"))
    {
        if error.kind() != io::ErrorKind::BrokenPipe {
            return Err(error.to_string());
        }
    }
    Ok(())
}

fn parse_args(args: &[String]) -> Result<Command, String> {
    // Values are collected first and validated afterwards, in the same order the
    // original checked them: a missing input outranks a bad --width, even when
    // --width came first on the command line. `None` means the flag never
    // appeared, which is different from appearing without a value.
    let mut width_raw: Option<String> = None;
    let mut mode_raw: Option<String> = None;
    let mut format_raw: Option<String> = None;
    let mut background_raw: Option<String> = None;
    let mut font = String::from("monospace");
    let mut opaque = false;
    let mut input: Option<String> = None;

    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if arg == "-h" || arg == "--help" {
            return Ok(Command::Help);
        }
        if arg == "-V" || arg == "--version" {
            return Ok(Command::Version);
        }
        if arg == "-w" || arg == "--width" {
            index += 1;
            // A trailing flag has no value; an empty string stands in for the
            // original's `undefined`, which failed the same validations.
            width_raw = Some(args.get(index).cloned().unwrap_or_default());
        } else if arg == "-m" || arg == "--mode" {
            index += 1;
            // A trailing flag has no value; an empty string stands in for the
            // original's `undefined`, which failed the same validations.
            mode_raw = Some(args.get(index).cloned().unwrap_or_default());
        } else if arg == "-f" || arg == "--format" {
            index += 1;
            // A trailing flag has no value; an empty string stands in for the
            // original's `undefined`, which failed the same validations.
            format_raw = Some(args.get(index).cloned().unwrap_or_default());
        } else if arg == "-b" || arg == "--background" {
            index += 1;
            // A trailing flag has no value; an empty string stands in for the
            // original's `undefined`, which failed the same validations.
            background_raw = Some(args.get(index).cloned().unwrap_or_default());
        } else if arg == "--opaque" {
            opaque = true;
        } else if arg == "--font" {
            index += 1;
            // A missing value keeps the default, as the original did.
            if let Some(value) = args.get(index) {
                font = value.clone();
            }
        } else if let Some(stripped) = arg.strip_prefix('-') {
            return Err(format!("Unknown option: -{stripped}"));
        } else if input.is_some() {
            return Err("Only one input image is supported.".to_string());
        } else {
            input = Some(arg.to_string());
        }
        index += 1;
    }

    let input = input.ok_or_else(|| "An input image is required.".to_string())?;

    let width = match &width_raw {
        None => 80,
        Some(raw) => match raw.parse::<usize>() {
            Ok(value) if (1..=1000).contains(&value) => value,
            _ => return Err("--width must be an integer from 1 to 1000.".to_string()),
        },
    };

    let mode = match mode_raw.as_deref() {
        None => RenderMode::Half,
        Some("half") => RenderMode::Half,
        Some("quadrant") => RenderMode::Quadrant,
        Some("braille") => RenderMode::Braille,
        Some("glyph") => RenderMode::Glyph,
        Some(_) => return Err("--mode must be half, quadrant, braille, or glyph.".to_string()),
    };

    let format = match format_raw.as_deref() {
        None => Format::Ansi,
        Some("ansi") => Format::Ansi,
        Some("html") => Format::Html,
        Some(_) => return Err("--format must be ansi or html.".to_string()),
    };

    let background = match background_raw.as_deref() {
        None => [0, 0, 0],
        Some(raw) => input::parse_hex_color(raw)
            .ok_or_else(|| "--background must be in #rrggbb form.".to_string())?,
    };

    Ok(Command::Run(Options {
        input,
        width,
        mode,
        format,
        background,
        font,
        // Leaving transparent cells unpainted is the default: a transparent
        // logo should show the terminal's own background, not a rectangle of
        // --background. `--opaque` restores painting every cell.
        transparent: !opaque,
    }))
}
