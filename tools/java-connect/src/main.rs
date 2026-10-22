//! Developer acceptance path through saved-server selection, session ownership and world admission.
use java_protocol::{auth, session::Options, target, transport};
use std::{io::Write, path::PathBuf, time::Duration};
use tokio_util::sync::CancellationToken;

fn output(value: serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
    let mut out = std::io::stdout().lock();
    serde_json::to_writer(&mut out, &value)?;
    writeln!(out)?;
    Ok(())
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|s| s.starts_with("account-")) {
        return account(&args).await;
    }
    let get = |name: &str| {
        args.iter()
            .position(|s| s == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let path=PathBuf::from(get("--servers").ok_or("usage: java-connect --servers FILE --index N --accounts DIR [--offline-fixture NAME] [--keepalives 2] [--reconnect 2] [--timeout 120]")?);
    let index: usize = get("--index").unwrap_or_else(|| "0".into()).parse()?;
    let loaded = launcher_host::servers::load_servers(&path);
    if let Some(error) = loaded.recovery_message {
        return Err(error.into());
    }
    let server = loaded
        .servers
        .get(index)
        .ok_or("saved server index out of range")?;
    if server.edition != launcher::menu::ServerEdition::Java {
        return Err("Selected entry is Bedrock; Java diagnostic will not reroute it".into());
    }
    let address = server.address.parse()?;
    let report =
        transport::discover(&address, Duration::from_secs(5), &CancellationToken::new()).await?;
    if !report
        .status
        .version
        .as_ref()
        .is_some_and(|v| v.protocol == target::PROTOCOL)
    {
        return Err(format!(
            "Unsupported Java version; this build requires {} (protocol {})",
            target::GAME_VERSION,
            target::PROTOCOL
        )
        .into());
    }
    let keepalives: u64 = get("--keepalives").unwrap_or_else(|| "2".into()).parse()?;
    let reconnects: u64 = get("--reconnect").unwrap_or_else(|| "2".into()).parse()?;
    let timeout: u64 = get("--timeout").unwrap_or_else(|| "120".into()).parse()?;
    if keepalives == 0 || reconnects == 0 || timeout == 0 {
        return Err("Acceptance requires positive keepalive, session and timeout counts".into());
    }
    let required_chunks: usize = get("--require-chunks")
        .unwrap_or_else(|| "1".into())
        .parse()?;
    let sample: Option<[i32; 3]> = get("--inspect-block")
        .map(|s| -> Result<[i32; 3], Box<dyn std::error::Error>> {
            s.split(',')
                .map(str::parse)
                .collect::<Result<Vec<i32>, _>>()?
                .try_into()
                .map_err(|_| "--inspect-block requires x,y,z".into())
        })
        .transpose()?;
    let registry = get("--block-report")
        .map(|path| -> Result<_, Box<dyn std::error::Error>> {
            let bytes = std::fs::read(path)?;
            if bytes.len() != target::BLOCK_REPORT_BYTES {
                return Err("Java block report size differs from target pin".into());
            }
            let registry =
                assets::registry::java::BlockRegistry::load(&bytes, target::BLOCK_REPORT_SHA256)?;
            if registry.len() != target::BLOCK_STATE_COUNT {
                return Err("Java block registry count differs from target pin".into());
            }
            Ok(registry)
        })
        .transpose()?;
    let mut state = client_world::java::WorldState::default();
    for generation in 1..=reconnects {
        state.begin(generation);
        let mut session = if let Some(name) = get("--offline-fixture") {
            client_session::java::Session::spawn(Options::fixture(
                server.address.parse()?,
                name,
                generation,
            ))
        } else {
            let address = server.address.clone();
            let root =
                PathBuf::from(get("--accounts").ok_or("Online mode requires --accounts DIR")?);
            client_session::java::Session::run(move |events, commands| {
                launcher_host::java::run(address, generation, root, events, commands)
            })
        };
        let deadline = tokio::time::sleep(Duration::from_secs(timeout));
        tokio::pin!(deadline);
        loop {
            tokio::select! {
                _=tokio::signal::ctrl_c()=>{session.shutdown().await;return Err("Acceptance cancelled".into());},
                _=&mut deadline=>{
                    output(serde_json::json!({"timeout_generation":generation,"keepalives":state.keepalives,"decoded_chunks":state.decoded_chunks,"resident_chunks":state.terrain.len(),"vitals":state.vitals,"death":state.death.is_some(),"deferred_types":state.deferred_types}))?;
                    session.shutdown().await;return Err("Acceptance timed out before live terrain/position/vitals/keepalive evidence".into());
                },
                event=session.next()=>match event{
                    Some(event)=>{
                        if (matches!(event.event,java_protocol::session::Event::Death{..}) || matches!(&event.event,java_protocol::session::Event::Vitals(v) if v.health <= 0.)) && args.iter().any(|s|s=="--respawn-on-death") {
                            session.command(java_protocol::session::Command::Respawn)?;
                        }
                        let log=match &event.event {
                            java_protocol::session::Event::DeferredPlay{..}|java_protocol::session::Event::Terrain{..}=>None,
                            _=>Some(serde_json::to_value(&event)?),
                        };
                        if state.admit(event) && let Some(log)=log{output(log)?;}
                        if let Some(error)=state.terrain_error { session.shutdown().await; return Err(error.into()); }
                        if state.bootstrap.is_some() && state.teleport.is_some() && state.keepalives>=keepalives && state.death.is_none() && state.vitals.as_ref().is_some_and(|v| v.health>0.) && state.terrain.len()>=required_chunks {
                            let block=sample.map(|position| { let id=state.terrain.block(position); let facts=id.and_then(|id|registry.as_ref()?.state(id.0)); serde_json::json!({"position":position,"state_id":id.map(|id|id.0),"name":facts.map(|f| &*f.name),"properties":facts.map(|f| &f.properties)}) });
                            output(serde_json::json!({"accepted_generation":generation,"keepalives":state.keepalives,"deferred_packets":state.deferred_packets,"deferred_types":state.deferred_types,"decoded_chunks":state.decoded_chunks,"resident_chunks":state.terrain.len(),"terrain_payload_bytes":state.terrain.retained_payload_bytes(),"block_updates":state.block_updates,"block_sample":block,"vitals":state.vitals,"dimension_epoch":state.terrain.dimension().map(|d|d.epoch),"rendered_world":false}))?;
                            session.shutdown().await;break;
                        }
                    },
                    None=>return Err(session.outcome().await.err().unwrap_or_else(||"Java session closed before acceptance".into()).into()),
                }
            }
        }
    }
    Ok(())
}
async fn account(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(args.get(1).ok_or(
        "account-login DIR CLIENT_ID | account-select DIR UUID | account-logout DIR UUID",
    )?);
    let value = args.get(2).ok_or("missing public client ID/account UUID")?;
    match args[0].as_str() {
        "account-login" => {
            let client = auth::Client::new(value.clone())?;
            let cancel = CancellationToken::new();
            let result = tokio::select! {_=tokio::signal::ctrl_c()=>return Err("Java sign-in cancelled".into()), result=client.sign_in(&cancel,|prompt|{
                let _=output(serde_json::json!({"verification_uri":prompt.verification_uri,"user_code":prompt.user_code,"expires_in":prompt.expires_in}));
            })=>result?};
            launcher::java_accounts::save(&root, &result)?;
            std::fs::write(
                root.join("oauth-client.json"),
                serde_json::to_vec_pretty(&serde_json::json!({"client_id":value}))?,
            )?;
            output(serde_json::json!({"profile":result.profile,"expires_at":result.expires_at()}))?;
        }
        "account-select" => launcher::java_accounts::select(&root, uuid::Uuid::parse_str(value)?)?,
        "account-logout" => launcher::java_accounts::logout(&root, uuid::Uuid::parse_str(value)?)?,
        _ => return Err("unknown Java account command".into()),
    }
    Ok(())
}
