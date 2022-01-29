use crate::{Location, MemType, Section, Sequence};
use mp4::{
    AvcConfig, FourCC, MediaConfig, Mp4Config, Mp4Sample, Mp4Writer, TrackConfig, TrackType,
};
use openh264::encoder::{Encoder, EncoderConfig};
use openh264::formats::RBGYUVConverter;
use std::cmp;
use std::convert::{TryFrom, TryInto};
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io::{self, BufWriter};
use std::path::Path;

#[derive(Debug)]
pub struct RenderError {
    kind: RenderErrorKind,
    frame: Option<u32>,
}

#[derive(Debug)]
enum RenderErrorKind {
    Io(io::Error),
    H264(openh264::Error),
    Mp4(mp4::Error),
}

impl From<io::Error> for RenderError {
    fn from(err: io::Error) -> Self {
        Self {
            kind: RenderErrorKind::Io(err),
            frame: None,
        }
    }
}

impl From<openh264::Error> for RenderError {
    fn from(err: openh264::Error) -> Self {
        Self {
            kind: RenderErrorKind::H264(err),
            frame: None,
        }
    }
}

impl From<mp4::Error> for RenderError {
    fn from(err: mp4::Error) -> Self {
        Self {
            kind: RenderErrorKind::Mp4(err),
            frame: None,
        }
    }
}

impl fmt::Display for RenderError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let loc_string = match self.frame {
            Some(frame) => format!(" (on frame {})", frame),
            None => "".to_string(), // TODO: meh
        };
        match &self.kind {
            RenderErrorKind::Io(err) => write!(fmt, "I/O error{}: {}", loc_string, err),
            RenderErrorKind::H264(err) => write!(fmt, "H264 error{}: {}", loc_string, err),
            RenderErrorKind::Mp4(err) => write!(fmt, "MP4 error{}: {}", loc_string, err),
        }
    }
}

impl Error for RenderError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self.kind {
            RenderErrorKind::Io(ref err) => Some(err),
            RenderErrorKind::H264(ref err) => Some(err),
            RenderErrorKind::Mp4(ref err) => Some(err),
        }
    }
}

#[derive(Debug)]
struct Canvas {
    bank_width: u32,
    nb_banks: u32,
    pixels: Vec<u8>,
}

type Color = (u8, u8, u8);

impl Canvas {
    // The layout is: N pixels, 2 spacers, N pixels, and so on
    const HEIGHT: u32 = 512;
    const MAX_WIDTH: u32 = Canvas::HEIGHT * 2; // 2:1 should be an *acceptable* ratio
    const SPACER_WIDTH: u32 = 2;
    const MAX_BANK_WIDTH: u32 = 32 - Canvas::SPACER_WIDTH;
    const BYTES_PER_ROW: u32 = 0x4000 / Canvas::HEIGHT; // How many bytes each row of pixels represents

    const FILLED_COLOR: Color = (0, 255, 0);
    const OVERLAY_COLOR: Color = (255, 0, 0);

    pub fn new(nb_banks: u32) -> Self {
        // Pick a width depending on the amount of banks
        // Note that the width has to be even! Thus, we round the width down if necessary.
        let bank_width = cmp::min(
            ((Self::MAX_WIDTH / nb_banks) & !1) - Self::SPACER_WIDTH,
            Self::MAX_BANK_WIDTH,
        );
        let width = Self::n_banks_width(bank_width, nb_banks);

        let mut canvas = Self {
            bank_width,
            nb_banks,
            // Canvas is white by default
            pixels: vec![255; (width * Self::HEIGHT * 3).try_into().unwrap()],
        };

        // Draw columns between sections
        let width = canvas.width();
        for y in 0..canvas.height() {
            for bank in 1..canvas.nb_banks {
                for xofs in 1..=Self::SPACER_WIDTH {
                    Self::write_color(
                        &mut canvas.pixels,
                        bank * (canvas.bank_width + Self::SPACER_WIDTH) - xofs,
                        y,
                        width,
                        (0, 0, 0),
                    );
                }
            }
        }

        canvas
    }

    fn n_banks_width(bank_width: u32, nb_banks: u32) -> u32 {
        (bank_width + Self::SPACER_WIDTH) * nb_banks - Self::SPACER_WIDTH
    }

    pub fn width(&self) -> u32 {
        Self::n_banks_width(self.bank_width, self.nb_banks)
    }

    pub fn height(&self) -> u32 {
        Canvas::HEIGHT
    }

    fn write_color(pixels: &mut [u8], x: u32, y: u32, width: u32, color: Color) {
        let idx = usize::try_from(x + y * width).unwrap() * 3;
        pixels[idx] = color.0;
        pixels[idx + 1] = color.1;
        pixels[idx + 2] = color.2;
    }

    fn draw_rect(
        pixels: &mut [u8],
        location: &Location,
        nb_bytes: u32,
        width: u32,
        bank_width: u32,
        color: Color,
    ) {
        let addr = u32::from(location.addr) % 0x4000; // Only take the address within the bank

        let x = location.bank * (bank_width + Self::SPACER_WIDTH);
        let first_byte_row = addr / Self::BYTES_PER_ROW;
        // Cap at the end of the bank, of course
        let last_byte_row = cmp::min(addr + nb_bytes - 1, 0x3fff) / Self::BYTES_PER_ROW;

        for y in first_byte_row..=last_byte_row {
            for x_ofs in 0..bank_width {
                Self::write_color(pixels, x + x_ofs, y, width, color);
            }
        }
    }

    pub fn settle(&mut self, section: &Section, location: &Location) {
        let width = self.width();
        let bank_width = self.bank_width;

        Self::draw_rect(
            &mut self.pixels,
            location,
            section.size.into(),
            width,
            bank_width,
            Self::FILLED_COLOR,
        );
    }

    pub fn overlay(&self, section: &Section, location: &Location) -> Vec<u8> {
        let mut pixels = self.pixels.clone();
        let width = self.width();
        let bank_width = self.bank_width;

        Self::draw_rect(
            &mut pixels,
            location,
            section.size.into(),
            width,
            bank_width,
            Self::OVERLAY_COLOR,
        );
        pixels
    }
}

pub fn render(sequence: &Sequence, out_path: &Path) -> Result<(), RenderError> {
    eprint!("Rendering...\r");

    let out = BufWriter::new(File::create(out_path)?);
    let mut canvas = Canvas::new(sequence.nb_banks);
    let mut encoder = Encoder::with_config(EncoderConfig::new(canvas.width(), canvas.height()))?;
    let section = |section_id| &sequence.sections[section_id];

    let fcc = |code: &[u8; 4]| FourCC { value: *code };
    let mut writer = Mp4Writer::write_start(
        out,
        &Mp4Config {
            major_brand: fcc(b"isom"),
            minor_version: 512,
            compatible_brands: vec![fcc(b"isom"), fcc(b"iso2"), fcc(b"avc1"), fcc(b"mp41")],
            timescale: 60,
        },
    )?;

    writer.add_track(&TrackConfig {
        track_type: TrackType::Video,
        timescale: 60,
        language: "eng".to_string(), // No real language so to speak...
        media_conf: MediaConfig::AvcConfig(AvcConfig {
            width: canvas.width().try_into().unwrap(),
            height: Canvas::HEIGHT.try_into().unwrap(),
            seq_param_set: vec![
                0, // ???
                0, // avc_profile_indication
                0, // profile_compatibility
                0, // avc_level_indication
            ],
            pic_param_set: vec![],
        }),
    })?;

    let mut iter = sequence
        .frames
        .iter()
        .enumerate()
        .map(|(i, frame)| (i, frame, section(frame.section_id)))
        .filter(|(_, _, section)| matches!(section.mem_type, MemType::Rom0 | MemType::Romx))
        .peekable();

    while let Some((i, frame, section)) = iter.next() {
        eprint!("Rendering... {} / {}\r", i, sequence.frames.len());

        let pixels = canvas.overlay(section, &frame.location);
        let mut yuv = RBGYUVConverter::new(
            canvas.width().try_into().unwrap(),
            canvas.height().try_into().unwrap(),
        );
        yuv.convert(&pixels);

        let mut bytes = vec![];
        encoder.encode(&yuv)?.write_vec(&mut bytes);

        writer.write_sample(
            1,
            &Mp4Sample {
                start_time: i.try_into().unwrap(),
                duration: 1,
                rendering_offset: 0,
                is_sync: true,
                bytes: bytes.into(),
            },
        )?;

        // If the next frame uses a different section, "settle" the current one's
        if iter
            .peek()
            .map(|(_, next_frame, _)| next_frame.section_id != frame.section_id)
            == Some(true)
        {
            canvas.settle(section, &frame.location);
        }
    }

    writer.write_end()?;

    eprintln!("Rendering... - Done.      ");
    Ok(())
}
