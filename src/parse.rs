use crate::{Frame, Location, MemType, Section, Sequence};
use lazy_static::lazy_static;
use parse_display::Display;
use regex::Regex;
use std::error::Error;
use std::fmt;
use std::io;
use std::num::ParseIntError;
use std::str::FromStr;

#[derive(Debug, Display)]
#[display(style = "Title case")]
pub enum LocationParseError {
    MissingColon,
    #[display("{}: {0}")]
    BadBank(ParseIntError),
    #[display("{}: {0}")]
    BadAddr(ParseIntError),
}

#[derive(Debug, Display)]
#[display(style = "Title case")]
pub enum SectionParseError {
    SyntaxError,
    #[display("{}: {0}")]
    BadType(parse_display::ParseError),
    #[display("{}: {0}")]
    BadLocation(LocationParseError),
    #[display("{}: {0}")]
    BadAlignMask(ParseIntError),
    #[display("{}: {0}")]
    BadAlignOfs(ParseIntError),
    #[display("{}: {0}")]
    BadSize(ParseIntError),
}

type AttemptParseError = LocationParseError;

#[derive(Debug)]
pub enum ParseError {
    Io(io::Error),
    AttemptBeforeSection(u64, String),
    BadSection(SectionParseError, u64, String),
    BadAttempt(AttemptParseError, u64, String),
}

impl From<io::Error> for ParseError {
    fn from(err: io::Error) -> Self {
        ParseError::Io(err)
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Io(err) => write!(fmt, "I/O error: {}", err),
            Self::AttemptBeforeSection(line_no, line) => write!(
                fmt,
                "Location attempt before any sections on line {} ({})",
                line_no, line
            ),
            Self::BadSection(err, line_no, line) => {
                write!(fmt, "Bad section on line {}: {} ({})", line_no, err, line)
            }
            Self::BadAttempt(err, line_no, line) => write!(
                fmt,
                "Bad location attempt on line {}: {} ({})",
                line_no, err, line
            ),
        }
    }
}

impl Error for ParseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::AttemptBeforeSection(..) | Self::BadSection(..) | Self::BadAttempt(..) => None,
        }
    }
}

impl FromStr for Location {
    type Err = LocationParseError;

    fn from_str(line: &str) -> Result<Self, <Self as FromStr>::Err> {
        let (bank, addr) = line
            .split_once(':')
            .ok_or(LocationParseError::MissingColon)?;

        let bank = u32::from_str_radix(bank.trim(), 16).map_err(LocationParseError::BadBank)?;
        let addr = u16::from_str_radix(addr.trim(), 16).map_err(LocationParseError::BadAddr)?;
        Ok(Self { bank, addr })
    }
}

impl FromStr for Section {
    type Err = SectionParseError;

    fn from_str(rest: &str) -> Result<Self, <Self as FromStr>::Err> {
        // Format: "type @ bank:addr & algn_mask + ofs ] size name..." (only one space before name)
        lazy_static! {
            static ref RE: Regex = Regex::new(
                r"(?x)
                    ^([^[:blank:]@]+)
                    [[:blank:]]*@[[:blank:]]*([^[:blank:]&]+)
                    [[:blank:]]*&[[:blank:]]*([^[:blank:]+]+)
                    [[:blank:]]*\+[[:blank:]]*([^[:blank:]\]]+)
                    [[:blank:]]*\][[:blank:]]*([^[:blank:]]+)
                    [[:blank:]](.*)$"
            )
            .unwrap();
        }

        let captures = RE.captures(rest).ok_or(SectionParseError::SyntaxError)?;
        Ok(Self {
            mem_type: captures[1].parse().map_err(SectionParseError::BadType)?,
            location: captures[2]
                .parse()
                .map_err(SectionParseError::BadLocation)?,
            align_mask: u16::from_str_radix(&captures[3], 16)
                .map_err(SectionParseError::BadAlignMask)?,
            align_ofs: u16::from_str_radix(&captures[4], 16)
                .map_err(SectionParseError::BadAlignOfs)?,
            size: (&captures[5]).parse().map_err(SectionParseError::BadSize)?,
            name: captures[6].to_string(),
        })
    }
}

pub fn parse_input() -> Result<Sequence, ParseError> {
    eprint!("Parsing input...\r");

    let mut nb_banks = 2;
    let mut frames = Vec::new();
    let mut sections = Vec::new();

    let stdin = io::stdin();
    let mut line = String::new();
    let mut line_no = 0;
    while {
        line.clear();
        stdin.read_line(&mut line)? != 0
    } {
        line_no += 1;

        // Ignore leading whitespace (but not trailing, as it might be significant)
        let line = line.trim_start();
        let line = line.strip_suffix('\n').unwrap_or(line);
        let line = line.strip_suffix('\r').unwrap_or(line);
        // Ignore empty lines
        if line.is_empty() {
            continue;
        }

        match line.strip_prefix('[') {
            // New section
            Some(rest) => {
                let section: Section = rest.parse().map_err(|err_type| {
                    ParseError::BadSection(err_type, line_no, line.to_string())
                })?;

                sections.push(section);
            }

            // New attempt within a section
            None => {
                let location: Location = line.parse().map_err(|err_type| {
                    ParseError::BadAttempt(err_type, line_no, line.to_string())
                })?;
                let section_id = sections
                    .len()
                    .checked_sub(1)
                    .ok_or_else(|| ParseError::AttemptBeforeSection(line_no, line.to_string()))?;

                let section = &sections[section_id];
                match section.mem_type {
                    MemType::Romx => {
                        if location.bank >= nb_banks {
                            nb_banks = location.bank.next_power_of_two();
                        }
                    }
                    MemType::Rom0 => (),
                    _ => continue,
                }

                frames.push(Frame {
                    location,
                    section_id,
                });
            }
        }
    }

    eprintln!("Parsing input - Done.");

    Ok(Sequence {
        nb_banks,
        frames,
        sections,
    })
}
