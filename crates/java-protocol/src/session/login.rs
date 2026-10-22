//! Login dispatch and compression/encryption transition ordering.
use super::*;
impl Connection {
    pub(super) async fn login(&mut self, options: &Options) -> Result<(), Error> {
        let mut hello = vec![0];
        codec::write_varint(target::PROTOCOL, &mut hello);
        codec::write_string(&options.address.host, 255, &mut hello)?;
        hello.extend(options.address.port.to_be_bytes());
        hello.push(2);
        self.stream.write(&hello).await?;
        let mut start = Vec::new();
        codec::write_string(&self.profile.name, 16, &mut start)?;
        start.extend(self.profile.id.as_bytes());
        self.send("ServerboundHelloPacket", &start).await?;
        self.emit(Event::Phase(Phase::Login)).await?;
        let mut encrypted = false;
        let mut compression = false;
        loop {
            let packet = self.stream.read().await?;
            let mut r = Reader(&packet);
            let id = r.var()?;
            let name = usize::try_from(id)
                .ok()
                .and_then(|n| target::login::clientbound::NAMES.get(n))
                .ok_or(ProtocolError::Invalid("unknown login packet"))?;
            match *name {
                "ClientboundLoginDisconnectPacket" => {
                    let reason = r.string(32767)?;
                    r.finish()?;
                    return Err(Error::Disconnect(crate::status::plain_text(
                        &serde_json::from_str(&reason).map_err(ProtocolError::from)?,
                    )));
                }
                "ClientboundLoginCompressionPacket" => {
                    if compression {
                        return Err(
                            ProtocolError::Invalid("duplicate compression negotiation").into()
                        );
                    }
                    let threshold = r.var()?;
                    r.finish()?;
                    self.stream.compression(threshold);
                    compression = true;
                }
                "ClientboundHelloPacket" => {
                    if encrypted {
                        return Err(
                            ProtocolError::Invalid("duplicate encryption negotiation").into()
                        );
                    }
                    let id = r.string(20)?;
                    let key = r.bytes(8192)?;
                    let challenge = r.bytes(1024)?;
                    let authenticate = r.bool()?;
                    r.finish()?;
                    let (secret, response, hash) = crypto::response(&id, &key, &challenge)?;
                    if authenticate {
                        match &options.identity {
                            Identity::Online(a) => auth::join(a, &hash).await?,
                            Identity::OfflineFixture { .. } => {
                                return Err(Error::Unsupported(
                                    "online-mode authentication with an offline fixture identity",
                                ));
                            }
                        }
                    }
                    self.send("ServerboundKeyPacket", &response).await?;
                    self.stream.encryption(&secret);
                    encrypted = true;
                }
                "ClientboundCustomQueryPacket" => {
                    let id = r.var()?;
                    let _channel = r.string(32767)?;
                    let mut answer = Vec::new();
                    codec::write_varint(id, &mut answer);
                    answer.push(0);
                    self.send("ServerboundCustomQueryAnswerPacket", &answer)
                        .await?;
                }
                "ClientboundCookieRequestPacket" => {
                    let key = r.string(32767)?;
                    r.finish()?;
                    self.cookie(&key).await?;
                }
                "ClientboundLoginFinishedPacket" => {
                    let id = r.uuid()?;
                    let name = r.string(16)?;
                    let n = r.count(1024)?;
                    for _ in 0..n {
                        r.string(32767)?;
                        r.string(32767)?;
                        if r.bool()? {
                            r.string(32767)?;
                        }
                    }
                    self.server_session = r.uuid()?;
                    r.finish()?;
                    if matches!(options.identity, Identity::Online(_)) && id != self.profile.id {
                        return Err(
                            ProtocolError::Invalid("server changed authenticated profile").into(),
                        );
                    }
                    self.profile = auth::Profile { id, name };
                    self.send("ServerboundLoginAcknowledgedPacket", &[]).await?;
                    self.phase = Phase::Configuration;
                    self.emit(Event::Phase(self.phase)).await?;
                    return self.configure().await;
                }
                _ => return Err(Error::Unsupported("login packet")),
            }
        }
    }
}
