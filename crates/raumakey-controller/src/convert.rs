//! `convert --from <charset> --to <charset> (--in <file> --out <file> [--overwrite] |
//! --clipboard)` (plan §8, §10.2).
//!
//! Files go through `raumakey_converter::convert_file`: it never writes the source, refuses a
//! destination that is the source, refuses an existing destination unless `--overwrite` is
//! given, and writes a temporary file that it renames onto the destination. The clipboard goes
//! through `raumakey_converter::convert_clipboard`, which writes only while the clipboard still
//! holds the text it read. A command line has no secure (password) field, so `secure_field` is
//! false. No case or tone transform is applied (the command line has no options for them).
//!
//! Messages name the charsets and the paths the user passed, never file or clipboard text.

use std::path::Path;

use raumakey_converter::{
    Charset, ClipboardOutcome, ClipboardPort, ConvertError, ConvertOptions, convert_clipboard,
    convert_file,
};

use crate::cli::{ConvertArgs, ConvertTarget, EXIT_FAILED, EXIT_OK};
use crate::output::Output;

/// `convert` (see the module documentation). `clipboard` is used only by `--clipboard`.
pub fn convert(
    args: &ConvertArgs,
    clipboard: &mut dyn ClipboardPort,
    output: &mut Output<'_>,
) -> u8 {
    match &args.target {
        ConvertTarget::Files {
            input,
            output: destination,
            overwrite,
        } => convert_files(args, input, destination, *overwrite, output),
        ConvertTarget::Clipboard => convert_clipboard_text(args, clipboard, output),
    }
}

/// The note printed after a lossy conversion.
fn lossy_note(to: Charset, output: &mut Output<'_>) {
    output.line(format_args!(
        "Some characters could not be written exactly in {to}: they were written as \"?\" or, \
         in tcvn3, as lowercase letters."
    ));
}

fn convert_files(
    args: &ConvertArgs,
    input: &Path,
    destination: &Path,
    overwrite: bool,
    output: &mut Output<'_>,
) -> u8 {
    let options = ConvertOptions::default();
    match convert_file(input, destination, args.from, args.to, options, overwrite) {
        Ok(outcome) => {
            output.line(format_args!(
                "Converted {} ({}) into {} ({}): {} bytes read, {} bytes written.",
                input.display(),
                args.from,
                destination.display(),
                args.to,
                outcome.bytes_read,
                outcome.bytes_written
            ));
            if outcome.replaced_existing {
                output.line("The existing destination file was replaced.");
            }
            if outcome.lossy {
                lossy_note(args.to, output);
            }
            EXIT_OK
        }
        Err(error) => {
            output.error(format_args!(
                "convert: {} could not be converted into {}: {error}",
                input.display(),
                destination.display()
            ));
            if error == ConvertError::DestinationExists {
                output.detail("add --overwrite to replace it");
            }
            EXIT_FAILED
        }
    }
}

fn convert_clipboard_text(
    args: &ConvertArgs,
    clipboard: &mut dyn ClipboardPort,
    output: &mut Output<'_>,
) -> u8 {
    let options = ConvertOptions::default();
    match convert_clipboard(clipboard, args.from, args.to, options, false) {
        Ok(ClipboardOutcome::Converted { lossy }) => {
            output.line(format_args!(
                "Converted the clipboard text from {} to {}.",
                args.from, args.to
            ));
            if lossy {
                lossy_note(args.to, output);
            }
            EXIT_OK
        }
        Ok(ClipboardOutcome::Unchanged) => {
            output.line(format_args!(
                "Converting the clipboard text from {} to {} changes nothing; the clipboard was \
                 left alone.",
                args.from, args.to
            ));
            EXIT_OK
        }
        Ok(ClipboardOutcome::NoText) => {
            output.error("convert: the clipboard holds no text; nothing was changed");
            EXIT_FAILED
        }
        Err(error) => {
            output.error(format_args!(
                "convert: the clipboard text could not be converted: {error}"
            ));
            EXIT_FAILED
        }
    }
}
