use std::{
    cmp,
    collections::VecDeque,
    io::{self, Chain, Read},
};

pub const CAPACITY: usize = 1 << 14;

#[derive(Debug, Clone)]
pub struct Buffer<R> {
    buf: Vec<u8>,
    /// The byte currently being read.
    ///
    /// Never greater than [`Self::len`].
    index: usize,
    /// The number of bytes being read.
    ///
    /// `self.buf.len()` is like `Vec::capacity` and `self.len` is like `Vec::len`.
    ///
    /// Never greater than [`CAPACITY`].
    len: usize,
    reader: R,
}

impl<R: Read> Buffer<R> {
    /// Creates a new [`Buffer`] that can hold [`CAPACITY`] many elements.
    pub fn new(reader: R) -> Self {
        Self {
            buf: vec![0; CAPACITY],
            index: 0,
            len: 0,
            reader,
        }
    }

    /// Gets the remaining bytes in the buffer and the underlying reader.
    pub fn into_inner(mut self) -> Chain<VecDeque<u8>, R> {
        let vec = self.buf.drain(self.data_range()).collect::<VecDeque<u8>>();
        vec.chain(self.reader)
    }

    /// Equivalent to [`self.data_range().len()`](Self::data_range).
    #[inline]
    fn data_len(&self) -> usize {
        self.len - self.index
    }

    #[inline]
    /// Range from `self.index` to `self.len`.
    ///
    /// This is where [`Self::data`] lives.
    fn data_range(&self) -> std::ops::Range<usize> {
        self.index..self.len
    }

    /// Gets the data in [`Self::data_range`].
    #[inline]
    pub fn data(&self) -> &[u8] {
        debug_assert!(self.index <= self.len && self.len <= CAPACITY);

        // SAFETY: self.index <= self.len <= CAPACITY
        unsafe { self.buf.get_unchecked(self.data_range()) }
    }

    /// Returns the first item in [`Self::data`].
    #[inline]
    pub fn peek(&self) -> Option<u8> {
        self.data().first().copied()
    }

    /// Sets `self.index` and `self.len` to 0.
    #[inline]
    pub fn discard_data(&mut self) {
        self.index = 0;
        self.len = 0;
    }

    /// Advances `self.index` by `n` up to `self.len`.
    #[inline]
    pub fn consume(&mut self, n: usize) {
        self.index = cmp::min(self.index + n, self.len);
    }

    /// Like [`self.consume(1)`](Self::consume).
    #[inline]
    pub fn bump(&mut self) {
        self.consume(1);
    }

    /// Reads up to the specified amount of bytes to [`Self::data`] and returns [`Self::data`].
    /// The only situation where this reads less than `n` bytes into [`Self::data`] is if
    /// EOF is reached.
    ///
    /// `n` must be smaller or equal to [`CAPACITY`].
    /// If it's bigger and `reader` doesn't reach EOF before `n` bytes are read, this function
    /// will loop infinitely.
    pub fn ensure_bytes(&mut self, n: usize) -> io::Result<&[u8]> {
        debug_assert!(n <= CAPACITY);

        if self.index > 0 {
            self.backshift();
        }

        while self.data_len() < n {
            let len = self.reader.read(&mut self.buf[self.len..])?;

            // EOF
            if len == 0 {
                break;
            }

            self.len += len;
        }

        Ok(self.data())
    }

    /// Moves [`Self::data`] to the beginning.
    fn backshift(&mut self) {
        let data_range = self.data_range();
        self.index = 0;
        self.len = data_range.len();
        self.buf.copy_within(data_range, 0);
    }
}

impl<R: Read> AsRef<[u8]> for Buffer<R> {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.data()
    }
}
