use base64::{Engine as _, engine::general_purpose::STANDARD};
use crossbeam_channel::{Receiver, Sender, unbounded};
use eframe::egui;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{thread, time::Duration};
use tungstenite::{Error as WsError, Message, connect, stream::MaybeTlsStream};

const VOLUME_EVENTS: u64 = 1 << 16;

#[derive(Clone)]
pub struct Settings {
    pub host: String,
    pub port: u16,
    pub password: String,
}
pub enum Command {
    Connect(Settings),
    Disconnect,
}
pub enum Event {
    Connecting,
    Connected,
    Disconnected(String),
    Meters(Vec<InputLevels>),
}
pub struct InputLevels {
    pub name: String,
    pub channels: Vec<ChannelLevels>,
}
pub struct ChannelLevels {
    pub magnitude: f32,
    pub peak: f32,
}

pub fn start(ctx: egui::Context) -> (Sender<Command>, Receiver<Event>) {
    let (command_tx, command_rx) = unbounded();
    let (event_tx, event_rx) = unbounded();
    thread::spawn(move || {
        while let Ok(command) = command_rx.recv() {
            if let Command::Connect(settings) = command {
                let _ = event_tx.send(Event::Connecting);
                ctx.request_repaint();
                let reason = connection(&settings, &command_rx, &event_tx, &ctx)
                    .err()
                    .unwrap_or_default();
                let _ = event_tx.send(Event::Disconnected(reason));
                ctx.request_repaint();
            }
        }
    });
    (command_tx, event_rx)
}

fn connection(
    settings: &Settings,
    commands: &Receiver<Command>,
    tx: &Sender<Event>,
    ctx: &egui::Context,
) -> Result<(), String> {
    if settings.host.is_empty() {
        return Err("OBS host cannot be empty".into());
    }
    let url = format!("ws://{}:{}", settings.host, settings.port);
    let (mut socket, _) = connect(url.as_str()).map_err(|e| format!("Could not connect: {e}"))?;
    let hello = read_json(&mut socket)?;
    if hello["op"].as_u64() != Some(0) {
        return Err("OBS sent an invalid Hello message".into());
    }
    let data = &hello["d"];
    let mut identify = json!({"rpcVersion": 1, "eventSubscriptions": VOLUME_EVENTS});
    if let Some(auth) = data.get("authentication") {
        identify["authentication"] = Value::String(authentication(auth, &settings.password)?);
    }
    socket
        .send(Message::Text(
            json!({"op": 1, "d": identify}).to_string().into(),
        ))
        .map_err(|e| format!("Could not identify with OBS: {e}"))?;
    if read_json(&mut socket)?["op"].as_u64() != Some(2) {
        return Err("OBS rejected the connection or password".into());
    }
    tx.send(Event::Connected)
        .map_err(|_| "Application closed".to_owned())?;
    ctx.request_repaint();
    if let MaybeTlsStream::Plain(stream) = socket.get_mut() {
        stream
            .set_read_timeout(Some(Duration::from_millis(100)))
            .map_err(|e| format!("Could not configure OBS socket: {e}"))?;
    }
    loop {
        if matches!(commands.try_recv(), Ok(Command::Disconnect)) {
            let _ = socket.close(None);
            return Ok(());
        }
        match socket.read() {
            Err(WsError::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                continue;
            }
            Err(error) => return Err(format!("OBS disconnected: {error}")),
            Ok(message) => match message {
                Message::Text(text) => {
                    let value: Value = serde_json::from_str(&text)
                        .map_err(|e| format!("Invalid OBS message: {e}"))?;
                    if let Some(inputs) = parse_meters(&value) {
                        tx.send(Event::Meters(inputs))
                            .map_err(|_| "Application closed".to_owned())?;
                        ctx.request_repaint();
                    }
                }
                Message::Ping(data) => socket
                    .send(Message::Pong(data))
                    .map_err(|e| e.to_string())?,
                Message::Close(_) => return Ok(()),
                _ => {}
            },
        }
    }
}

fn read_json<S: std::io::Read + std::io::Write>(
    socket: &mut tungstenite::WebSocket<S>,
) -> Result<Value, String> {
    loop {
        match socket
            .read()
            .map_err(|e| format!("OBS handshake failed: {e}"))?
        {
            Message::Text(text) => return serde_json::from_str(&text).map_err(|e| e.to_string()),
            Message::Ping(data) => socket
                .send(Message::Pong(data))
                .map_err(|e| e.to_string())?,
            Message::Close(_) => return Err("OBS closed the connection".into()),
            _ => {}
        }
    }
}

fn authentication(auth: &Value, password: &str) -> Result<String, String> {
    let challenge = auth["challenge"]
        .as_str()
        .ok_or("Missing authentication challenge")?;
    let salt = auth["salt"].as_str().ok_or("Missing authentication salt")?;
    let secret = STANDARD.encode(Sha256::digest(format!("{password}{salt}").as_bytes()));
    Ok(STANDARD.encode(Sha256::digest(format!("{secret}{challenge}").as_bytes())))
}

fn parse_meters(value: &Value) -> Option<Vec<InputLevels>> {
    if value["op"].as_u64()? != 5 || value.pointer("/d/eventType")?.as_str()? != "InputVolumeMeters"
    {
        return None;
    }
    Some(
        value
            .pointer("/d/eventData/inputs")?
            .as_array()?
            .iter()
            .filter_map(|input| {
                let name = input["inputName"].as_str()?.to_owned();
                let channels = input["inputLevelsMul"]
                    .as_array()?
                    .iter()
                    .filter_map(|channel| {
                        let v = channel.as_array()?;
                        let magnitude = v.first()?.as_f64()? as f32;
                        Some(ChannelLevels {
                            magnitude,
                            peak: v.get(1).and_then(Value::as_f64).unwrap_or(magnitude as f64)
                                as f32,
                        })
                    })
                    .collect();
                Some(InputLevels { name, channels })
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_meter_event() {
        let v = json!({"op":5,"d":{"eventType":"InputVolumeMeters","eventData":{"inputs":[{"inputName":"Mic/Aux","inputLevelsMul":[[0.25,0.5],[0.1,0.2]]}]}}});
        let p = parse_meters(&v).unwrap();
        assert_eq!(p[0].name, "Mic/Aux");
        assert_eq!(p[0].channels.len(), 2);
        assert_eq!(p[0].channels[0].peak, 0.5);
    }
}
