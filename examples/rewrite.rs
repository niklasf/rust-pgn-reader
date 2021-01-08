// Rewrites a PGN.
// Usage: cargo run --release --example rewrite -- [PGN]...

use std::env;
use std::io;
use std::str;
use std::fs::File;

use pgn_reader::{BufferedReader, RawComment, RawHeader, Visitor, SanPlus, Nag, Outcome, Skip};

#[derive(Debug)]
struct Rewrite<W> {
    w: W,
    white: bool,
    ply: u32,
    after_comment: bool,
}

impl<W: io::Write> Visitor for Rewrite<W> {
    type Result = ();

    fn begin_game(&mut self) {
        self.white = true;
        self.ply = 1;
        self.after_comment = false;
    }

    fn header(&mut self, key: &[u8], value: RawHeader<'_>) {
        writeln!(self.w, "[{} \"{}\"]", str::from_utf8(key).unwrap(), str::from_utf8(value.as_bytes()).unwrap()).unwrap();
    }

    fn end_headers(&mut self) -> Skip {
        writeln!(self.w).unwrap();
        Skip(false)
    }

    fn san(&mut self, san: SanPlus) {
        if !self.white || self.ply > 1 {
            write!(self.w, " ").unwrap();
        }
        if self.white {
            write!(self.w, "{}. ", self.ply).unwrap();
        } else if self.after_comment {
            write!(self.w, "{}... ", self.ply).unwrap();
        }
        self.after_comment = false;
        write!(self.w, "{}", san).unwrap();

        self.white = !self.white;
        if self.white {
            self.ply += 1;
        }
    }

    fn nag(&mut self, nag: Nag) {
        write!(self.w, "{}", match nag {
            Nag::DUBIOUS_MOVE => "?!",
            Nag::MISTAKE => "?",
            Nag::BLUNDER => "??",
            Nag::SPECULATIVE_MOVE => "!?",
            _ => panic!("unknown nag: {:?}", nag),
        }).unwrap();
    }

    fn comment(&mut self, comment: RawComment<'_>) {
        write!(self.w, " {{{}}}", str::from_utf8(comment.as_bytes()).unwrap()).unwrap();
        self.after_comment = true;
    }

    fn begin_variation(&mut self) -> Skip {
        panic!("variation!")
    }

    fn outcome(&mut self, outcome: Option<Outcome>) {
        if !self.white || self.ply > 1 {
            write!(self.w, " ").unwrap();
        }
        match outcome {
            None => write!(self.w, "*"),
            Some(outcome) => write!(self.w, "{}", outcome),
        }.unwrap()
    }

    fn end_game(&mut self) {
        if !self.white || self.ply > 1 {
            writeln!(self.w).unwrap();
        }
        writeln!(self.w).unwrap();
    }
}

fn main() -> Result<(), io::Error> {
    for arg in env::args().skip(1) {
        let file = File::open(&arg).expect("fopen");

        let uncompressed: Box<dyn io::Read> = if arg.ends_with(".bz2") {
            Box::new(bzip2::read::MultiBzDecoder::new(file))
        } else if arg.ends_with(".xz") {
            Box::new(xz2::read::XzDecoder::new(file))
        } else if arg.ends_with(".gz") {
            Box::new(flate2::read::GzDecoder::new(file))
        } else if arg.ends_with(".lz4") {
            Box::new(lz4::Decoder::new(file)?)
        } else {
            Box::new(file)
        };

        let mut reader = BufferedReader::new(uncompressed);

        let mut stats = Rewrite { w: io::stdout(), ply: 1, white: true, after_comment: false };
        reader.read_all(&mut stats)?;
    }

    Ok(())
}
