extern crate pgn_reader;
extern crate arrayvec;
extern crate memmap;
extern crate madvise;
extern crate shakmaty;
extern crate huffman_compress;

use pgn_reader::{Visitor, Skip, Reader, San};

use huffman_compress::{Tree, Book, codebook};

use arrayvec::ArrayVec;

use shakmaty::{Chess, Role, Position, Setup, MoveList, Square, Move, Color, Piece, Bitboard};

use memmap::Mmap;
use madvise::{AccessPattern, AdviseMemory};

use std::env;
use std::fs::File;
use std::collections::HashMap;

struct Histogram {
    counts: [u64; 256],
    pos: Chess,
    skip: bool,
}

impl Histogram {
    fn new() -> Histogram {
        Histogram {
            counts: [0; 256],
            pos: Chess::default(),
            skip: false,
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

            // add all pseudo legal king moves
            let our_king = self.pos.board().king_of(self.pos.turn()).expect("has king");
            let mut king_ring = shakmaty::attacks::king_attacks(our_king) & !self.pos.us();

            for m in &legals {
                if m.role() == Role::King {
                    king_ring.discard(m.to());
                }
            }

            for sq in king_ring {
                legals.push(Move::Normal {
                    role: Role::King,
                    from: our_king,
                    capture: self.pos.board().role_at(sq),
                    to: sq,
                    promotion: None,
                });
            }

            let mut augmented: ArrayVec<[(&Move, (_)); 512]> = legals.iter().map(|m| {
                let score =
                    ((m.promotion().unwrap_or(Role::Pawn) as u32) << 26) +
                    ((m.is_capture() as u32) << 25) +
                    (poor_mans_see(&self.pos, m) << 22) +
                    (((512 + move_value(self.pos.turn(), m)) as u32) << 12) +
                    (u32::from(m.to()) << 6) +
                    u32::from(m.from().expect("no drops"));

                (m, score)
            }).collect();

            augmented.sort_unstable_by(|a, b| b.1.cmp(&a.1));

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
    let mut histogram = Histogram::new();
    let mut num_games = 0;

    for arg in env::args().skip(1) {
        eprintln!("reading {} ...", arg);
        let file = File::open(&arg).expect("fopen");
        let pgn = unsafe { Mmap::map(&file).expect("mmap") };
        pgn.advise_memory_access(AccessPattern::Sequential).expect("madvise");

        num_games += Reader::new(&mut histogram, &pgn[..]).into_iter().count();
        let bits = histogram.bits();

        println!("histogram = {:?}", &histogram.counts[..]);
        println!("num_games = {}", num_games);
        println!("# {} bytes per game", bits as f64 / num_games as f64 / 8.0);
    }
}
