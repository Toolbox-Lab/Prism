// TODO: Could encode and decode, and the functions upstream that call them, be
// const fn's?

use crate::{crc::checksum, error::DecodeError};

pub fn encode(ver: u8, payload: &[u8]) -> String {
    let mut d: Vec<u8> = Vec::with_capacity(1 + payload.len() + 2);
    d.push(ver);
    d.extend_from_slice(payload);
    d.extend_from_slice(&checksum(&d));
    data_encoding::BASE32_NOPAD.encode(&d)
}

pub fn decode(s: &str) -> Result<(u8, Vec<u8>), DecodeError> {
    let data = data_encoding::BASE32_NOPAD.decode(s.as_bytes()).ok();
    if let Some(data) = data {
        if data.len() < 3 {
            return Err(DecodeError::Invalid);
        }
        let ver = data[0];
        let (data_without_crc, crc_actual) = data.split_at(data.len() - 2);
        let crc_expect = checksum(data_without_crc);
        if crc_actual != crc_expect {
            return Err(DecodeError::Invalid);
        }
        let payload = &data_without_crc[1..];
        Ok((ver, payload.to_vec()))
    } else {
        Err(DecodeError::Invalid)
    }
}
