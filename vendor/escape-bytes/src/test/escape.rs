use crate::{escape_into, escaped_len, escaped_max_len, EscapeIntoError};

#[cfg(feature = "alloc")]
use crate::escape;

#[cfg(feature = "alloc")]
#[allow(clippy::too_many_lines)]
#[test]
fn test_escape_all() {
    assert_eq!(escape([0x00]), br"\0");
    assert_eq!(escape([0x01]), br"\x01");
    assert_eq!(escape([0x02]), br"\x02");
    assert_eq!(escape([0x03]), br"\x03");
    assert_eq!(escape([0x04]), br"\x04");
    assert_eq!(escape([0x05]), br"\x05");
    assert_eq!(escape([0x06]), br"\x06");
    assert_eq!(escape([0x07]), br"\x07");
    assert_eq!(escape([0x08]), br"\x08");
    assert_eq!(escape([0x09]), br"\t");
    assert_eq!(escape([0x0a]), br"\n");
    assert_eq!(escape([0x0b]), br"\x0b");
    assert_eq!(escape([0x0c]), br"\x0c");
    assert_eq!(escape([0x0d]), br"\r");
    assert_eq!(escape([0x0e]), br"\x0e");
    assert_eq!(escape([0x0f]), br"\x0f");
    assert_eq!(escape([0x10]), br"\x10");
    assert_eq!(escape([0x11]), br"\x11");
    assert_eq!(escape([0x12]), br"\x12");
    assert_eq!(escape([0x13]), br"\x13");
    assert_eq!(escape([0x14]), br"\x14");
    assert_eq!(escape([0x15]), br"\x15");
    assert_eq!(escape([0x16]), br"\x16");
    assert_eq!(escape([0x17]), br"\x17");
    assert_eq!(escape([0x18]), br"\x18");
    assert_eq!(escape([0x19]), br"\x19");
    assert_eq!(escape([0x1a]), br"\x1a");
    assert_eq!(escape([0x1b]), br"\x1b");
    assert_eq!(escape([0x1c]), br"\x1c");
    assert_eq!(escape([0x1d]), br"\x1d");
    assert_eq!(escape([0x1e]), br"\x1e");
    assert_eq!(escape([0x1f]), br"\x1f");
    assert_eq!(escape([0x20] /*   */), br" ");
    assert_eq!(escape([0x21] /* ! */), br"!");
    assert_eq!(escape([0x22] /* " */), br#"""#);
    assert_eq!(escape([0x23] /* # */), br"#");
    assert_eq!(escape([0x24] /* $ */), br"$");
    assert_eq!(escape([0x25] /* % */), br"%");
    assert_eq!(escape([0x26] /* & */), br"&");
    assert_eq!(escape([0x27] /* ' */), br"'");
    assert_eq!(escape([0x28] /* ( */), br"(");
    assert_eq!(escape([0x29] /* ) */), br")");
    assert_eq!(escape([0x2a] /* * */), br"*");
    assert_eq!(escape([0x2b] /* + */), br"+");
    assert_eq!(escape([0x2c] /* , */), br",");
    assert_eq!(escape([0x2d] /* - */), br"-");
    assert_eq!(escape([0x2e] /* . */), br".");
    assert_eq!(escape([0x2f] /* / */), br"/");
    assert_eq!(escape([0x30] /* 0 */), br"0");
    assert_eq!(escape([0x31] /* 1 */), br"1");
    assert_eq!(escape([0x32] /* 2 */), br"2");
    assert_eq!(escape([0x33] /* 3 */), br"3");
    assert_eq!(escape([0x34] /* 4 */), br"4");
    assert_eq!(escape([0x35] /* 5 */), br"5");
    assert_eq!(escape([0x36] /* 6 */), br"6");
    assert_eq!(escape([0x37] /* 7 */), br"7");
    assert_eq!(escape([0x38] /* 8 */), br"8");
    assert_eq!(escape([0x39] /* 9 */), br"9");
    assert_eq!(escape([0x3a] /* : */), br":");
    assert_eq!(escape([0x3b] /* ; */), br";");
    assert_eq!(escape([0x3c] /* < */), br"<");
    assert_eq!(escape([0x3d] /* = */), br"=");
    assert_eq!(escape([0x3e] /* > */), br">");
    assert_eq!(escape([0x3f] /* ? */), br"?");
    assert_eq!(escape([0x40] /* @ */), br"@");
    assert_eq!(escape([0x41] /* A */), br"A");
    assert_eq!(escape([0x42] /* B */), br"B");
    assert_eq!(escape([0x43] /* C */), br"C");
    assert_eq!(escape([0x44] /* D */), br"D");
    assert_eq!(escape([0x45] /* E */), br"E");
    assert_eq!(escape([0x46] /* F */), br"F");
    assert_eq!(escape([0x47] /* G */), br"G");
    assert_eq!(escape([0x48] /* H */), br"H");
    assert_eq!(escape([0x49] /* I */), br"I");
    assert_eq!(escape([0x4a] /* J */), br"J");
    assert_eq!(escape([0x4b] /* K */), br"K");
    assert_eq!(escape([0x4c] /* L */), br"L");
    assert_eq!(escape([0x4d] /* M */), br"M");
    assert_eq!(escape([0x4e] /* N */), br"N");
    assert_eq!(escape([0x4f] /* O */), br"O");
    assert_eq!(escape([0x50] /* P */), br"P");
    assert_eq!(escape([0x51] /* Q */), br"Q");
    assert_eq!(escape([0x52] /* R */), br"R");
    assert_eq!(escape([0x53] /* S */), br"S");
    assert_eq!(escape([0x54] /* T */), br"T");
    assert_eq!(escape([0x55] /* U */), br"U");
    assert_eq!(escape([0x56] /* V */), br"V");
    assert_eq!(escape([0x57] /* W */), br"W");
    assert_eq!(escape([0x58] /* X */), br"X");
    assert_eq!(escape([0x59] /* Y */), br"Y");
    assert_eq!(escape([0x5a] /* Z */), br"Z");
    assert_eq!(escape([0x5b] /* [ */), br"[");
    assert_eq!(escape([0x5c] /* \ */), br"\\");
    assert_eq!(escape([0x5d] /* ] */), br"]");
    assert_eq!(escape([0x5e] /* ^ */), br"^");
    assert_eq!(escape([0x5f] /* _ */), br"_");
    assert_eq!(escape([0x60] /* ` */), br"`");
    assert_eq!(escape([0x61] /* a */), br"a");
    assert_eq!(escape([0x62] /* b */), br"b");
    assert_eq!(escape([0x63] /* c */), br"c");
    assert_eq!(escape([0x64] /* d */), br"d");
    assert_eq!(escape([0x65] /* e */), br"e");
    assert_eq!(escape([0x66] /* f */), br"f");
    assert_eq!(escape([0x67] /* g */), br"g");
    assert_eq!(escape([0x68] /* h */), br"h");
    assert_eq!(escape([0x69] /* i */), br"i");
    assert_eq!(escape([0x6a] /* j */), br"j");
    assert_eq!(escape([0x6b] /* k */), br"k");
    assert_eq!(escape([0x6c] /* l */), br"l");
    assert_eq!(escape([0x6d] /* m */), br"m");
    assert_eq!(escape([0x6e] /* n */), br"n");
    assert_eq!(escape([0x6f] /* o */), br"o");
    assert_eq!(escape([0x70] /* p */), br"p");
    assert_eq!(escape([0x71] /* q */), br"q");
    assert_eq!(escape([0x72] /* r */), br"r");
    assert_eq!(escape([0x73] /* s */), br"s");
    assert_eq!(escape([0x74] /* t */), br"t");
    assert_eq!(escape([0x75] /* u */), br"u");
    assert_eq!(escape([0x76] /* v */), br"v");
    assert_eq!(escape([0x77] /* w */), br"w");
    assert_eq!(escape([0x78] /* x */), br"x");
    assert_eq!(escape([0x79] /* y */), br"y");
    assert_eq!(escape([0x7a] /* z */), br"z");
    assert_eq!(escape([0x7b] /* { */), br"{");
    assert_eq!(escape([0x7c] /* | */), br"|");
    assert_eq!(escape([0x7d] /* } */), br"}");
    assert_eq!(escape([0x7e] /* ~ */), br"~");
    assert_eq!(escape([0x7f]), br"\x7f");
    assert_eq!(escape([0x80]), br"\x80");
    assert_eq!(escape([0x81]), br"\x81");
    assert_eq!(escape([0x82]), br"\x82");
    assert_eq!(escape([0x83]), br"\x83");
    assert_eq!(escape([0x84]), br"\x84");
    assert_eq!(escape([0x85]), br"\x85");
    assert_eq!(escape([0x86]), br"\x86");
    assert_eq!(escape([0x87]), br"\x87");
    assert_eq!(escape([0x88]), br"\x88");
    assert_eq!(escape([0x89]), br"\x89");
    assert_eq!(escape([0x8a]), br"\x8a");
    assert_eq!(escape([0x8b]), br"\x8b");
    assert_eq!(escape([0x8c]), br"\x8c");
    assert_eq!(escape([0x8d]), br"\x8d");
    assert_eq!(escape([0x8e]), br"\x8e");
    assert_eq!(escape([0x8f]), br"\x8f");
    assert_eq!(escape([0x90]), br"\x90");
    assert_eq!(escape([0x91]), br"\x91");
    assert_eq!(escape([0x92]), br"\x92");
    assert_eq!(escape([0x93]), br"\x93");
    assert_eq!(escape([0x94]), br"\x94");
    assert_eq!(escape([0x95]), br"\x95");
    assert_eq!(escape([0x96]), br"\x96");
    assert_eq!(escape([0x97]), br"\x97");
    assert_eq!(escape([0x98]), br"\x98");
    assert_eq!(escape([0x99]), br"\x99");
    assert_eq!(escape([0x9a]), br"\x9a");
    assert_eq!(escape([0x9b]), br"\x9b");
    assert_eq!(escape([0x9c]), br"\x9c");
    assert_eq!(escape([0x9d]), br"\x9d");
    assert_eq!(escape([0x9e]), br"\x9e");
    assert_eq!(escape([0x9f]), br"\x9f");
    assert_eq!(escape([0xa0]), br"\xa0");
    assert_eq!(escape([0xa1]), br"\xa1");
    assert_eq!(escape([0xa2]), br"\xa2");
    assert_eq!(escape([0xa3]), br"\xa3");
    assert_eq!(escape([0xa4]), br"\xa4");
    assert_eq!(escape([0xa5]), br"\xa5");
    assert_eq!(escape([0xa6]), br"\xa6");
    assert_eq!(escape([0xa7]), br"\xa7");
    assert_eq!(escape([0xa8]), br"\xa8");
    assert_eq!(escape([0xa9]), br"\xa9");
    assert_eq!(escape([0xaa]), br"\xaa");
    assert_eq!(escape([0xab]), br"\xab");
    assert_eq!(escape([0xac]), br"\xac");
    assert_eq!(escape([0xad]), br"\xad");
    assert_eq!(escape([0xae]), br"\xae");
    assert_eq!(escape([0xaf]), br"\xaf");
    assert_eq!(escape([0xb0]), br"\xb0");
    assert_eq!(escape([0xb1]), br"\xb1");
    assert_eq!(escape([0xb2]), br"\xb2");
    assert_eq!(escape([0xb3]), br"\xb3");
    assert_eq!(escape([0xb4]), br"\xb4");
    assert_eq!(escape([0xb5]), br"\xb5");
    assert_eq!(escape([0xb6]), br"\xb6");
    assert_eq!(escape([0xb7]), br"\xb7");
    assert_eq!(escape([0xb8]), br"\xb8");
    assert_eq!(escape([0xb9]), br"\xb9");
    assert_eq!(escape([0xba]), br"\xba");
    assert_eq!(escape([0xbb]), br"\xbb");
    assert_eq!(escape([0xbc]), br"\xbc");
    assert_eq!(escape([0xbd]), br"\xbd");
    assert_eq!(escape([0xbe]), br"\xbe");
    assert_eq!(escape([0xbf]), br"\xbf");
    assert_eq!(escape([0xc0]), br"\xc0");
    assert_eq!(escape([0xc1]), br"\xc1");
    assert_eq!(escape([0xc2]), br"\xc2");
    assert_eq!(escape([0xc3]), br"\xc3");
    assert_eq!(escape([0xc4]), br"\xc4");
    assert_eq!(escape([0xc5]), br"\xc5");
    assert_eq!(escape([0xc6]), br"\xc6");
    assert_eq!(escape([0xc7]), br"\xc7");
    assert_eq!(escape([0xc8]), br"\xc8");
    assert_eq!(escape([0xc9]), br"\xc9");
    assert_eq!(escape([0xca]), br"\xca");
    assert_eq!(escape([0xcb]), br"\xcb");
    assert_eq!(escape([0xcc]), br"\xcc");
    assert_eq!(escape([0xcd]), br"\xcd");
    assert_eq!(escape([0xce]), br"\xce");
    assert_eq!(escape([0xcf]), br"\xcf");
    assert_eq!(escape([0xd0]), br"\xd0");
    assert_eq!(escape([0xd1]), br"\xd1");
    assert_eq!(escape([0xd2]), br"\xd2");
    assert_eq!(escape([0xd3]), br"\xd3");
    assert_eq!(escape([0xd4]), br"\xd4");
    assert_eq!(escape([0xd5]), br"\xd5");
    assert_eq!(escape([0xd6]), br"\xd6");
    assert_eq!(escape([0xd7]), br"\xd7");
    assert_eq!(escape([0xd8]), br"\xd8");
    assert_eq!(escape([0xd9]), br"\xd9");
    assert_eq!(escape([0xda]), br"\xda");
    assert_eq!(escape([0xdb]), br"\xdb");
    assert_eq!(escape([0xdc]), br"\xdc");
    assert_eq!(escape([0xdd]), br"\xdd");
    assert_eq!(escape([0xde]), br"\xde");
    assert_eq!(escape([0xdf]), br"\xdf");
    assert_eq!(escape([0xe0]), br"\xe0");
    assert_eq!(escape([0xe1]), br"\xe1");
    assert_eq!(escape([0xe2]), br"\xe2");
    assert_eq!(escape([0xe3]), br"\xe3");
    assert_eq!(escape([0xe4]), br"\xe4");
    assert_eq!(escape([0xe5]), br"\xe5");
    assert_eq!(escape([0xe6]), br"\xe6");
    assert_eq!(escape([0xe7]), br"\xe7");
    assert_eq!(escape([0xe8]), br"\xe8");
    assert_eq!(escape([0xe9]), br"\xe9");
    assert_eq!(escape([0xea]), br"\xea");
    assert_eq!(escape([0xeb]), br"\xeb");
    assert_eq!(escape([0xec]), br"\xec");
    assert_eq!(escape([0xed]), br"\xed");
    assert_eq!(escape([0xee]), br"\xee");
    assert_eq!(escape([0xef]), br"\xef");
    assert_eq!(escape([0xf0]), br"\xf0");
    assert_eq!(escape([0xf1]), br"\xf1");
    assert_eq!(escape([0xf2]), br"\xf2");
    assert_eq!(escape([0xf3]), br"\xf3");
    assert_eq!(escape([0xf4]), br"\xf4");
    assert_eq!(escape([0xf5]), br"\xf5");
    assert_eq!(escape([0xf6]), br"\xf6");
    assert_eq!(escape([0xf7]), br"\xf7");
    assert_eq!(escape([0xf8]), br"\xf8");
    assert_eq!(escape([0xf9]), br"\xf9");
    assert_eq!(escape([0xfa]), br"\xfa");
    assert_eq!(escape([0xfb]), br"\xfb");
    assert_eq!(escape([0xfc]), br"\xfc");
    assert_eq!(escape([0xfd]), br"\xfd");
    assert_eq!(escape([0xfe]), br"\xfe");
    assert_eq!(escape([0xff]), br"\xff");
}

#[cfg(feature = "alloc")]
#[test]
fn test_escape() {
    assert_eq!(escape(b""), br"");
    assert_eq!(escape(b"hello world"), br"hello world");
    assert_eq!(escape(b"hello\x1eworld"), br"hello\x1eworld");
}

#[test]
#[rustfmt::skip]
fn test_escape_into() {
    let mut buf = [0u8; 128];
    assert_eq!(escape_into(&mut buf, b""), Ok(0));
    assert_eq!(buf, [0u8; 128]);

    let mut buf = [0u8; 128];
    assert_eq!(escape_into(&mut buf, b"hello world"), Ok(11));
    assert_eq!(buf.as_slice(), [br"hello world".as_slice(), &[0u8; 117]].concat());

    let mut buf = [0u8; 128];
    assert_eq!(escape_into(&mut buf, b"hello\x1eworld"), Ok(14));
    assert_eq!(buf.as_slice(), [br"hello\x1eworld".as_slice(), &[0u8; 114]].concat());

    let mut buf = [0u8; 128];
    assert_eq!(escape_into(&mut buf, b"hello\x28world"), Ok(11));
    assert_eq!(buf.as_slice(), [br"hello(world".as_slice(), &[0u8; 117]].concat());
}

#[test]
#[rustfmt::skip]
fn test_escape_into_error() {
    let mut buf = [0u8; 10];
    assert_eq!(escape_into(&mut buf, b"hello\x28world"), Err(EscapeIntoError::OutOfBounds));
    assert_eq!(buf.as_slice(), br"hello(worl".as_slice());
}

#[test]
fn test_escaped_max_len() {
    assert_eq!(escaped_max_len(0), Some(0));
    assert_eq!(escaped_max_len(10), Some(40));
}

#[test]
fn test_escaped_len() {
    assert_eq!(escaped_len(b"hello world"), 11);
    assert_eq!(escaped_len(b"hello\\world"), 12);
    assert_eq!(escaped_len(b"hello\x1eworld"), 14);
}
