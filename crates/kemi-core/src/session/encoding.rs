//! セッションファイルの符号化（R-SESSION）。外側は版つきのヘッダ、内容は 1 つの gzip。
//!
//! ```text
//! "kemi-session\n" (13 バイト) | 版 (u8) | meta の長さ (u32 LE) | meta (JSON) | payload (gzip)
//! ```
//!
//! payload は内容のバイナリレコードを gzip で包んだもの。meta は人が読める形に残す。

use std::collections::BTreeMap;

use crate::source::FileContent;

/// ファイル先頭の目印。読めないファイルを黙ってセッションにしない。
pub(crate) const MAGIC: &[u8] = b"kemi-session\n";
/// 保存形式の版。
pub(crate) const VERSION: u8 = 1;
/// 展開後の写しの上限。壊れたファイルが巨大なメモリを取らないように。
pub(crate) const DECOMPRESS_LIMIT: usize = 1024 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum DecodeError {
    /// セッションのファイルではない。
    NotASession,
    /// セッションのファイルだが、読めない。
    Corrupt(&'static str),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::NotASession => formatter.write_str("not a kemi session file"),
            DecodeError::Corrupt(reason) => write!(formatter, "broken session file: {reason}"),
        }
    }
}

/// 版・meta・payload を 1 つのファイルにする。payload は gzip 済みのバイト列。
pub(crate) fn encode_file(version: u8, meta: &[u8], payload: Option<&[u8]>) -> Vec<u8> {
    let mut out = Vec::with_capacity(MAGIC.len() + 5 + meta.len() + payload.map_or(0, <[u8]>::len));
    out.extend_from_slice(MAGIC);
    out.push(version);
    out.extend_from_slice(&(meta.len() as u32).to_le_bytes());
    out.extend_from_slice(meta);
    if let Some(payload) = payload {
        out.extend_from_slice(payload);
    }
    out
}

/// ファイルを版・meta・payload（gzip のまま）に分けたもの。
#[derive(Debug)]
pub(crate) struct Envelope<'a> {
    pub version: u8,
    pub meta: &'a [u8],
    pub payload: Option<&'a [u8]>,
}

/// ファイルを版・meta・payload（gzip のまま）に分ける。
pub(crate) fn decode_file(bytes: &[u8]) -> Result<Envelope<'_>, DecodeError> {
    let rest = bytes.strip_prefix(MAGIC).ok_or(DecodeError::NotASession)?;
    let (&version, rest) = rest
        .split_first()
        .ok_or(DecodeError::Corrupt("no version"))?;
    if rest.len() < 4 {
        return Err(DecodeError::Corrupt("no metadata length"));
    }
    let length = u32::from_le_bytes(rest[..4].try_into().expect("4 bytes")) as usize;
    let rest = &rest[4..];
    if rest.len() < length {
        return Err(DecodeError::Corrupt("metadata is cut off"));
    }
    let meta = &rest[..length];
    let payload = &rest[length..];
    Ok(Envelope {
        version,
        meta,
        payload: (!payload.is_empty()).then_some(payload),
    })
}

/// gzip のメンバー 1 つに包む。mtime は残さない。
pub(crate) fn gzip(data: &[u8]) -> Vec<u8> {
    let deflated = miniz_oxide::deflate::compress_to_vec(data, 6);
    let mut out = Vec::with_capacity(deflated.len() + 18);
    out.extend_from_slice(&[0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff]);
    out.extend_from_slice(&deflated);
    out.extend_from_slice(&crc32(data).to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out
}

/// gzip のメンバー 1 つを展開する。`limit` を超える展開は失敗にする。
pub(crate) fn gunzip(data: &[u8], limit: usize) -> Result<Vec<u8>, DecodeError> {
    let body = data
        .strip_prefix(&[0x1f, 0x8b, 0x08][..])
        .ok_or(DecodeError::Corrupt("not gzip"))?;
    // 書き出しは FLG=0 なので、残りの固定ヘッダは 7 バイト。
    let body = body
        .get(7..)
        .ok_or(DecodeError::Corrupt("gzip header is cut off"))?;
    // 末尾は CRC32 (4 バイト) と ISIZE (4 バイト)。
    let split = body
        .len()
        .checked_sub(8)
        .ok_or(DecodeError::Corrupt("gzip trailer is cut off"))?;
    let (deflated, trailer) = body.split_at(split);
    let out = miniz_oxide::inflate::decompress_to_vec_with_limit(deflated, limit)
        .map_err(|_| DecodeError::Corrupt("cannot inflate the contents"))?;
    let expected_crc = u32::from_le_bytes(trailer[..4].try_into().expect("4 bytes"));
    let expected_size = u32::from_le_bytes(trailer[4..8].try_into().expect("4 bytes"));
    if crc32(&out) != expected_crc || out.len() as u32 != expected_size {
        return Err(DecodeError::Corrupt("gzip checksum does not match"));
    }
    Ok(out)
}

/// 内容を 1 つのバイナリレコードにする。長さ前置で、非 UTF-8 のバイト列もそのまま往復する。
pub(crate) fn encode_contents(contents: &BTreeMap<String, FileContent>) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(contents.len() as u32).to_le_bytes());
    for (id, content) in contents {
        push_bytes(&mut out, id.as_bytes());
        push_side(&mut out, content.old.as_deref());
        push_side(&mut out, content.new.as_deref());
    }
    out
}

/// `encode_contents` の逆。
pub(crate) fn decode_contents(bytes: &[u8]) -> Result<BTreeMap<String, FileContent>, DecodeError> {
    let mut reader = Reader { bytes, at: 0 };
    let count = reader.u32()? as usize;
    let mut contents = BTreeMap::new();
    for _ in 0..count {
        let id = String::from_utf8(reader.bytes()?.to_vec())
            .map_err(|_| DecodeError::Corrupt("file id is not UTF-8"))?;
        let old = reader.side()?;
        let new = reader.side()?;
        contents.insert(id, FileContent { old, new });
    }
    Ok(contents)
}

fn push_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(bytes);
}

fn push_side(out: &mut Vec<u8>, side: Option<&[u8]>) {
    match side {
        None => out.push(0),
        Some(bytes) => {
            out.push(1);
            out.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
            out.extend_from_slice(bytes);
        }
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8], DecodeError> {
        let end = self
            .at
            .checked_add(count)
            .filter(|end| *end <= self.bytes.len())
            .ok_or(DecodeError::Corrupt("contents are cut off"))?;
        let slice = &self.bytes[self.at..end];
        self.at = end;
        Ok(slice)
    }

    fn u32(&mut self) -> Result<u32, DecodeError> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes(bytes.try_into().expect("4 bytes")))
    }

    fn u64(&mut self) -> Result<u64, DecodeError> {
        let bytes = self.take(8)?;
        Ok(u64::from_le_bytes(bytes.try_into().expect("8 bytes")))
    }

    fn bytes(&mut self) -> Result<&'a [u8], DecodeError> {
        let length = self.u32()? as usize;
        self.take(length)
    }

    fn side(&mut self) -> Result<Option<Vec<u8>>, DecodeError> {
        match self.take(1)?[0] {
            0 => Ok(None),
            1 => {
                let length = self.u64()? as usize;
                Ok(Some(self.take(length)?.to_vec()))
            }
            _ => Err(DecodeError::Corrupt("unknown side marker")),
        }
    }
}

const CRC_TABLE: [u32; 256] = build_crc_table();

const fn build_crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut index = 0;
    while index < 256 {
        let mut value = index as u32;
        let mut bit = 0;
        while bit < 8 {
            value = if value & 1 == 1 {
                0xedb8_8320 ^ (value >> 1)
            } else {
                value >> 1
            };
            bit += 1;
        }
        table[index] = value;
        index += 1;
    }
    table
}

pub(crate) fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in data {
        crc = CRC_TABLE[((crc ^ u32::from(*byte)) & 0xff) as usize] ^ (crc >> 8);
    }
    crc ^ 0xffff_ffff
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content(old: Option<&[u8]>, new: Option<&[u8]>) -> FileContent {
        FileContent {
            old: old.map(<[u8]>::to_vec),
            new: new.map(<[u8]>::to_vec),
        }
    }

    #[test]
    fn gzip_roundtrips_arbitrary_bytes() {
        let data: Vec<u8> = (0..=255u8).cycle().take(10_000).collect();

        let packed = gzip(&data);
        let unpacked = gunzip(&packed, DECOMPRESS_LIMIT).unwrap();

        assert_eq!(unpacked, data);
    }

    #[test]
    fn gzip_is_a_gzip_member() {
        let packed = gzip(b"hello");

        assert_eq!(&packed[..3], &[0x1f, 0x8b, 0x08]);
        assert_eq!(gunzip(&packed, DECOMPRESS_LIMIT).unwrap(), b"hello");
    }

    #[test]
    fn gzip_rejects_a_cut_off_member() {
        let packed = gzip(b"hello");

        assert!(gunzip(&packed[..packed.len() - 1], DECOMPRESS_LIMIT).is_err());
        assert!(gunzip(b"not gzip", DECOMPRESS_LIMIT).is_err());
    }

    #[test]
    fn gzip_stops_at_the_limit() {
        let packed = gzip(&vec![7u8; 10_000]);

        assert!(gunzip(&packed, 100).is_err());
    }

    #[test]
    fn contents_record_roundtrips_non_utf8_bytes() {
        let mut contents = BTreeMap::new();
        contents.insert(
            "f1".to_string(),
            content(Some(&[0xff, 0xfe, 0x00]), Some(b"text\n".as_slice())),
        );
        contents.insert("f2".to_string(), content(None, Some(b"added".as_slice())));
        contents.insert("f3".to_string(), content(Some(b"deleted".as_slice()), None));

        let encoded = encode_contents(&contents);
        let decoded = decode_contents(&encoded).unwrap();

        assert_eq!(decoded, contents);
    }

    #[test]
    fn contents_record_rejects_a_cut_off_record() {
        let mut contents = BTreeMap::new();
        contents.insert("f1".to_string(), content(None, Some(b"x".as_slice())));

        let encoded = encode_contents(&contents);

        assert!(decode_contents(&encoded[..encoded.len() - 1]).is_err());
    }

    #[test]
    fn file_envelope_roundtrips() {
        let encoded = encode_file(7, b"{\"a\":1}", Some(b"\x1f\x8bpayload"));

        let envelope = decode_file(&encoded).unwrap();

        assert_eq!(envelope.version, 7);
        assert_eq!(envelope.meta, b"{\"a\":1}");
        assert_eq!(envelope.payload, Some(b"\x1f\x8bpayload".as_slice()));
    }

    #[test]
    fn file_envelope_without_payload_roundtrips() {
        let encoded = encode_file(VERSION, b"{}", None);

        let envelope = decode_file(&encoded).unwrap();

        assert_eq!(envelope.meta, b"{}");
        assert_eq!(envelope.payload, None);
    }

    #[test]
    fn file_envelope_rejects_foreign_and_cut_off_bytes() {
        assert_eq!(decode_file(b"{}").unwrap_err(), DecodeError::NotASession);
        assert!(decode_file(b"kemi-session\n").is_err());
        assert!(decode_file(b"kemi-session\n\x01\x05").is_err());
    }
}
