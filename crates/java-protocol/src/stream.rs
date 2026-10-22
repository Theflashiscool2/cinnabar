//! Ordered Java framing with continuous AES-CFB8 and negotiated zlib compression.
use crate::{ProtocolError, codec, wire::Reader};
use aes::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use flate2::{Compression, Decompress, FlushDecompress, Status, write::ZlibEncoder};
use std::io::Write;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const MAX_UNCOMPRESSED: usize = 8 * 1024 * 1024;
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error("Java transport I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid Java compression: {0}")]
    Compression(&'static str),
}
pub struct Stream<S> {
    io: S,
    threshold: Option<usize>,
    encrypt: Option<cfb8::Encryptor<aes::Aes128>>,
    decrypt: Option<cfb8::Decryptor<aes::Aes128>>,
    read_length: usize,
    read_prefix: usize,
    read_frame: Vec<u8>,
    read_offset: usize,
}
impl<S: AsyncRead + AsyncWrite + Unpin> Stream<S> {
    pub fn new(io: S) -> Self {
        Self {
            io,
            threshold: None,
            encrypt: None,
            decrypt: None,
            read_length: 0,
            read_prefix: 0,
            read_frame: Vec::new(),
            read_offset: 0,
        }
    }
    pub fn compression(&mut self, threshold: i32) {
        self.threshold = usize::try_from(threshold).ok();
    }
    pub fn encryption(&mut self, key: &[u8; 16]) {
        self.encrypt = Some(cfb8::Encryptor::new(key.into(), key.into()));
        self.decrypt = Some(cfb8::Decryptor::new(key.into(), key.into()));
    }
    /// Cancellation-safe reads retain framing and cipher progress in this stream. A cancelled
    /// write is still terminal: its caller must drop the stream rather than retrying bytes.
    pub async fn read(&mut self) -> Result<Vec<u8>, Error> {
        while self.read_frame.is_empty() {
            let mut byte = [0];
            if self.io.read(&mut byte).await? == 0 {
                return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof).into());
            }
            decrypt_bytes(&mut self.decrypt, &mut byte);
            self.read_length |= usize::from(byte[0] & 127) << (self.read_prefix * 7);
            self.read_prefix += 1;
            if byte[0] & 128 == 0 {
                if self.read_length == 0 || self.read_length > codec::MAX_FRAME_BYTES {
                    return Err(ProtocolError::InvalidFrameLength.into());
                }
                self.read_frame.resize(self.read_length, 0);
            } else if self.read_prefix == codec::FRAME_LENGTH_BYTES {
                return Err(ProtocolError::InvalidFrameLength.into());
            }
        }
        while self.read_offset < self.read_frame.len() {
            let n = self
                .io
                .read(&mut self.read_frame[self.read_offset..])
                .await?;
            if n == 0 {
                return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof).into());
            }
            decrypt_bytes(
                &mut self.decrypt,
                &mut self.read_frame[self.read_offset..self.read_offset + n],
            );
            self.read_offset += n;
        }
        self.read_length = 0;
        self.read_prefix = 0;
        self.read_offset = 0;
        decode(std::mem::take(&mut self.read_frame), self.threshold)
    }
    pub async fn write(&mut self, body: &[u8]) -> Result<(), Error> {
        let mut frame = encode(body, self.threshold)?;
        if let Some(cipher) = &mut self.encrypt {
            for byte in &mut frame {
                let mut block = [*byte].into();
                cipher.encrypt_block_mut(&mut block);
                *byte = block[0];
            }
        }
        self.io.write_all(&frame).await?;
        self.io.flush().await?;
        Ok(())
    }
}
fn decrypt_bytes(cipher: &mut Option<cfb8::Decryptor<aes::Aes128>>, bytes: &mut [u8]) {
    if let Some(cipher) = cipher {
        for byte in bytes {
            let mut block = [*byte].into();
            cipher.decrypt_block_mut(&mut block);
            *byte = block[0];
        }
    }
}
fn decode(frame: Vec<u8>, threshold: Option<usize>) -> Result<Vec<u8>, Error> {
    let Some(threshold) = threshold else {
        return Ok(frame);
    };
    let mut r = Reader(&frame);
    let size = usize::try_from(r.var()?).map_err(|_| Error::Compression("negative size"))?;
    if size == 0 {
        if r.0.is_empty() || r.0.len() >= threshold {
            return Err(Error::Compression("uncompressed packet at/above threshold"));
        }
        return Ok(r.0.to_vec());
    }
    if size < threshold || size > MAX_UNCOMPRESSED {
        return Err(Error::Compression("decompressed length/threshold"));
    }
    let mut output = vec![0; size];
    let mut z = Decompress::new(true);
    let status = z
        .decompress(r.0, &mut output, FlushDecompress::Finish)
        .map_err(|_| Error::Compression("invalid zlib"))?;
    if status != Status::StreamEnd
        || z.total_out() != size as u64
        || z.total_in() != r.0.len() as u64
    {
        return Err(Error::Compression(
            "truncated, trailing, or wrong-size zlib",
        ));
    }
    Ok(output)
}
fn encode(body: &[u8], threshold: Option<usize>) -> Result<Vec<u8>, Error> {
    if body.is_empty() || body.len() > MAX_UNCOMPRESSED {
        return Err(ProtocolError::InvalidFrameLength.into());
    }
    let Some(threshold) = threshold else {
        return Ok(codec::frame(body)?);
    };
    let mut frame = Vec::new();
    if body.len() >= threshold {
        codec::write_varint(body.len() as i32, &mut frame);
        let mut z = ZlibEncoder::new(Vec::new(), Compression::default());
        z.write_all(body)?;
        frame.extend(z.finish()?);
    } else {
        frame.push(0);
        frame.extend_from_slice(body);
    }
    Ok(codec::frame(&frame)?)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn input_during_partial_encrypted_read_keeps_both_cipher_positions() {
        use std::{future::Future, task::Poll};
        let (socket, mut peer) = tokio::io::duplex(32);
        let mut wire = Stream::new(socket);
        wire.encryption(&[0x13; 16]);
        peer.write_all(&[0xc8, 0xb4]).await.unwrap();
        {
            let mut read = std::pin::pin!(wire.read());
            std::future::poll_fn(|cx| {
                assert!(read.as_mut().poll(cx).is_pending());
                Poll::Ready(())
            })
            .await;
        }
        wire.write(&[9]).await.unwrap();
        peer.write_all(&[0x9d, 0x64, 0xeb, 0x30, 0x62])
            .await
            .unwrap();
        assert_eq!(wire.read().await.unwrap(), [0, 1, 2]);
        assert_eq!(wire.read().await.unwrap(), [5, 6]);
    }
    #[test]
    fn compression_threshold_and_corruption() {
        for threshold in [0, 1, 16, 17] {
            let body = vec![5; 16];
            let f = encode(&body, Some(threshold)).unwrap();
            let mut r = Reader(&f);
            let _ = r.var().unwrap();
            assert_eq!(decode(r.0.to_vec(), Some(threshold)).unwrap(), body);
        }
        assert!(decode(vec![1, 0], Some(2)).is_err());
        assert!(decode(vec![0, 1], Some(0)).is_err());
        assert!(decode(vec![1, 0xff], Some(0)).is_err());
    }
    #[tokio::test]
    async fn encrypted_frames_match_independent_openssl_vector() {
        // openssl enc -aes-128-cfb8, key=iv=13 repeated 16 times, plaintext 03000102020506.
        let (socket, mut peer) = tokio::io::duplex(16);
        let mut wire = Stream::new(socket);
        wire.encryption(&[0x13; 16]);
        wire.write(&[0, 1, 2]).await.unwrap();
        wire.write(&[5, 6]).await.unwrap();
        let mut bytes = [0; 7];
        peer.read_exact(&mut bytes).await.unwrap();
        assert_eq!(bytes, [0xc8, 0xb4, 0x9d, 0x64, 0xeb, 0x30, 0x62]);
    }
    #[tokio::test]
    async fn cipher_state_survives_fragmented_frames_and_negotiation() {
        let (a, b) = tokio::io::duplex(7);
        let mut a = Stream::new(a);
        let mut b = Stream::new(b);
        let writer = tokio::spawn(async move {
            a.write(&[3, 0]).await.unwrap();
            a.compression(0);
            a.encryption(&[19; 16]);
            a.write(&[1; 80]).await.unwrap();
            a.write(&[2; 90]).await.unwrap();
        });
        assert_eq!(b.read().await.unwrap(), [3, 0]);
        b.compression(0);
        b.encryption(&[19; 16]);
        assert_eq!(b.read().await.unwrap(), [1; 80]);
        assert_eq!(b.read().await.unwrap(), [2; 90]);
        writer.await.unwrap();
    }
}
