use crate::{unescape_into, unescaped_len, UnescapeError, UnescapeIntoError};

#[cfg(feature = "alloc")]
use crate::unescape;

#[cfg(feature = "alloc")]
#[test]
#[allow(clippy::too_many_lines)]
fn test_unescape_all() {
    assert_eq!(unescape(br"\x00"), Ok(alloc::vec![0x00]));
    assert_eq!(unescape(br"\0"), Ok(alloc::vec![0x00]));
    assert_eq!(unescape(br"\x01"), Ok(alloc::vec![0x01]));
    assert_eq!(unescape(br"\x02"), Ok(alloc::vec![0x02]));
    assert_eq!(unescape(br"\x03"), Ok(alloc::vec![0x03]));
    assert_eq!(unescape(br"\x04"), Ok(alloc::vec![0x04]));
    assert_eq!(unescape(br"\x05"), Ok(alloc::vec![0x05]));
    assert_eq!(unescape(br"\x06"), Ok(alloc::vec![0x06]));
    assert_eq!(unescape(br"\x07"), Ok(alloc::vec![0x07]));
    assert_eq!(unescape(br"\x08"), Ok(alloc::vec![0x08]));
    assert_eq!(unescape(br"\x09"), Ok(alloc::vec![0x09]));
    assert_eq!(unescape(br"\t"), Ok(alloc::vec![0x09]));
    assert_eq!(unescape(br"\x0a"), Ok(alloc::vec![0x0a]));
    assert_eq!(unescape(br"\n"), Ok(alloc::vec![0x0a]));
    assert_eq!(unescape(br"\x0b"), Ok(alloc::vec![0x0b]));
    assert_eq!(unescape(br"\x0c"), Ok(alloc::vec![0x0c]));
    assert_eq!(unescape(br"\x0d"), Ok(alloc::vec![0x0d]));
    assert_eq!(unescape(br"\r"), Ok(alloc::vec![0x0d]));
    assert_eq!(unescape(br"\x0e"), Ok(alloc::vec![0x0e]));
    assert_eq!(unescape(br"\x0f"), Ok(alloc::vec![0x0f]));
    assert_eq!(unescape(br"\x10"), Ok(alloc::vec![0x10]));
    assert_eq!(unescape(br"\x11"), Ok(alloc::vec![0x11]));
    assert_eq!(unescape(br"\x12"), Ok(alloc::vec![0x12]));
    assert_eq!(unescape(br"\x13"), Ok(alloc::vec![0x13]));
    assert_eq!(unescape(br"\x14"), Ok(alloc::vec![0x14]));
    assert_eq!(unescape(br"\x15"), Ok(alloc::vec![0x15]));
    assert_eq!(unescape(br"\x16"), Ok(alloc::vec![0x16]));
    assert_eq!(unescape(br"\x17"), Ok(alloc::vec![0x17]));
    assert_eq!(unescape(br"\x18"), Ok(alloc::vec![0x18]));
    assert_eq!(unescape(br"\x19"), Ok(alloc::vec![0x19]));
    assert_eq!(unescape(br"\x1a"), Ok(alloc::vec![0x1a]));
    assert_eq!(unescape(br"\x1b"), Ok(alloc::vec![0x1b]));
    assert_eq!(unescape(br"\x1c"), Ok(alloc::vec![0x1c]));
    assert_eq!(unescape(br"\x1d"), Ok(alloc::vec![0x1d]));
    assert_eq!(unescape(br"\x1e"), Ok(alloc::vec![0x1e]));
    assert_eq!(unescape(br"\x1f"), Ok(alloc::vec![0x1f]));
    assert_eq!(unescape(br" " /*   */), Ok(alloc::vec![0x20]));
    assert_eq!(unescape(br"!" /* ! */), Ok(alloc::vec![0x21]));
    assert_eq!(unescape(br#"""# /* " */), Ok(alloc::vec![0x22]));
    assert_eq!(unescape(br"#" /* # */), Ok(alloc::vec![0x23]));
    assert_eq!(unescape(br"$" /* $ */), Ok(alloc::vec![0x24]));
    assert_eq!(unescape(br"%" /* % */), Ok(alloc::vec![0x25]));
    assert_eq!(unescape(br"&" /* & */), Ok(alloc::vec![0x26]));
    assert_eq!(unescape(br"'" /* ' */), Ok(alloc::vec![0x27]));
    assert_eq!(unescape(br"(" /* ( */), Ok(alloc::vec![0x28]));
    assert_eq!(unescape(br")" /* ) */), Ok(alloc::vec![0x29]));
    assert_eq!(unescape(br"*" /* * */), Ok(alloc::vec![0x2a]));
    assert_eq!(unescape(br"+" /* + */), Ok(alloc::vec![0x2b]));
    assert_eq!(unescape(br"," /* , */), Ok(alloc::vec![0x2c]));
    assert_eq!(unescape(br"-" /* - */), Ok(alloc::vec![0x2d]));
    assert_eq!(unescape(br"." /* . */), Ok(alloc::vec![0x2e]));
    assert_eq!(unescape(br"/" /* / */), Ok(alloc::vec![0x2f]));
    assert_eq!(unescape(br"0" /* 0 */), Ok(alloc::vec![0x30]));
    assert_eq!(unescape(br"1" /* 1 */), Ok(alloc::vec![0x31]));
    assert_eq!(unescape(br"2" /* 2 */), Ok(alloc::vec![0x32]));
    assert_eq!(unescape(br"3" /* 3 */), Ok(alloc::vec![0x33]));
    assert_eq!(unescape(br"4" /* 4 */), Ok(alloc::vec![0x34]));
    assert_eq!(unescape(br"5" /* 5 */), Ok(alloc::vec![0x35]));
    assert_eq!(unescape(br"6" /* 6 */), Ok(alloc::vec![0x36]));
    assert_eq!(unescape(br"7" /* 7 */), Ok(alloc::vec![0x37]));
    assert_eq!(unescape(br"8" /* 8 */), Ok(alloc::vec![0x38]));
    assert_eq!(unescape(br"9" /* 9 */), Ok(alloc::vec![0x39]));
    assert_eq!(unescape(br":" /* : */), Ok(alloc::vec![0x3a]));
    assert_eq!(unescape(br";" /* ; */), Ok(alloc::vec![0x3b]));
    assert_eq!(unescape(br"<" /* < */), Ok(alloc::vec![0x3c]));
    assert_eq!(unescape(br"=" /* = */), Ok(alloc::vec![0x3d]));
    assert_eq!(unescape(br">" /* > */), Ok(alloc::vec![0x3e]));
    assert_eq!(unescape(br"?" /* ? */), Ok(alloc::vec![0x3f]));
    assert_eq!(unescape(br"@" /* @ */), Ok(alloc::vec![0x40]));
    assert_eq!(unescape(br"A" /* A */), Ok(alloc::vec![0x41]));
    assert_eq!(unescape(br"B" /* B */), Ok(alloc::vec![0x42]));
    assert_eq!(unescape(br"C" /* C */), Ok(alloc::vec![0x43]));
    assert_eq!(unescape(br"D" /* D */), Ok(alloc::vec![0x44]));
    assert_eq!(unescape(br"E" /* E */), Ok(alloc::vec![0x45]));
    assert_eq!(unescape(br"F" /* F */), Ok(alloc::vec![0x46]));
    assert_eq!(unescape(br"G" /* G */), Ok(alloc::vec![0x47]));
    assert_eq!(unescape(br"H" /* H */), Ok(alloc::vec![0x48]));
    assert_eq!(unescape(br"I" /* I */), Ok(alloc::vec![0x49]));
    assert_eq!(unescape(br"J" /* J */), Ok(alloc::vec![0x4a]));
    assert_eq!(unescape(br"K" /* K */), Ok(alloc::vec![0x4b]));
    assert_eq!(unescape(br"L" /* L */), Ok(alloc::vec![0x4c]));
    assert_eq!(unescape(br"M" /* M */), Ok(alloc::vec![0x4d]));
    assert_eq!(unescape(br"N" /* N */), Ok(alloc::vec![0x4e]));
    assert_eq!(unescape(br"O" /* O */), Ok(alloc::vec![0x4f]));
    assert_eq!(unescape(br"P" /* P */), Ok(alloc::vec![0x50]));
    assert_eq!(unescape(br"Q" /* Q */), Ok(alloc::vec![0x51]));
    assert_eq!(unescape(br"R" /* R */), Ok(alloc::vec![0x52]));
    assert_eq!(unescape(br"S" /* S */), Ok(alloc::vec![0x53]));
    assert_eq!(unescape(br"T" /* T */), Ok(alloc::vec![0x54]));
    assert_eq!(unescape(br"U" /* U */), Ok(alloc::vec![0x55]));
    assert_eq!(unescape(br"V" /* V */), Ok(alloc::vec![0x56]));
    assert_eq!(unescape(br"W" /* W */), Ok(alloc::vec![0x57]));
    assert_eq!(unescape(br"X" /* X */), Ok(alloc::vec![0x58]));
    assert_eq!(unescape(br"Y" /* Y */), Ok(alloc::vec![0x59]));
    assert_eq!(unescape(br"Z" /* Z */), Ok(alloc::vec![0x5a]));
    assert_eq!(unescape(br"[" /* [ */), Ok(alloc::vec![0x5b]));
    assert_eq!(unescape(br"\\" /* \ */), Ok(alloc::vec![0x5c]));
    assert_eq!(unescape(br"]" /* ] */), Ok(alloc::vec![0x5d]));
    assert_eq!(unescape(br"^" /* ^ */), Ok(alloc::vec![0x5e]));
    assert_eq!(unescape(br"_" /* _ */), Ok(alloc::vec![0x5f]));
    assert_eq!(unescape(br"`" /* ` */), Ok(alloc::vec![0x60]));
    assert_eq!(unescape(br"a" /* a */), Ok(alloc::vec![0x61]));
    assert_eq!(unescape(br"b" /* b */), Ok(alloc::vec![0x62]));
    assert_eq!(unescape(br"c" /* c */), Ok(alloc::vec![0x63]));
    assert_eq!(unescape(br"d" /* d */), Ok(alloc::vec![0x64]));
    assert_eq!(unescape(br"e" /* e */), Ok(alloc::vec![0x65]));
    assert_eq!(unescape(br"f" /* f */), Ok(alloc::vec![0x66]));
    assert_eq!(unescape(br"g" /* g */), Ok(alloc::vec![0x67]));
    assert_eq!(unescape(br"h" /* h */), Ok(alloc::vec![0x68]));
    assert_eq!(unescape(br"i" /* i */), Ok(alloc::vec![0x69]));
    assert_eq!(unescape(br"j" /* j */), Ok(alloc::vec![0x6a]));
    assert_eq!(unescape(br"k" /* k */), Ok(alloc::vec![0x6b]));
    assert_eq!(unescape(br"l" /* l */), Ok(alloc::vec![0x6c]));
    assert_eq!(unescape(br"m" /* m */), Ok(alloc::vec![0x6d]));
    assert_eq!(unescape(br"n" /* n */), Ok(alloc::vec![0x6e]));
    assert_eq!(unescape(br"o" /* o */), Ok(alloc::vec![0x6f]));
    assert_eq!(unescape(br"p" /* p */), Ok(alloc::vec![0x70]));
    assert_eq!(unescape(br"q" /* q */), Ok(alloc::vec![0x71]));
    assert_eq!(unescape(br"r" /* r */), Ok(alloc::vec![0x72]));
    assert_eq!(unescape(br"s" /* s */), Ok(alloc::vec![0x73]));
    assert_eq!(unescape(br"t" /* t */), Ok(alloc::vec![0x74]));
    assert_eq!(unescape(br"u" /* u */), Ok(alloc::vec![0x75]));
    assert_eq!(unescape(br"v" /* v */), Ok(alloc::vec![0x76]));
    assert_eq!(unescape(br"w" /* w */), Ok(alloc::vec![0x77]));
    assert_eq!(unescape(br"x" /* x */), Ok(alloc::vec![0x78]));
    assert_eq!(unescape(br"y" /* y */), Ok(alloc::vec![0x79]));
    assert_eq!(unescape(br"z" /* z */), Ok(alloc::vec![0x7a]));
    assert_eq!(unescape(br"{" /* { */), Ok(alloc::vec![0x7b]));
    assert_eq!(unescape(br"|" /* | */), Ok(alloc::vec![0x7c]));
    assert_eq!(unescape(br"}" /* } */), Ok(alloc::vec![0x7d]));
    assert_eq!(unescape(br"~" /* ~ */), Ok(alloc::vec![0x7e]));
    assert_eq!(unescape(br"\x7f"), Ok(alloc::vec![0x7f]));
    assert_eq!(unescape(br"\x80"), Ok(alloc::vec![0x80]));
    assert_eq!(unescape(br"\x81"), Ok(alloc::vec![0x81]));
    assert_eq!(unescape(br"\x82"), Ok(alloc::vec![0x82]));
    assert_eq!(unescape(br"\x83"), Ok(alloc::vec![0x83]));
    assert_eq!(unescape(br"\x84"), Ok(alloc::vec![0x84]));
    assert_eq!(unescape(br"\x85"), Ok(alloc::vec![0x85]));
    assert_eq!(unescape(br"\x86"), Ok(alloc::vec![0x86]));
    assert_eq!(unescape(br"\x87"), Ok(alloc::vec![0x87]));
    assert_eq!(unescape(br"\x88"), Ok(alloc::vec![0x88]));
    assert_eq!(unescape(br"\x89"), Ok(alloc::vec![0x89]));
    assert_eq!(unescape(br"\x8a"), Ok(alloc::vec![0x8a]));
    assert_eq!(unescape(br"\x8b"), Ok(alloc::vec![0x8b]));
    assert_eq!(unescape(br"\x8c"), Ok(alloc::vec![0x8c]));
    assert_eq!(unescape(br"\x8d"), Ok(alloc::vec![0x8d]));
    assert_eq!(unescape(br"\x8e"), Ok(alloc::vec![0x8e]));
    assert_eq!(unescape(br"\x8f"), Ok(alloc::vec![0x8f]));
    assert_eq!(unescape(br"\x90"), Ok(alloc::vec![0x90]));
    assert_eq!(unescape(br"\x91"), Ok(alloc::vec![0x91]));
    assert_eq!(unescape(br"\x92"), Ok(alloc::vec![0x92]));
    assert_eq!(unescape(br"\x93"), Ok(alloc::vec![0x93]));
    assert_eq!(unescape(br"\x94"), Ok(alloc::vec![0x94]));
    assert_eq!(unescape(br"\x95"), Ok(alloc::vec![0x95]));
    assert_eq!(unescape(br"\x96"), Ok(alloc::vec![0x96]));
    assert_eq!(unescape(br"\x97"), Ok(alloc::vec![0x97]));
    assert_eq!(unescape(br"\x98"), Ok(alloc::vec![0x98]));
    assert_eq!(unescape(br"\x99"), Ok(alloc::vec![0x99]));
    assert_eq!(unescape(br"\x9a"), Ok(alloc::vec![0x9a]));
    assert_eq!(unescape(br"\x9b"), Ok(alloc::vec![0x9b]));
    assert_eq!(unescape(br"\x9c"), Ok(alloc::vec![0x9c]));
    assert_eq!(unescape(br"\x9d"), Ok(alloc::vec![0x9d]));
    assert_eq!(unescape(br"\x9e"), Ok(alloc::vec![0x9e]));
    assert_eq!(unescape(br"\x9f"), Ok(alloc::vec![0x9f]));
    assert_eq!(unescape(br"\xa0"), Ok(alloc::vec![0xa0]));
    assert_eq!(unescape(br"\xa1"), Ok(alloc::vec![0xa1]));
    assert_eq!(unescape(br"\xa2"), Ok(alloc::vec![0xa2]));
    assert_eq!(unescape(br"\xa3"), Ok(alloc::vec![0xa3]));
    assert_eq!(unescape(br"\xa4"), Ok(alloc::vec![0xa4]));
    assert_eq!(unescape(br"\xa5"), Ok(alloc::vec![0xa5]));
    assert_eq!(unescape(br"\xa6"), Ok(alloc::vec![0xa6]));
    assert_eq!(unescape(br"\xa7"), Ok(alloc::vec![0xa7]));
    assert_eq!(unescape(br"\xa8"), Ok(alloc::vec![0xa8]));
    assert_eq!(unescape(br"\xa9"), Ok(alloc::vec![0xa9]));
    assert_eq!(unescape(br"\xaa"), Ok(alloc::vec![0xaa]));
    assert_eq!(unescape(br"\xab"), Ok(alloc::vec![0xab]));
    assert_eq!(unescape(br"\xac"), Ok(alloc::vec![0xac]));
    assert_eq!(unescape(br"\xad"), Ok(alloc::vec![0xad]));
    assert_eq!(unescape(br"\xae"), Ok(alloc::vec![0xae]));
    assert_eq!(unescape(br"\xaf"), Ok(alloc::vec![0xaf]));
    assert_eq!(unescape(br"\xb0"), Ok(alloc::vec![0xb0]));
    assert_eq!(unescape(br"\xb1"), Ok(alloc::vec![0xb1]));
    assert_eq!(unescape(br"\xb2"), Ok(alloc::vec![0xb2]));
    assert_eq!(unescape(br"\xb3"), Ok(alloc::vec![0xb3]));
    assert_eq!(unescape(br"\xb4"), Ok(alloc::vec![0xb4]));
    assert_eq!(unescape(br"\xb5"), Ok(alloc::vec![0xb5]));
    assert_eq!(unescape(br"\xb6"), Ok(alloc::vec![0xb6]));
    assert_eq!(unescape(br"\xb7"), Ok(alloc::vec![0xb7]));
    assert_eq!(unescape(br"\xb8"), Ok(alloc::vec![0xb8]));
    assert_eq!(unescape(br"\xb9"), Ok(alloc::vec![0xb9]));
    assert_eq!(unescape(br"\xba"), Ok(alloc::vec![0xba]));
    assert_eq!(unescape(br"\xbb"), Ok(alloc::vec![0xbb]));
    assert_eq!(unescape(br"\xbc"), Ok(alloc::vec![0xbc]));
    assert_eq!(unescape(br"\xbd"), Ok(alloc::vec![0xbd]));
    assert_eq!(unescape(br"\xbe"), Ok(alloc::vec![0xbe]));
    assert_eq!(unescape(br"\xbf"), Ok(alloc::vec![0xbf]));
    assert_eq!(unescape(br"\xc0"), Ok(alloc::vec![0xc0]));
    assert_eq!(unescape(br"\xc1"), Ok(alloc::vec![0xc1]));
    assert_eq!(unescape(br"\xc2"), Ok(alloc::vec![0xc2]));
    assert_eq!(unescape(br"\xc3"), Ok(alloc::vec![0xc3]));
    assert_eq!(unescape(br"\xc4"), Ok(alloc::vec![0xc4]));
    assert_eq!(unescape(br"\xc5"), Ok(alloc::vec![0xc5]));
    assert_eq!(unescape(br"\xc6"), Ok(alloc::vec![0xc6]));
    assert_eq!(unescape(br"\xc7"), Ok(alloc::vec![0xc7]));
    assert_eq!(unescape(br"\xc8"), Ok(alloc::vec![0xc8]));
    assert_eq!(unescape(br"\xc9"), Ok(alloc::vec![0xc9]));
    assert_eq!(unescape(br"\xca"), Ok(alloc::vec![0xca]));
    assert_eq!(unescape(br"\xcb"), Ok(alloc::vec![0xcb]));
    assert_eq!(unescape(br"\xcc"), Ok(alloc::vec![0xcc]));
    assert_eq!(unescape(br"\xcd"), Ok(alloc::vec![0xcd]));
    assert_eq!(unescape(br"\xce"), Ok(alloc::vec![0xce]));
    assert_eq!(unescape(br"\xcf"), Ok(alloc::vec![0xcf]));
    assert_eq!(unescape(br"\xd0"), Ok(alloc::vec![0xd0]));
    assert_eq!(unescape(br"\xd1"), Ok(alloc::vec![0xd1]));
    assert_eq!(unescape(br"\xd2"), Ok(alloc::vec![0xd2]));
    assert_eq!(unescape(br"\xd3"), Ok(alloc::vec![0xd3]));
    assert_eq!(unescape(br"\xd4"), Ok(alloc::vec![0xd4]));
    assert_eq!(unescape(br"\xd5"), Ok(alloc::vec![0xd5]));
    assert_eq!(unescape(br"\xd6"), Ok(alloc::vec![0xd6]));
    assert_eq!(unescape(br"\xd7"), Ok(alloc::vec![0xd7]));
    assert_eq!(unescape(br"\xd8"), Ok(alloc::vec![0xd8]));
    assert_eq!(unescape(br"\xd9"), Ok(alloc::vec![0xd9]));
    assert_eq!(unescape(br"\xda"), Ok(alloc::vec![0xda]));
    assert_eq!(unescape(br"\xdb"), Ok(alloc::vec![0xdb]));
    assert_eq!(unescape(br"\xdc"), Ok(alloc::vec![0xdc]));
    assert_eq!(unescape(br"\xdd"), Ok(alloc::vec![0xdd]));
    assert_eq!(unescape(br"\xde"), Ok(alloc::vec![0xde]));
    assert_eq!(unescape(br"\xdf"), Ok(alloc::vec![0xdf]));
    assert_eq!(unescape(br"\xe0"), Ok(alloc::vec![0xe0]));
    assert_eq!(unescape(br"\xe1"), Ok(alloc::vec![0xe1]));
    assert_eq!(unescape(br"\xe2"), Ok(alloc::vec![0xe2]));
    assert_eq!(unescape(br"\xe3"), Ok(alloc::vec![0xe3]));
    assert_eq!(unescape(br"\xe4"), Ok(alloc::vec![0xe4]));
    assert_eq!(unescape(br"\xe5"), Ok(alloc::vec![0xe5]));
    assert_eq!(unescape(br"\xe6"), Ok(alloc::vec![0xe6]));
    assert_eq!(unescape(br"\xe7"), Ok(alloc::vec![0xe7]));
    assert_eq!(unescape(br"\xe8"), Ok(alloc::vec![0xe8]));
    assert_eq!(unescape(br"\xe9"), Ok(alloc::vec![0xe9]));
    assert_eq!(unescape(br"\xea"), Ok(alloc::vec![0xea]));
    assert_eq!(unescape(br"\xeb"), Ok(alloc::vec![0xeb]));
    assert_eq!(unescape(br"\xec"), Ok(alloc::vec![0xec]));
    assert_eq!(unescape(br"\xed"), Ok(alloc::vec![0xed]));
    assert_eq!(unescape(br"\xee"), Ok(alloc::vec![0xee]));
    assert_eq!(unescape(br"\xef"), Ok(alloc::vec![0xef]));
    assert_eq!(unescape(br"\xf0"), Ok(alloc::vec![0xf0]));
    assert_eq!(unescape(br"\xf1"), Ok(alloc::vec![0xf1]));
    assert_eq!(unescape(br"\xf2"), Ok(alloc::vec![0xf2]));
    assert_eq!(unescape(br"\xf3"), Ok(alloc::vec![0xf3]));
    assert_eq!(unescape(br"\xf4"), Ok(alloc::vec![0xf4]));
    assert_eq!(unescape(br"\xf5"), Ok(alloc::vec![0xf5]));
    assert_eq!(unescape(br"\xf6"), Ok(alloc::vec![0xf6]));
    assert_eq!(unescape(br"\xf7"), Ok(alloc::vec![0xf7]));
    assert_eq!(unescape(br"\xf8"), Ok(alloc::vec![0xf8]));
    assert_eq!(unescape(br"\xf9"), Ok(alloc::vec![0xf9]));
    assert_eq!(unescape(br"\xfa"), Ok(alloc::vec![0xfa]));
    assert_eq!(unescape(br"\xfb"), Ok(alloc::vec![0xfb]));
    assert_eq!(unescape(br"\xfc"), Ok(alloc::vec![0xfc]));
    assert_eq!(unescape(br"\xfd"), Ok(alloc::vec![0xfd]));
    assert_eq!(unescape(br"\xfe"), Ok(alloc::vec![0xfe]));
    assert_eq!(unescape(br"\xff"), Ok(alloc::vec![0xff]));
}

#[cfg(feature = "alloc")]
#[test]
fn test_unescape() {
    assert_eq!(unescape(br""), Ok(b"".to_vec()));
    assert_eq!(unescape(br"hello world"), Ok(b"hello world".to_vec()));
    assert_eq!(unescape(br"hello\x1eworld"), Ok(b"hello\x1eworld".to_vec()));
    assert_eq!(unescape(br"hello\x28world"), Ok(b"hello(world".to_vec()));
}

#[cfg(feature = "alloc")]
#[test]
#[rustfmt::skip]
fn test_unescape_error() {
    assert_eq!(unescape(br"hello\world"), Err(UnescapeError::InvalidEscape));
    assert_eq!(unescape(br"hello\xworld"), Err(UnescapeError::InvalidHexHi));
    assert_eq!(unescape(br"hello\x1world"), Err(UnescapeError::InvalidHexLo));
    assert_eq!(unescape(b"hello\\x1\0world"), Err(UnescapeError::InvalidHexLo));
    assert_eq!(unescape(br"hello\"), Err(UnescapeError::InvalidEscape));
    assert_eq!(unescape(br"hello\x"), Err(UnescapeError::InvalidHexHi));
    assert_eq!(unescape(br"hello\x1"), Err(UnescapeError::InvalidHexLo));
}

#[test]
#[rustfmt::skip]
fn test_unescape_into() {
    let mut buf = [0u8; 128];
    assert_eq!(unescape_into(&mut buf, br""), Ok(0));
    assert_eq!(buf, [0u8; 128]);

    let mut buf = [0u8; 128];
    assert_eq!(unescape_into(&mut buf, br"hello world"), Ok(11));
    assert_eq!(buf.as_slice(), [b"hello world".as_slice(), &[0u8; 117]].concat());

    let mut buf = [0u8; 128];
    assert_eq!(unescape_into(&mut buf, br"hello\x1eworld"), Ok(11));
    assert_eq!(buf.as_slice(), [b"hello\x1eworld".as_slice(), &[0u8; 117]].concat());

    let mut buf = [0u8; 128];
    assert_eq!(unescape_into(&mut buf, br"hello\x28world"), Ok(11));
    assert_eq!(buf.as_slice(), [b"hello\x28world".as_slice(), &[0u8; 117]].concat());
}

#[test]
#[rustfmt::skip]
fn test_unescape_into_error() {
    let mut buf = [0u8; 10];
    assert_eq!(unescape_into(&mut buf, b"hello\x28world"), Err(UnescapeIntoError::OutOfBounds));
    let mut buf = [0u8; 128];
    assert_eq!(unescape_into(&mut buf, br"hello\world"), Err(UnescapeIntoError::Unescape(UnescapeError::InvalidEscape)));
    let mut buf = [0u8; 128];
    assert_eq!(unescape_into(&mut buf, br"hello\xworld"), Err(UnescapeIntoError::Unescape(UnescapeError::InvalidHexHi)));
    let mut buf = [0u8; 128];
    assert_eq!(unescape_into(&mut buf, br"hello\x1world"), Err(UnescapeIntoError::Unescape(UnescapeError::InvalidHexLo)));
    let mut buf = [0u8; 128];
    assert_eq!(unescape_into(&mut buf, b"hello\\x1\0world"), Err(UnescapeIntoError::Unescape(UnescapeError::InvalidHexLo)));
    let mut buf = [0u8; 128];
    assert_eq!(unescape_into(&mut buf, br"hello\"), Err(UnescapeIntoError::Unescape(UnescapeError::InvalidEscape)));
    let mut buf = [0u8; 128];
    assert_eq!(unescape_into(&mut buf, br"hello\x"), Err(UnescapeIntoError::Unescape(UnescapeError::InvalidHexHi)));
    let mut buf = [0u8; 128];
    assert_eq!(unescape_into(&mut buf, br"hello\x1"), Err(UnescapeIntoError::Unescape(UnescapeError::InvalidHexLo)));
}

#[test]
fn test_unescaped_len() {
    assert_eq!(unescaped_len(br"hello world"), Ok(11));
    assert_eq!(unescaped_len(br"hello\\world"), Ok(11));
    assert_eq!(unescaped_len(br"hello\x1eworld"), Ok(11));
}

#[test]
#[rustfmt::skip]
fn test_unescaped_len_error() {
    assert_eq!(unescaped_len(br"hello\world"), Err(UnescapeError::InvalidEscape));
    assert_eq!(unescaped_len(br"hello\xworld"), Err(UnescapeError::InvalidHexHi));
    assert_eq!(unescaped_len(br"hello\x1world"), Err(UnescapeError::InvalidHexLo));
    assert_eq!(unescaped_len(b"hello\\x1\0world"), Err(UnescapeError::InvalidHexLo));
    assert_eq!(unescaped_len(br"hello\"), Err(UnescapeError::InvalidEscape));
    assert_eq!(unescaped_len(br"hello\x"), Err(UnescapeError::InvalidHexHi));
    assert_eq!(unescaped_len(br"hello\x1"), Err(UnescapeError::InvalidHexLo));
}
