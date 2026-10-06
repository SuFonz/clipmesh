//! Framing: one length-delimited protobuf [`Envelope`] per frame.
//!
//! ```text
//! +----------------+---------------------------------+
//! | u32 big endian | serialized clipmesh.v1.Envelope |
//! | payload length | (payload length bytes)          |
//! +----------------+---------------------------------+
//! ```
//!
//! Why a hand written codec instead of `LengthDelimitedCodec`: the read path
//! must reject an oversized frame *before* allocating, and the write path must
//! reject it before it touches the socket. Both checks live in one place here,
//! which makes the memory ceiling easy to audit.

use bytes::{BufMut, Bytes, BytesMut};
use prost::Message as _;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::error::{ProtocolError, Result};
use crate::message::Envelope;

/// Width of the length prefix.
pub const LENGTH_PREFIX_BYTES: usize = 4;

/// Hard ceiling for a single frame.
///
/// Image chunks are 64 KiB, so this is ~256x the largest legitimate frame: it
/// only ever trips on a hostile or badly broken peer.
pub const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

/// Serialize an envelope into a length-prefixed frame.
///
/// # Errors
/// Fails when the envelope is larger than [`MAX_FRAME_BYTES`] or cannot be
/// encoded.
pub fn encode(envelope: &Envelope) -> Result<Bytes> {
    let payload_len = envelope.encoded_len();
    if payload_len > MAX_FRAME_BYTES {
        return Err(ProtocolError::FrameTooLarge {
            max: MAX_FRAME_BYTES,
            actual: payload_len,
        });
    }

    let mut buffer = BytesMut::with_capacity(LENGTH_PREFIX_BYTES + payload_len);
    // Safe cast: the check above bounds payload_len well below u32::MAX.
    buffer.put_u32(payload_len as u32);
    envelope.encode(&mut buffer)?;
    Ok(buffer.freeze())
}

/// Parse a frame payload (without its length prefix) back into an envelope.
///
/// # Errors
/// Fails when the frame is empty, oversized or not a valid envelope.
pub fn decode(payload: &[u8]) -> Result<Envelope> {
    if payload.is_empty() {
        return Err(ProtocolError::EmptyFrame);
    }
    if payload.len() > MAX_FRAME_BYTES {
        return Err(ProtocolError::FrameTooLarge {
            max: MAX_FRAME_BYTES,
            actual: payload.len(),
        });
    }
    Ok(Envelope::decode(payload)?)
}

/// Write one envelope, flushing so the peer sees it immediately.
///
/// # Errors
/// Propagates encoding and transport failures.
pub async fn write_envelope<W>(writer: &mut W, envelope: &Envelope) -> Result<()>
where
    W: AsyncWrite + Unpin,
{
    let frame = encode(envelope)?;
    writer.write_all(&frame).await?;
    writer.flush().await?;
    Ok(())
}

/// Read one raw frame payload.
///
/// Returns `Ok(None)` on a clean end of stream *between* frames, which is the
/// normal way a peer goes away. A stream that ends mid-frame is an error.
///
/// # Errors
/// Fails on I/O errors, oversized frames and zero length frames.
pub async fn read_frame<R>(reader: &mut R) -> Result<Option<Bytes>>
where
    R: AsyncRead + Unpin,
{
    let mut header = [0u8; LENGTH_PREFIX_BYTES];
    match reader.read_exact(&mut header).await {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error.into()),
    }

    let declared = u32::from_be_bytes(header) as usize;
    if declared == 0 {
        return Err(ProtocolError::EmptyFrame);
    }
    if declared > MAX_FRAME_BYTES {
        return Err(ProtocolError::FrameTooLarge {
            max: MAX_FRAME_BYTES,
            actual: declared,
        });
    }

    let mut payload = vec![0u8; declared];
    reader.read_exact(&mut payload).await?;
    Ok(Some(Bytes::from(payload)))
}

/// Read one envelope.
///
/// # Errors
/// Propagates framing failures and protobuf decode errors.
pub async fn read_envelope<R>(reader: &mut R) -> Result<Option<Envelope>>
where
    R: AsyncRead + Unpin,
{
    match read_frame(reader).await? {
        Some(payload) => Ok(Some(decode(&payload)?)),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard::{ImageMeta, TextPayload};
    use crate::device::DeviceId;
    use crate::message::{MessageKind, Payload};
    use crate::proto;

    #[test]
    fn envelope_round_trips_through_a_frame() {
        let payload = TextPayload::new_local("hello frame", DeviceId::new());
        let envelope = Envelope::text(payload.to_proto());
        let frame = encode(&envelope).unwrap();

        let declared = u32::from_be_bytes(frame[..4].try_into().unwrap()) as usize;
        assert_eq!(declared, frame.len() - LENGTH_PREFIX_BYTES);

        let restored = decode(&frame[LENGTH_PREFIX_BYTES..]).unwrap();
        assert_eq!(restored, envelope);
    }

    #[test]
    fn oversized_payloads_are_rejected_before_touching_the_socket() {
        let envelope = Envelope::text(proto::ClipboardText {
            id: "big".into(),
            source_device: DeviceId::new().to_string(),
            timestamp: 0,
            content: "x".repeat(MAX_FRAME_BYTES + 1),
        });
        assert!(matches!(
            encode(&envelope),
            Err(ProtocolError::FrameTooLarge { .. })
        ));
    }

    #[test]
    fn empty_frames_are_rejected() {
        assert!(matches!(decode(&[]), Err(ProtocolError::EmptyFrame)));
    }

    #[tokio::test]
    async fn frames_survive_a_real_stream() {
        let (mut client, mut server) = tokio::io::duplex(64 * 1024);

        let payload = ImageMeta::new_local(DeviceId::new(), &[3u8; 512], 16, 32);
        let outgoing = vec![
            Envelope::text(TextPayload::new_local("first", DeviceId::new()).to_proto()),
            Envelope::image(payload.to_proto()),
            Envelope::ping(),
        ];

        let writer = tokio::spawn({
            let outgoing = outgoing.clone();
            async move {
                for envelope in &outgoing {
                    write_envelope(&mut client, envelope).await.unwrap();
                }
            }
        });

        for expected in &outgoing {
            let received = read_envelope(&mut server).await.unwrap().unwrap();
            assert_eq!(&received, expected);
        }
        writer.await.unwrap();
    }

    #[tokio::test]
    async fn a_clean_close_reports_end_of_stream() {
        let (client, mut server) = tokio::io::duplex(1024);
        drop(client);
        assert!(read_envelope(&mut server).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn a_truncated_frame_is_an_error() {
        let (mut client, mut server) = tokio::io::duplex(1024);
        // Announce 64 bytes, then vanish after 4.
        client.write_all(&64u32.to_be_bytes()).await.unwrap();
        client.write_all(&[0u8; 4]).await.unwrap();
        drop(client);

        let error = read_envelope(&mut server).await.unwrap_err();
        assert!(matches!(error, ProtocolError::Io(_)));
    }

    #[tokio::test]
    async fn a_hostile_length_prefix_is_refused_without_allocating() {
        let (mut client, mut server) = tokio::io::duplex(1024);
        client
            .write_all(&((MAX_FRAME_BYTES + 1) as u32).to_be_bytes())
            .await
            .unwrap();

        let error = read_envelope(&mut server).await.unwrap_err();
        assert!(matches!(error, ProtocolError::FrameTooLarge { .. }));
    }

    #[test]
    fn every_variant_has_a_kind() {
        let variants: Vec<Envelope> = vec![
            Envelope::new(Payload::Ack(proto::Ack {
                id: "1".into(),
                ok: true,
                message: String::new(),
            })),
            Envelope::error(crate::message::ErrorCode::Internal, "boom"),
            Envelope::pong(1),
            Envelope::image_chunk(proto::ImageChunk {
                id: "1".into(),
                offset: 0,
                data: vec![1, 2, 3],
                last: true,
            }),
        ];
        let kinds: Vec<MessageKind> = variants.iter().filter_map(Envelope::kind).collect();
        assert_eq!(
            kinds,
            vec![
                MessageKind::Ack,
                MessageKind::Error,
                MessageKind::Pong,
                MessageKind::ImageChunk
            ]
        );
    }
}
