//! Native TCP edge for a single bounded status exchange; no login or Bedrock relay.

use std::time::Duration;

use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpStream,
    time::{Instant, timeout},
};

use crate::{
    ProtocolError, codec,
    status::{self, ServerStatus},
};

/// Status and the ping/pong round trip, excluding DNS, connection and JSON response time.
#[derive(Debug)]
pub struct StatusReport {
    pub status: ServerStatus,
    pub latency: Option<Duration>,
}

#[derive(Debug, thiserror::Error)]
pub enum QueryError {
    #[error("Java status query timed out")]
    Timeout,
    #[error("Java status I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
}

/// Queries a DNS host or unbracketed IP at an explicit port.
///
/// The deadline covers DNS, TCP connection, all writes and both replies. No SRV
/// lookup, legacy ping, authentication or version compatibility claim is made.
/// The original hostname goes into the handshake for virtual-host routing.
pub async fn query_status(
    host: &str,
    port: u16,
    protocol: i32,
    budget: Duration,
) -> Result<StatusReport, QueryError> {
    let handshake = status::handshake(host, port, protocol)?;
    timeout(budget, async {
        let mut stream = TcpStream::connect((host, port)).await?;
        stream.set_nodelay(true)?;
        exchange(&mut stream, &handshake).await
    })
    .await
    .map_err(|_| QueryError::Timeout)?
}

/// Runs the exchange on a fresh stream; the caller owns its total deadline.
async fn exchange<S>(stream: &mut S, handshake: &[u8]) -> Result<StatusReport, QueryError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    stream.write_all(handshake).await?;
    stream.write_all(&status::request()).await?;
    stream.flush().await?;
    let status = status::decode_response(&read_frame(stream).await?)?;
    // One ping per fresh TCP connection; the token is an echo value, not an ID or timestamp.
    let token = 0;
    let start = Instant::now();
    stream.write_all(&status::ping(token)).await?;
    stream.flush().await?;
    status::check_pong(&read_frame(stream).await?, token)?;
    Ok(StatusReport {
        status,
        latency: Some(start.elapsed()),
    })
}

/// Reads exactly one frame and bounds allocation before reading the body.
async fn read_frame<S: AsyncRead + Unpin>(stream: &mut S) -> Result<Vec<u8>, QueryError> {
    let mut length = 0usize;
    for index in 0..codec::FRAME_LENGTH_BYTES {
        let byte = stream.read_u8().await?;
        length |= usize::from(byte & 0x7f) << (index * 7);
        if byte & 0x80 == 0 {
            if length == 0 || length > codec::MAX_FRAME_BYTES {
                return Err(ProtocolError::InvalidFrameLength.into());
            }
            let mut body = vec![0; length];
            stream.read_exact(&mut body).await?;
            return Ok(body);
        }
    }
    Err(ProtocolError::InvalidFrameLength.into())
}

#[cfg(test)]
#[path = "transport_tests.rs"]
mod tests;

/// Launcher discovery preserves a valid status when pong is absent; malformed/mismatched
/// pong remains an error. The total deadline includes DNS/SRV and cancellation closes the socket.
pub async fn discover(
    address: &crate::address::Address,
    budget: Duration,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<StatusReport, QueryError> {
    let deadline = Instant::now() + budget;
    tokio::select! {biased;_=cancel.cancelled()=>Err(QueryError::Io(std::io::Error::new(std::io::ErrorKind::Interrupted,"Java discovery cancelled"))),result=async{
        let (mut stream,status)=tokio::time::timeout_at(deadline,async{
            let sockets=crate::address::resolve(address).await?;
            let mut stream=TcpStream::connect(sockets.as_slice()).await?;
            stream.write_all(&status::handshake(&address.host,address.port,crate::target::PROTOCOL)?).await?;
            stream.write_all(&status::request()).await?;
            let status=status::decode_response(&read_frame(&mut stream).await?)?;
            Ok::<_,QueryError>((stream,status))
        }).await.map_err(|_|QueryError::Timeout)??;
        let start=Instant::now();
        let pong=tokio::time::timeout_at(deadline,async{
            stream.write_all(&status::ping(0)).await?;
            status::check_pong(&read_frame(&mut stream).await?,0)?;
            Ok::<_,QueryError>(start.elapsed())
        }).await;
        let latency=match pong{
            Ok(Ok(value))=>Some(value),Err(_)=>None,
            Ok(Err(QueryError::Io(error))) if matches!(error.kind(),std::io::ErrorKind::UnexpectedEof|std::io::ErrorKind::ConnectionReset|std::io::ErrorKind::BrokenPipe)=>None,
            Ok(Err(error))=>return Err(error),
        };
        Ok(StatusReport{status,latency})
    }=>result}
}
