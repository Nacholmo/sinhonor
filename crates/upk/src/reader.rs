use crate::{Error, NameRef, Result};

/// Little-endian cursor over a byte slice.
pub(crate) struct Reader<'a> {
    d: &'a [u8],
    p: usize,
}

impl<'a> Reader<'a> {
    pub fn new(d: &'a [u8]) -> Self {
        Self { d, p: 0 }
    }
    pub fn pos(&self) -> usize {
        self.p
    }
    pub fn seek(&mut self, p: usize) {
        self.p = p;
    }
    pub fn remaining(&self) -> usize {
        self.d.len().saturating_sub(self.p)
    }
    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        let s = self.d.get(self.p..self.p + n).ok_or(Error::Truncated("bytes"))?;
        self.p += n;
        Ok(s)
    }
    pub fn skip(&mut self, n: usize) -> Result<()> {
        self.bytes(n).map(|_| ())
    }
    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }
    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.bytes(2)?.try_into().unwrap()))
    }
    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.bytes(4)?.try_into().unwrap()))
    }
    pub fn i32(&mut self) -> Result<i32> {
        Ok(self.u32()? as i32)
    }
    pub fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.bytes(8)?.try_into().unwrap()))
    }
    pub fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_bits(self.u32()?))
    }
    pub fn name_ref(&mut self) -> Result<NameRef> {
        Ok(NameRef { index: self.i32()?, number: self.i32()? })
    }
    /// FString: i32 length including the terminator; negative means UTF-16.
    pub fn fstring(&mut self) -> Result<String> {
        let len = self.i32()?;
        if len == 0 {
            return Ok(String::new());
        }
        if len < 0 {
            let n = (-len) as usize;
            if n > 1 << 20 {
                return Err(Error::Truncated("fstring"));
            }
            let raw = self.bytes(n * 2)?;
            let units: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            Ok(String::from_utf16_lossy(&units).trim_end_matches('\0').to_string())
        } else {
            let n = len as usize;
            if n > 1 << 20 {
                return Err(Error::Truncated("fstring"));
            }
            let raw = self.bytes(n)?;
            Ok(raw.iter().take_while(|&&b| b != 0).map(|&b| b as char).collect())
        }
    }
}
