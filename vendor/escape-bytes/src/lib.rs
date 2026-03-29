//! Escapes bytes that are not printable ASCII characters.
//!
//! The exact rules are:
//! - Nul is escaped as `\0`.
//! - Tab is escaped as `\t`.
//! - Line feed is escaped as `\n`.
//! - Carriage return is escaed as `\r`.
//! - Backslach is escaped as `\\`.
//! - Any character in the printable ASCII range `0x20`..=`0x7e` is not escaped.
//! - Any other character is hex escaped in the form `\xNN`.
//!
//! Intended for use where byte sequences are not valid ASCII or UTF-8 but need
//! to be stored in a semi-human readable form where only ASCII or UTF-8 are
//! permitted.
//!
//! ## Examples
//!
//! ### Escape
//!
//! ```rust
//! let str = b"hello\xc3world";
//! let escaped = escape_bytes::escape(str);
//! assert_eq!(escaped, br"hello\xc3world");
//! ```
//!
//! ### Unescape
//!
//! ```rust
//! # fn main() -> Result<(), escape_bytes::UnescapeError> {
//! let escaped = br"hello\xc3world";
//! let unescaped = escape_bytes::unescape(escaped)?;
//! assert_eq!(unescaped, b"hello\xc3world");
//! # Ok(())
//! # }
//! ```

#![no_std]
#![allow(clippy::module_name_repetitions)]

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(test)]
mod test;

mod escape;
pub use escape::*;

mod unescape;
pub use unescape::*;
