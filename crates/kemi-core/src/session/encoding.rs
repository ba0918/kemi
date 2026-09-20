//! セッションの 2 つのファイルの符号化（R-SESSION）。
//!
//! `<id>.session` は版つきのヘッダと meta（JSON）だけ。
//!
//! ```text
//! "kemi-session\n" (13 バイト) | 版 (u8) | meta の長さ (u32 LE) | meta (JSON)
//! ```
//!
//! `<id>.payload` は写しをひとまとまりの gzip にしたもの。目印も版も付けない
//! （読み手は `<id>.session` の版を確かめてからでないと見に行かない）。展開すると:
//!
//! ```text
//! 写しのメタデータの長さ (u32 LE) | 写しのメタデータ (JSON) | 内容のバイナリレコード
//! ```

use std::collections::BTreeMap;

use crate::source::FileContent;

/// ファイル先頭の目印。読めないファイルを黙ってセッションにしない。
pub(crate) const MAGIC: &[u8] = b"kemi-session\n";
/// セッション形式の版。
pub(crate) const VERSION: u8 = 2;
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

/// 版と meta を `<id>.session` のバイト列にする。
pub(crate) fn encode_session(version: u8, meta: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEAD_LEN + meta.len());
    out.extend_from_slice(MAGIC);
    out.push(version);
    out.extend_from_slice(&(meta.len() as u32).to_le_bytes());
    out.extend_from_slice(meta);
    out
}

/// 先頭の固定長（目印・版・meta の長さ）。
pub(crate) const HEAD_LEN: usize = MAGIC.len() + 1 + 4;

/// 先頭の固定長から版だけを取り出す。meta を読まないので、読み手の無い版の
/// 大きなファイルを丸ごとメモリに載せずに版を確かめられる。
pub(crate) fn decode_version(head: &[u8]) -> Result<u8, DecodeError> {
    decode_head(head).map(|(version, _)| version)
}

/// 先頭の固定長から版と meta の長さを取り出す。
fn decode_head(head: &[u8]) -> Result<(u8, usize), DecodeError> {
    let rest = head.strip_prefix(MAGIC).ok_or(DecodeError::NotASession)?;
    let (&version, rest) = rest
        .split_first()
        .ok_or(DecodeError::Corrupt("no version"))?;
    if rest.len() < 4 {
        return Err(DecodeError::Corrupt("no metadata length"));
    }
    let length = u32::from_le_bytes(rest[..4].try_into().expect("4 bytes")) as usize;
    Ok((version, length))
}

/// `<id>.session` のバイト列を版と meta に分ける。長さを持つので、途中で切れた
/// 書き込みをセッションとして読まない。
pub(crate) fn decode_session(bytes: &[u8]) -> Result<(u8, &[u8]), DecodeError> {
    let (version, length) = decode_head(bytes)?;
    let rest = &bytes[HEAD_LEN..];
    if rest.len() < length {
        return Err(DecodeError::Corrupt("metadata is cut off"));
    }
    Ok((version, &rest[..length]))
}

/// `<id>.payload` の中身（展開後）の先頭に、写しのメタデータを書く。内容のレコードは
/// この後ろへ続けて書く。書き出しは 1 つのバッファで組み立て、写しを二重に持たない。
pub(crate) fn push_copy_meta(out: &mut Vec<u8>, meta: &[u8]) {
    out.reserve(4 + meta.len());
    out.extend_from_slice(&(meta.len() as u32).to_le_bytes());
    out.extend_from_slice(meta);
}

/// 写しのメタデータと内容のレコードを 1 つのバイト列にする。
#[cfg(test)]
pub(crate) fn encode_copy_body(meta: &[u8], contents: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    push_copy_meta(&mut out, meta);
    out.extend_from_slice(contents);
    out
}

/// `encode_copy_body` の逆。
pub(crate) fn decode_copy_body(bytes: &[u8]) -> Result<(&[u8], &[u8]), DecodeError> {
    if bytes.len() < 4 {
        return Err(DecodeError::Corrupt("no copy metadata length"));
    }
    let length = u32::from_le_bytes(bytes[..4].try_into().expect("4 bytes")) as usize;
    let rest = &bytes[4..];
    if rest.len() < length {
        return Err(DecodeError::Corrupt("copy metadata is cut off"));
    }
    Ok((&rest[..length], &rest[length..]))
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

/// 内容を 1 つのバイナリレコードにして `out` の末尾へ書く。長さ前置で、非 UTF-8 の
/// バイト列もそのまま往復する。
pub(crate) fn encode_contents_into(out: &mut Vec<u8>, contents: &BTreeMap<String, FileContent>) {
    out.extend_from_slice(&(contents.len() as u32).to_le_bytes());
    for (id, content) in contents {
        push_bytes(out, id.as_bytes());
        push_side(out, content.old.as_deref());
        push_side(out, content.new.as_deref());
    }
}

/// 内容のレコードだけを 1 つのバイト列にする。
#[cfg(test)]
pub(crate) fn encode_contents(contents: &BTreeMap<String, FileContent>) -> Vec<u8> {
    let mut out = Vec::new();
    encode_contents_into(&mut out, contents);
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
    fn session_file_roundtrips_the_version_and_metadata() {
        let encoded = encode_session(7, b"{\"a\":1}");

        let (version, meta) = decode_session(&encoded).unwrap();

        assert_eq!(version, 7);
        assert_eq!(meta, b"{\"a\":1}");
    }

    #[test]
    fn session_file_rejects_foreign_and_cut_off_bytes() {
        assert_eq!(decode_session(b"{}").unwrap_err(), DecodeError::NotASession);
        assert!(decode_session(b"kemi-session\n").is_err());
        assert!(decode_session(b"kemi-session\n\x01\x05").is_err());
        // 長さより短いところで切れた書き込みは、セッションとして読まない。
        let encoded = encode_session(VERSION, b"{\"a\":1}");
        assert!(decode_session(&encoded[..encoded.len() - 1]).is_err());
    }

    #[test]
    fn copy_body_roundtrips_metadata_and_contents() {
        let encoded = encode_copy_body(b"{\"units\":[]}", b"\x00\x01\xff");

        let (meta, contents) = decode_copy_body(&encoded).unwrap();

        assert_eq!(meta, b"{\"units\":[]}");
        assert_eq!(contents, b"\x00\x01\xff");
    }

    #[test]
    fn copy_body_rejects_a_cut_off_record() {
        let encoded = encode_copy_body(b"{\"units\":[]}", b"");

        assert!(decode_copy_body(&encoded[..encoded.len() - 1]).is_err());
        assert!(decode_copy_body(b"\x01\x02").is_err());
    }
}
