extern crate pgn_reader;
extern crate arrayvec;
extern crate memmap;
extern crate madvise;
extern crate shakmaty;
extern crate huffman_compress;
extern crate spsa;
extern crate float_cmp;

use pgn_reader::{Visitor, Skip, Reader, San};

use spsa::{HyperParameters};

use huffman_compress::{Tree, Book, codebook};

use arrayvec::ArrayVec;

use shakmaty::{Chess, Role, Position, Setup, MoveList, Square, Move, Color, Piece};

use float_cmp::ApproxOrdUlps;

use memmap::Mmap;
use madvise::{AccessPattern, AdviseMemory};

use std::env;
use std::fs::File;
use std::collections::HashMap;

struct Histogram {
    counts: [u64; 256],
    pos: Chess,
    skip: bool,
    theta: [f64; 6]
}

impl Histogram {
    fn new(theta: [f64; 6]) -> Histogram {
        Histogram {
            counts: [0; 256],
            pos: Chess::default(),
            skip: false,
            theta,
        }
    }

    fn huffman(&self) -> (Book<u8>, Tree<u8>) {
        let weights: HashMap<_, _> = self.counts.iter()
            .enumerate()
            .map(|(k, v)| (k as u8, v + 1))
            .collect();

        codebook(&weights)
    }

    fn bits(&self) -> u64 {
        let (book, _) = self.huffman();

        self.counts.iter()
            .enumerate()
            .map(|(k, v)| book.get(&(k as u8)).map_or(0, |c| c.len() as u64 * v))
            .sum()
    }
}

impl<'pgn> Visitor<'pgn> for Histogram {
    type Result = ();

    fn begin_game(&mut self) {
        self.pos = Chess::default();
        self.skip = false;
    }

    fn header(&mut self, key: &'pgn [u8], _value: &'pgn [u8]) {
        if key == b"FEN" {
            self.skip = true;
        }
    }

    fn end_headers(&mut self) -> Skip {
        Skip(self.skip)
    }

    fn begin_variation(&mut self) -> Skip {
        Skip(true)
    }

    fn san(&mut self, san: San) {
        if !self.skip {
            let mut legals = MoveList::new();
            self.pos.legal_moves(&mut legals);

            let mut augmented: ArrayVec<[(&Move, (_)); 512]> = legals.iter().map(|m| {
                let score =
                    self.theta[0] * (m.promotion().unwrap_or(Role::Pawn) as u8 as f64 / 6.0) +
                    self.theta[1] * (m.is_capture() as u8 as f64) +
                    self.theta[2] * (poor_mans_see(&self.pos, m) as f64 / 6.0) +
                    self.theta[3] * ((move_value(self.pos.turn(), m) + 500) as f64 / 1000.0) +
                    self.theta[4] * (u32::from(m.to()) as f64 / 64.0) +
                    self.theta[5] * (u32::from(m.from().expect("no drops")) as f64 / 64.0);
                (m, score)
            }).collect();

            augmented.sort_unstable_by(|a, b| b.1.approx_cmp(&a.1, 1));

            let idx = match augmented.iter().position(|a| san.matches(a.0)) {
                Some(idx) => idx,
                None => {
                    eprintln!("illegal san: {}", san);
                    self.skip = true;
                    return;
                }
            };

            self.counts[idx] += 1;

            self.pos.play_unchecked(&augmented[idx].0);
        }
    }

    fn end_game(&mut self, _game: &'pgn [u8]) { }
}

fn piece_value(piece: Piece, square: Square) -> i16 {
    let sq = if piece.color.is_white() { square.flip_vertical() } else { square };
    PSQT[piece.role as usize][usize::from(sq)]
}

fn move_value(turn: Color, m: &Move) -> i16 {
    let role = m.role();
    piece_value(role.of(turn), m.to()) - piece_value(role.of(turn), m.from().expect("no drops"))
}

fn poor_mans_see(pos: &Chess, m: &Move) -> u32 {
    if (shakmaty::attacks::pawn_attacks(pos.turn(), m.to()) & pos.board().pawns() & pos.them()).any() {
        5 - m.role() as u32
    } else {
        6
    }
}

static PSQT: [[i16; 64]; 6] = [
    [
         0,  0,  0,  0,  0,  0,  0,  0,
        50, 50, 50, 50, 50, 50, 50, 50,
        10, 10, 20, 30, 30, 20, 10, 10,
         5,  5, 10, 25, 25, 10,  5,  5,
         0,  0,  0, 20, 21,  0,  0,  0,
         5, -5,-10,  0,  0,-10, -5,  5,
         5, 10, 10,-31,-31, 10, 10,  5,
         0,  0,  0,  0,  0,  0,  0,  0
    ],
    [
        -50,-40,-30,-30,-30,-30,-40,-50,
        -40,-20,  0,  0,  0,  0,-20,-40,
        -30,  0, 10, 15, 15, 10,  0,-30,
        -30,  5, 15, 20, 20, 15,  5,-30,
        -30,  0, 15, 20, 20, 15,  0,-30,
        -30,  5, 10, 15, 15, 11,  5,-30,
        -40,-20,  0,  5,  5,  0,-20,-40,
        -50,-40,-30,-30,-30,-30,-40,-50
    ],
    [
        -20,-10,-10,-10,-10,-10,-10,-20,
        -10,  0,  0,  0,  0,  0,  0,-10,
        -10,  0,  5, 10, 10,  5,  0,-10,
        -10,  5,  5, 10, 10,  5,  5,-10,
        -10,  0, 10, 10, 10, 10,  0,-10,
        -10, 10, 10, 10, 10, 10, 10,-10,
        -10,  5,  0,  0,  0,  0,  5,-10,
        -20,-10,-10,-10,-10,-10,-10,-20
    ],
    [
          0,  0,  0,  0,  0,  0,  0,  0,
          5, 10, 10, 10, 10, 10, 10,  5,
         -5,  0,  0,  0,  0,  0,  0, -5,
         -5,  0,  0,  0,  0,  0,  0, -5,
         -5,  0,  0,  0,  0,  0,  0, -5,
         -5,  0,  0,  0,  0,  0,  0, -5,
         -5,  0,  0,  0,  0,  0,  0, -5,
          0,  0,  0,  5,  5,  0,  0,  0
    ],
    [
        -20,-10,-10, -5, -5,-10,-10,-20,
        -10,  0,  0,  0,  0,  0,  0,-10,
        -10,  0,  5,  5,  5,  5,  0,-10,
         -5,  0,  5,  5,  5,  5,  0, -5,
          0,  0,  5,  5,  5,  5,  0, -5,
        -10,  5,  5,  5,  5,  5,  0,-10,
        -10,  0,  5,  0,  0,  0,  0,-10,
        -20,-10,-10, -5, -5,-10,-10,-20
    ],
    [
        -30,-40,-40,-50,-50,-40,-40,-30,
        -30,-40,-40,-50,-50,-40,-40,-30,
        -30,-40,-40,-50,-50,-40,-40,-30,
        -30,-40,-40,-50,-50,-40,-40,-30,
        -20,-30,-30,-40,-40,-30,-30,-20,
        -10,-20,-20,-20,-20,-20,-20,-10,
         20, 20,  0,  0,  0,  0, 20, 20,
          1, 30, 10,  0,  0, 10, 30,  2
    ]
];

fn main() {
    let arg = env::args().skip(1).next().expect("pgn file as argument");
    eprintln!("reading {} ...", arg);
    let file = File::open(&arg).expect("fopen");
    let mmap = unsafe { Mmap::map(&file).expect("mmap") };
    let mut pgn = &mmap[..];
    pgn.advise_memory_access(AccessPattern::Sequential).expect("madvise");

    let batch_size = 100;

    let mut spsa = HyperParameters::default().spsa();

    for k in 0..1000 {
        spsa.step(&mut |theta| {
            let mut histogram = Histogram::new(theta);

            {
                let mut reader = Reader::new(&mut histogram, pgn);

                pgn = reader.remaining_pgn();

                for _ in 0..batch_size {
                    reader.read_game();
                }
            }

            let bytes = histogram.bits() as f64 / batch_size as f64 / 8.0;
            println!("k={} bytes={} theta={:?}", k, bytes, theta);
            bytes
        });
    }
}
