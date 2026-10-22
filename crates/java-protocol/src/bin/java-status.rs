//! Developer probe for the native Java status path.

#[cfg(not(target_arch = "wasm32"))]
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use java_protocol::{status, transport};
    use std::{io::Write, time::Duration};

    let mut args = std::env::args().skip(1);
    let host = args
        .next()
        .ok_or("usage: java-status HOST [PORT] [PROTOCOL]")?;
    let port = args
        .next()
        .map(|v| v.parse())
        .transpose()?
        .unwrap_or(status::DEFAULT_PORT);
    let protocol = args
        .next()
        .map(|v| v.parse())
        .transpose()?
        .unwrap_or(status::DISCOVERY_PROTOCOL);
    if args.next().is_some() {
        return Err("usage: java-status HOST [PORT] [PROTOCOL]".into());
    }
    let report = transport::query_status(&host, port, protocol, Duration::from_secs(5)).await?;
    let mut stdout = std::io::stdout().lock();
    serde_json::to_writer_pretty(
        &mut stdout,
        &serde_json::json!({
            "status": report.status,
            "latency_ms": report.latency.map(|d| d.as_secs_f64() * 1000.0),
        }),
    )?;
    writeln!(stdout)?;
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn main() {
    panic!("java-status requires native TCP support");
}
