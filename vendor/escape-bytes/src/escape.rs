use core::borrow::Borrow;

/// Escape the bytes.
///
/// See [crate] for the exact rules.
///
/// ## Example
///
/// ```rust
/// let str = b"hello\xc3world";
/// let escaped = escape_bytes::escape(str);
/// assert_eq!(escaped, br"hello\xc3world");
/// ```
#[cfg(feature = "alloc")]
#[cfg_attr(feature = "doc", doc(cfg(feature = "alloc")))]
pub fn escape<I>(i: I) -> alloc::vec::Vec<u8>
where
    I: IntoIterator,
    I::Item: Borrow<u8>,
{
    let mut escaped = alloc::vec::Vec::<u8>::new();
    for b in Escape::new(i) {
        escaped.push(b);
    }
    escaped
}

/// Escape into error occurs when escaping into a slice cannot continue.
#[derive(Debug, PartialEq, Eq)]
pub enum EscapeIntoError {
    /// Writing into the slice would write to a position that is out-of-bounds.
    OutOfBounds,
}

/// Escape the bytes into the slice.
///
/// See [crate] for the exact rules.
///
/// Returns the number of bytes written to the slice.
///
/// ## Errors
///
/// If the slice is not large enough to receive the escaped value. No
/// information is provided to support continuing escaping into a new buffer
/// from where it stops. Use the [`Escape`] iterator directly if that is needed.
pub fn escape_into<I>(out: &mut [u8], i: I) -> Result<usize, EscapeIntoError>
where
    I: IntoIterator,
    I::Item: Borrow<u8>,
{
    let mut count = 0usize;
    for (idx, b) in Escape::new(i).enumerate() {
        let Some(v) = out.get_mut(idx) else {
            return Err(EscapeIntoError::OutOfBounds);
        };
        *v = b;
        count += 1;
    }
    Ok(count)
}

/// Returns the maximum escaped length of the given unescaped length.
#[must_use]
pub const fn escaped_max_len(len: usize) -> Option<usize> {
    len.checked_mul(4)
}

/// Returns the escaped length of the input.
pub fn escaped_len<I>(i: I) -> usize
where
    I: IntoIterator,
    Escape<I>: Iterator,
{
    Escape::new(i).count()
}

/// Iterator that escapes the input iterator.
///
/// See [crate] for the exact rules.
///
/// Use [`escape`] or [`escape_into`].
#[derive(Debug)]
pub struct Escape<I>
where
    I: IntoIterator,
{
    next: Next,
    input: I::IntoIter,
}

impl<I> Clone for Escape<I>
where
    I: IntoIterator,
    I::IntoIter: Clone,
{
    fn clone(&self) -> Self {
        Self {
            next: self.next.clone(),
            input: self.input.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Next {
    Input,
    Byte1(u8),
    Byte2(u8, u8),
    Byte3(u8, u8, u8),
}

impl<I> Escape<I>
where
    I: IntoIterator,
{
    pub fn new(i: I) -> Self {
        Self {
            next: Next::Input,
            input: i.into_iter(),
        }
    }
}

impl<I> Iterator for Escape<I>
where
    I: IntoIterator,
    I::Item: Borrow<u8>,
{
    type Item = u8;

    /// Returns the next escaped byte.
    fn next(&mut self) -> Option<Self::Item> {
        match self.next {
            Next::Input => {
                let Some(b) = self.input.next() else {
                    return None;
                };
                let b = *b.borrow();
                match b {
                    // Backslash is rendered as double backslash.
                    b'\\' => {
                        self.next = Next::Byte1(b'\\');
                        Some(b'\\')
                    }
                    // Nul is rendered as backslash 0.
                    b'\0' => {
                        self.next = Next::Byte1(b'0');
                        Some(b'\\')
                    }
                    // Tab is rendered as backslash t.
                    b'\t' => {
                        self.next = Next::Byte1(b't');
                        Some(b'\\')
                    }
                    // Carriage return is rendered as backslash r.
                    b'\r' => {
                        self.next = Next::Byte1(b'r');
                        Some(b'\\')
                    }
                    // Line feed is rendered as backslash n.
                    b'\n' => {
                        self.next = Next::Byte1(b'n');
                        Some(b'\\')
                    }
                    // All other printable ASCII characters render as their ASCII character.
                    b' '..=b'~' => Some(b),
                    // All other values rendered as an escaped hex value.
                    _ => {
                        const HEX_ALPHABET: [u8; 16] = *b"0123456789abcdef";
                        self.next = Next::Byte3(
                            b'x',
                            HEX_ALPHABET[(b >> 4) as usize],
                            HEX_ALPHABET[(b & 0xF) as usize],
                        );
                        Some(b'\\')
                    }
                }
            }
            Next::Byte1(b1) => {
                self.next = Next::Input;
                Some(b1)
            }
            Next::Byte2(b1, b2) => {
                self.next = Next::Byte1(b2);
                Some(b1)
            }
            Next::Byte3(b1, b2, b3) => {
                self.next = Next::Byte2(b2, b3);
                Some(b1)
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let input_hint = self.input.size_hint();
        (input_hint.0, input_hint.1.and_then(escaped_max_len))
    }
}
