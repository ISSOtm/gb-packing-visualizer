use parse_display::FromStr;
use std::env;
use std::ffi::OsStr;
use std::path::Path;
use std::process::exit;

mod parse;
mod render;

#[derive(Debug)]
struct Location {
    bank: u32,
    addr: u16,
}

#[derive(FromStr, Debug, PartialEq, Eq)]
#[display(style = "UPPERCASE")]
enum MemType {
    Rom0,
    Romx,
    Vram,
    Sram,
    Wram0,
    Wramx,
    Oam,
    Hram,
}

#[derive(Debug)]
struct Section {
    mem_type: MemType,
    location: Location,
    align_mask: u16,
    align_ofs: u16,
    size: u16,
    name: String,
}

#[derive(Debug)]
struct Frame {
    location: Location,
    section_id: usize,
}

#[derive(Debug)]
pub struct Sequence {
    nb_banks: u32,
    frames: Vec<Frame>,
    sections: Vec<Section>,
}

impl Location {
    fn is_floating(&self) -> bool {
        self.addr == u16::MAX
    }

    fn is_floating_bank(&self) -> bool {
        self.bank == u32::MAX
    }
}

impl Section {
    fn is_floating(&self) -> bool {
        self.location.is_floating()
    }

    fn is_floating_bank(&self) -> bool {
        self.location.is_floating_bank()
    }
}

fn usage(progname: &OsStr) {
    eprintln!("Usage: {} <output file>", progname.to_string_lossy());
}

fn main() {
    let mut args = env::args_os();
    let progname = args.next().unwrap_or_else(|| env!("CARGO_PKG_NAME").into());
    let out_path = args.next().unwrap_or_else(|| {
        usage(&progname);
        exit(1);
    });

    let sequence = match parse::parse_input() {
        Ok(seq) => seq,
        Err(err) => {
            eprintln!("Input parse error: {}", err);
            exit(1);
        }
    };

    if let Err(err) = render::render(&sequence, Path::new(&out_path)) {
        eprintln!("Rendering error: {}", err);
        exit(1);
    }
}
