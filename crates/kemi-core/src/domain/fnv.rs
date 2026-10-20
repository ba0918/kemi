//! FNV-1a（64 bit）。値はディスクに残る（コメントの content_hash、結果ファイルの名前）
//! ので、定数と手順を変えない。

pub(crate) struct Fnv1a(u64);

impl Fnv1a {
    pub(crate) fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    pub(crate) fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    /// 16 桁の 16 進。
    pub(crate) fn finish_hex(&self) -> String {
        format!("{:016x}", self.0)
    }
}
