// Computes the average branching factor in game positions.
// Usage: cargo run --release --example branching -- [PGN]...

extern crate pgn_reader;
extern crate memmap;
extern crate madvise;
extern crate shakmaty;
extern crate average;

use pgn_reader::{Visitor, Skip, Reader, San};

use shakmaty::{Chess, Position, MoveList};
use shakmaty::fen::Fen;

use memmap::Mmap;
use madvise::{AccessPattern, AdviseMemory};

use average::{Variance, Quantile, Estimate};

use std::env;
use std::fs::File;

struct Branching {
    variance: Variance,
    quantile: Quantile,
    games: usize,
    pos: Chess,
    success: bool,
}

impl Branching {
    fn new() -> Branching {
        Branching {
            variance: Variance::new(),
            quantile: Quantile::new(0.99),
            games: 0,
            pos: Chess::default(),
            success: false,
        }
    }

    fn sample(&mut self) {
        let mut legals = MoveList::new();
        self.pos.legal_moves(&mut legals);
        self.variance.add(legals.len() as f64);
        self.quantile.add(legals.len() as f64);
    }
}

impl<'pgn> Visitor<'pgn> for Branching {
    type Result = ();

    fn begin_game(&mut self) {
        self.games += 1;
        self.pos = Chess::default();
        self.success = true;
    }

    fn header(&mut self, key: &'pgn [u8], value: &'pgn [u8]) {
        // Support games from a non-standard starting position.
        if key == b"FEN" {
            let fen = match Fen::from_bytes(value) {
                Ok(fen) => fen,
                Err(err) => {
                    eprintln!("invalid fen header in game {}: {} ({:?})", self.games, err, value);
                    self.success = false;
                    return;
                },
            };

            self.pos = match fen.position() {
                Ok(pos) => pos,
                Err(err) => {
                    eprintln!("illegal fen header in game {}: {} ({})", self.games, err, fen);
                    self.success = false;
                    return;
                },
            };
        }
    }

    fn end_headers(&mut self) -> Skip {
        if self.success {
            self.sample();
        }

        Skip(!self.success)
    }

    fn begin_variation(&mut self) -> Skip {
        Skip(true) // stay in the mainline
    }

    fn san(&mut self, san: San) {
        if self.success {
            match san.to_move(&self.pos) {
                Ok(m) => {
                    self.pos.play_unchecked(&m);
                    self.sample();
                },
                Err(err) => {
                    eprintln!("error in game {}: {} {}", self.games, err, san);
                    self.success = false;
                },
            }
        }
    }

    fn end_game(&mut self, _game: &'pgn [u8]) -> Self::Result { }
}

fn main() {
    for arg in env::args().skip(1) {
        let file = File::open(&arg).expect("fopen");
        let pgn = unsafe { Mmap::map(&file).expect("mmap") };
        pgn.advise_memory_access(AccessPattern::Sequential).expect("madvise");

        let mut branching = Branching::new();
        Reader::new(&mut branching, &pgn[..]).read_all();
        println!("{}: branching {} ± {}, p=0.99: {}", arg, branching.variance.mean(), branching.variance.sample_variance().sqrt(), branching.quantile.quantile());
    }
}
