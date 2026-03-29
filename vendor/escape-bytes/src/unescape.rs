use core::borrow::Borrow;

/// Unescape the bytes previously escaped.
///
/// See [crate] for the exact rules.
///
/// ## Errors
///
/// When encountering unexpected byte sequences.
///
/// ## Example
///
/// ```rust
/// # fn main() -> Result<(), escape_bytes::UnescapeError> {
/// let escaped = br"hello\xc3world";
/// let unescaped = escape_bytes::unescape(escaped)?;
/// assert_eq!(unescaped, b"hello\xc3world");
/// # Ok(())
/// # }
/// ```
#[cfg(feature = "alloc")]
#[cfg_attr(feature = "doc", doc(cfg(feature = "alloc")))]
pub fn unescape<I>(i: I) -> Result<alloc::vec::Vec<u8>, UnescapeError>
where
    I: IntoIterator,
    I::Item: Borrow<u8>,
{
    let mut escaped = alloc::vec::Vec::<u8>::new();
    for b in Unescape::new(i) {
        let b = b?;
        escaped.push(b);
    }
    Ok(escaped)
}

/// Escape into error occurs when escaping into a slice cannot continue.
#[derive(Debug, PartialEq, Eq)]
pub enum UnescapeIntoError {
    /// Writing into the slice would write to a position that is out-of-bounds.
    OutOfBounds,
    /// Occurs when encountering unexpected byte sequences.
    Unescape(UnescapeError),
}

/// Unescape the bytes into the slice.
///
/// See [crate] for the exact rules.
///
/// Returns the number of bytes written to the slice.
///
/// ## Errors
///
/// If the slice is not large enough to receive the escaped value. No
/// information is provided to support continuing escaping into a new buffer
/// from where it stops. Use the [`Unescape`] iterator directly if that is needed.
///
/// When encountering unexpected byte sequences.
pub fn unescape_into<I>(out: &mut [u8], i: I) -> Result<usize, UnescapeIntoError>
where
    I: IntoIterator,
    I::Item: Borrow<u8>,
{
    let mut count = 0usize;
    for (idx, b) in Unescape::new(i).enumerate() {
        let b = b.map_err(UnescapeIntoError::Unescape)?;
        let Some(v) = out.get_mut(idx) else {
            return Err(UnescapeIntoError::OutOfBounds);
        };
        *v = b;
        count += 1;
    }
    Ok(count)
}

/// Returns the unescaped length of the input.
///
/// ## Errors
///
/// When encountering unexpected byte sequences.
pub fn unescaped_len<I>(i: I) -> Result<usize, UnescapeError>
where
    I: IntoIterator,
    I::Item: Borrow<u8>,
{
    Unescape::new(i).try_fold(0usize, |sum, result| {
        result?;
        Ok(sum + 1)
    })
}

/// Iterator that unescapes the input iterator.
///
/// See [crate] for the exact rules.
///
/// Use [`unescape`] or [`unescape_into`].
#[derive(Debug)]
pub struct Unescape<I>
where
    I: IntoIterator,
{
    input: I::IntoIter,
}

impl<I> Clone for Unescape<I>
where
    I: IntoIterator,
    I::IntoIter: Clone,
{
    fn clone(&self) -> Self {
        Self {
            input: self.input.clone(),
        }
    }
}

impl<I> Unescape<I>
where
    I: IntoIterator,
{
    pub fn new(i: I) -> Self {
        Self {
            input: i.into_iter(),
        }
    }
}

/// Unescape error occurs when encountering unexpected byte sequences.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum UnescapeError {
    /// An escape backslash character (`\`) was followed by a character that was
    /// not a `\`, `0`, `t`, `r`, `n`, or `x`.
    InvalidEscape,
    /// An escape backslash character and x indicating a hex escape (`\x`) were
    /// followed by a character that was not a valid hex character
    /// (0123456789abcdef).
    InvalidHexHi,
    /// An escape backslash character, x indicating a hex escape, and the hi
    /// nibble (`\xN`) were followed by a character that was not a valid hex
    /// character (0123456789abcdef).
    InvalidHexLo,
}

impl<I> Iterator for Unescape<I>
where
    I: IntoIterator,
    I::Item: Borrow<u8>,
{
    type Item = Result<u8, UnescapeError>;

    /// Returns the next unescaped byte.
    fn next(&mut self) -> Option<Self::Item> {
        enum Next {
            New,
            Escape,
            EscapeHexHi,
            EscapeHexLo(u8),
        }
        let mut state = Next::New;
        loop {
            let Some(b) = self.input.next() else {
                return match state {
                    Next::New => None,
                    Next::Escape => Some(Err(UnescapeError::InvalidEscape)),
                    Next::EscapeHexHi => Some(Err(UnescapeError::InvalidHexHi)),
                    Next::EscapeHexLo(_) => Some(Err(UnescapeError::InvalidHexLo)),
                };
            };
            let b = *b.borrow();
            match state {
                Next::New => match b {
                    b'\\' => state = Next::Escape,
                    _ => return Some(Ok(b)),
                },
                Next::Escape => match b {
                    b'0' => return Some(Ok(b'\0')),
                    b't' => return Some(Ok(b'\t')),
                    b'n' => return Some(Ok(b'\n')),
                    b'r' => return Some(Ok(b'\r')),
                    b'\\' => return Some(Ok(b'\\')),
                    b'x' => state = Next::EscapeHexHi,
                    _ => return Some(Err(UnescapeError::InvalidEscape)),
                },
                Next::EscapeHexHi => {
                    let Some(hi) = HEX_ALPHABET_REVERSE_MAP[b as usize] else {
                        return Some(Err(UnescapeError::InvalidHexHi));
                    };
                    state = Next::EscapeHexLo(hi);
                }
                Next::EscapeHexLo(hi) => {
                    let Some(lo) = HEX_ALPHABET_REVERSE_MAP[b as usize] else {
                        return Some(Err(UnescapeError::InvalidHexLo));
                    };
                    return Some(Ok(hi << 4 | lo));
                }
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let input_hint = self.input.size_hint();
        (input_hint.0 / 4, input_hint.1)
    }
}

#[rustfmt::skip]
const HEX_ALPHABET_REVERSE_MAP: [Option<u8>; 256] = [
    None,None,None,None,None,None,None,None,
    None,None,None,None,None,None,None,None,
    None,None,None,None,None,None,None,None,
    None,None,None,None,None,None,None,None,
    None,None,None,None,None,None,None,None,
    None,None,None,None,None,None,None,None,
    // 0..=9
    Some(0x0), Some(0x1),Some(0x2),Some(0x3),Some(0x4),Some(0x5),Some(0x6),Some(0x7),Some(0x8),Some(0x9),
    // :..=@
    None,None,None,None,None,None,None,
    // A..=F
    Some(0xA), Some(0xB),Some(0xC),Some(0xD),Some(0xE),Some(0xF),
    // G..=Z
    None,None,None,None,None,None,None,None,
    None,None,None,None,None,None,None,None,
    None,None,None,None,
    // [..=`
    None,None,None,None,None,None,
    // a..=f
    Some(0xa), Some(0xb),Some(0xc),Some(0xd),Some(0xe),Some(0xf),
    // g..=z
    None,None,None,None,None,None,None,None,
    None,None,None,None,None,None,None,None,
    None,None,None,None,
    // {..=DEL
    None,None,None,None,None,
    // 0x128..
    None,None,None,None,None,None,None,None,
    None,None,None,None,None,None,None,None,
    None,None,None,None,None,None,None,None,
    None,None,None,None,None,None,None,None,
    None,None,None,None,None,None,None,None,
    None,None,None,None,None,None,None,None,
    None,None,None,None,None,None,None,None,
    None,None,None,None,None,None,None,None,
    None,None,None,None,None,None,None,None,
    None,None,None,None,None,None,None,None,
    None,None,None,None,None,None,None,None,
    None,None,None,None,None,None,None,None,
    None,None,None,None,None,None,None,None,
    None,None,None,None,None,None,None,None,
    None,None,None,None,None,None,None,None,
    None,None,None,None,None,None,None,None,
];

#[cfg(test)]
mod test {}
