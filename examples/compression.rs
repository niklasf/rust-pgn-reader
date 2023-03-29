extern crate arrayvec;
extern crate huffman_compress;
extern crate itertools;
extern crate madvise;
extern crate memmap;
extern crate pgn_reader;
extern crate shakmaty;

use pgn_reader::{Reader, San, Skip, Visitor};

use huffman_compress::{codebook, Book, Tree};

use arrayvec::ArrayVec;

use itertools::Itertools;

use shakmaty::{Chess, Color, Move, MoveList, Piece, Position, Role, Setup, Square};

use madvise::{AccessPattern, AdviseMemory};
use memmap::Mmap;

use std::collections::HashMap;
use std::env;
use std::fs::File;

struct Histogram {
    counts_by_game_phase: [[u64; 256]; 3],
    pos: Chess,
    ply: u64,
    skip: bool,
}

impl Histogram {
    fn new() -> Histogram {
        Histogram {
            counts_by_game_phase: [[0; 256]; 3],
            pos: Chess::default(),
            ply: 0,
            skip: false,
        }
    }

    fn huffman(&self) -> (Book<u8>, Tree<u8>) {
        let weights: HashMap<_, _> = self
            .counts_by_game_phase
            .iter()
            .flatten()
            .enumerate()
            .map(|(k, v)| (k as u8, v + 1))
            .collect();

        codebook(&weights)
    }

    fn bits(&self) -> Vec<u64> {
        let (book, _) = self.huffman();

        self.counts_by_game_phase
            .iter()
            .enumerate()
            .map(|(i, counts)| {
                counts
                    .iter()
                    .enumerate()
                    .map(|(k, v)| {
                        book.get(&((k + i * 256) as u8))
                            .map_or(0, |c| c.len() as u64 * v)
                    })
                    .sum()
            })
            .collect()
    }

    fn codes(&self) -> Vec<Vec<(u64, usize)>> {
        let (book, _) = self.huffman();

        (0usize..3)
            .map(|i| {
                (((i * 256) as usize)..((i + 1) * 256))
                    .map(|k| {
                        book.get(&(k as u8)).map_or((0u64, 0usize), |c| {
                            (
                                (0usize..c.len())
                                    .map(|i| (c[i] as u64) * (1 << (c.len() - 1 - i)))
                                    .sum(),
                                c.len(),
                            )
                        })
                    })
                    .sorted_by_key(|k| k.1)
                    .collect::<Vec<(u64, usize)>>()
            })
            .collect::<Vec<Vec<(u64, usize)>>>()
    }
}

impl<'pgn> Visitor<'pgn> for Histogram {
    type Result = ();

    fn begin_game(&mut self) {
        self.pos = Chess::default();
        self.ply = 0;
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
            self.ply += 1;

            let mut augmented: ArrayVec<[(&Move, _); 512]> = legals
                .iter()
                .map(|m| {
                    let score = ((m.promotion().unwrap_or(Role::Pawn) as u32) << 26)
                        + ((m.is_capture() as u32) << 25)
                        + (poor_mans_see(&self.pos, m) << 22)
                        + (((512 + move_value(self.pos.turn(), m)) as u32) << 12)
                        + (u32::from(m.to()) << 6)
                        + u32::from(m.from().expect("no drops"));

                    (m, score)
                })
                .collect();

            augmented.sort_unstable_by(|a, b| b.1.cmp(&a.1));

            let idx = match augmented.iter().position(|a| san.matches(a.0)) {
                Some(idx) => idx,
                None => {
                    eprintln!("illegal san: {}", san);
                    self.skip = true;
                    return;
                }
            };

            if self.ply <= 40 {
                self.counts_by_game_phase[0][idx] += 1;
            } else if 41 <= self.ply && self.ply <= 80 {
                self.counts_by_game_phase[1][idx] += 1;
            } else {
                self.counts_by_game_phase[2][idx] += 1;
            }
            self.pos.play_unchecked(&augmented[idx].0);
        }
    }

    fn end_game(&mut self, _game: &'pgn [u8]) {}
}

fn piece_value(piece: Piece, square: Square) -> i16 {
    let sq = if piece.color.is_white() {
        square.flip_vertical()
    } else {
        square
    };
    PSQT[piece.role as usize][usize::from(sq)]
}

fn move_value(turn: Color, m: &Move) -> i16 {
    let role = m.role();
    piece_value(role.of(turn), m.to()) - piece_value(role.of(turn), m.from().expect("no drops"))
}

fn poor_mans_see(pos: &Chess, m: &Move) -> u32 {
    if (shakmaty::attacks::pawn_attacks(pos.turn(), m.to()) & pos.board().pawns() & pos.them())
        .any()
    {
        5 - m.role() as u32
    } else {
        6
    }
}

static PSQT: [[i16; 64]; 6] = [
    [
        0, 0, 0, 0, 0, 0, 0, 0, 50, 50, 50, 50, 50, 50, 50, 50, 10, 10, 20, 30, 30, 20, 10, 10, 5,
        5, 10, 25, 25, 10, 5, 5, 0, 0, 0, 20, 21, 0, 0, 0, 5, -5, -10, 0, 0, -10, -5, 5, 5, 10, 10,
        -31, -31, 10, 10, 5, 0, 0, 0, 0, 0, 0, 0, 0,
    ],
    [
        -50, -40, -30, -30, -30, -30, -40, -50, -40, -20, 0, 0, 0, 0, -20, -40, -30, 0, 10, 15, 15,
        10, 0, -30, -30, 5, 15, 20, 20, 15, 5, -30, -30, 0, 15, 20, 20, 15, 0, -30, -30, 5, 10, 15,
        15, 11, 5, -30, -40, -20, 0, 5, 5, 0, -20, -40, -50, -40, -30, -30, -30, -30, -40, -50,
    ],
    [
        -20, -10, -10, -10, -10, -10, -10, -20, -10, 0, 0, 0, 0, 0, 0, -10, -10, 0, 5, 10, 10, 5,
        0, -10, -10, 5, 5, 10, 10, 5, 5, -10, -10, 0, 10, 10, 10, 10, 0, -10, -10, 10, 10, 10, 10,
        10, 10, -10, -10, 5, 0, 0, 0, 0, 5, -10, -20, -10, -10, -10, -10, -10, -10, -20,
    ],
    [
        0, 0, 0, 0, 0, 0, 0, 0, 5, 10, 10, 10, 10, 10, 10, 5, -5, 0, 0, 0, 0, 0, 0, -5, -5, 0, 0,
        0, 0, 0, 0, -5, -5, 0, 0, 0, 0, 0, 0, -5, -5, 0, 0, 0, 0, 0, 0, -5, -5, 0, 0, 0, 0, 0, 0,
        -5, 0, 0, 0, 5, 5, 0, 0, 0,
    ],
    [
        -20, -10, -10, -5, -5, -10, -10, -20, -10, 0, 0, 0, 0, 0, 0, -10, -10, 0, 5, 5, 5, 5, 0,
        -10, -5, 0, 5, 5, 5, 5, 0, -5, 0, 0, 5, 5, 5, 5, 0, -5, -10, 5, 5, 5, 5, 5, 0, -10, -10, 0,
        5, 0, 0, 0, 0, -10, -20, -10, -10, -5, -5, -10, -10, -20,
    ],
    [
        -30, -40, -40, -50, -50, -40, -40, -30, -30, -40, -40, -50, -50, -40, -40, -30, -30, -40,
        -40, -50, -50, -40, -40, -30, -30, -40, -40, -50, -50, -40, -40, -30, -20, -30, -30, -40,
        -40, -30, -30, -20, -10, -20, -20, -20, -20, -20, -20, -10, 20, 20, 0, 0, 0, 0, 20, 20, 1,
        30, 10, 0, 0, 10, 30, 2,
    ],
];

fn main() {
    let mut histogram = Histogram::new();
    let mut num_games = 0;

    for arg in env::args().skip(1) {
        eprintln!("reading {} ...", arg);
        let file = File::open(&arg).expect("fopen");
        let pgn = unsafe { Mmap::map(&file).expect("mmap") };
        pgn.advise_memory_access(AccessPattern::Sequential)
            .expect("madvise");

        num_games += Reader::new(&mut histogram, &pgn[..]).into_iter().count();
        let bits = histogram.bits();
        let codes = histogram.codes();

        let game_phases = vec!["Opening", "Middlegame", "Endgame"];

        for i in 0..3 {
            println!("{}", game_phases[i]);
            for k in 0..256 {
                println!(
                    "new Symbol ({:#b}, {}), // {}: {}",
                    codes[i][k].0, codes[i][k].1, i, histogram.counts_by_game_phase[i][k]
                );
            }
            println!("histogram = {:?}", &histogram.counts_by_game_phase[i][..]);
            println!(
                "# {} bytes per game",
                bits[i] as f64 / num_games as f64 / 8.0
            );
        }
        println!("num_games = {}", num_games);
    }
}
