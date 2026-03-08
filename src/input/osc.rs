//! OSC (Open Sound Control) server

use rosc::{OscPacket, OscMessage, OscType};
use std::net::{SocketAddrV4, UdpSocket, SocketAddr};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use crate::input::events::{InputEvent, InputSource, OscEvent};

/// OSC server for receiving control messages
pub struct OscServer {
    socket: Option<Arc<UdpSocket>>,
    event_sender: mpsc::Sender<(OscEvent, InputSource)>,
    listen_thread: Option<thread::JoinHandle<()>>,
    running: Arc<Mutex<bool>>,
    port: u16,
}

impl OscServer {
    /// Create new OSC server (doesn't start yet)
    pub fn new(event_sender: mpsc::Sender<(OscEvent, InputSource)>) -> Self {
        Self {
            socket: None,
            event_sender,
            listen_thread: None,
            running: Arc::new(Mutex::new(false)),
            port: 8000, // Default OSC port
        }
    }
    
    /// Start listening on specified port
    pub fn start(&mut self, port: u16) -> anyhow::Result<()> {
        self.stop();
        
        let addr = SocketAddrV4::new("0.0.0.0".parse()?, port);
        let socket = UdpSocket::bind(addr)?;
        socket.set_nonblocking(true)?;
        
        log::info!("OSC server listening on port {}", port);
        
        let socket = Arc::new(socket);
        self.socket = Some(socket.clone());
        self.port = port;
        
        let running = self.running.clone();
        let sender = self.event_sender.clone();
        
        *running.lock().unwrap() = true;
        let running_clone = running.clone();
        
        let handle = thread::spawn(move || {
            let mut buf = [0u8; 1536];
            
            while *running_clone.lock().unwrap() {
                match socket.recv_from(&mut buf) {
                    Ok((size, addr)) => {
                        match rosc::decoder::decode_udp(&buf[..size]) {
                            Ok((_, packet)) => {
                                let source = InputSource::Osc { 
                                    addr: SocketAddr::from(addr) 
                                };
                                process_packet(packet, &sender, source);
                            }
                            Err(e) => {
                                log::warn!("OSC decode error: {}", e);
                            }
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(std::time::Duration::from_millis(1));
                    }
                    Err(e) => {
                        log::error!("OSC socket error: {}", e);
                        break;
                    }
                }
            }
            
            log::info!("OSC server thread stopped");
        });
        
        self.listen_thread = Some(handle);
        
        Ok(())
    }
    
    /// Auto-start on default port
    pub fn auto_start(&mut self) -> anyhow::Result<()> {
        // Try default port, if taken try 8001, 8002, etc.
        for port in 8000..8010 {
            match self.start(port) {
                Ok(()) => return Ok(()),
                Err(e) => {
                    log::debug!("Port {} unavailable: {}", port, e);
                }
            }
        }
        Err(anyhow::anyhow!("Could not bind to any OSC port 8000-8009"))
    }
    
    /// Stop the server
    pub fn stop(&mut self) {
        *self.running.lock().unwrap() = false;
        
        if let Some(handle) = self.listen_thread.take() {
            let _ = handle.join();
        }
        
        self.socket = None;
        log::info!("OSC server stopped");
    }
    
    /// Check if running
    pub fn is_running(&self) -> bool {
        *self.running.lock().unwrap()
    }
    
    /// Get current port
    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for OscServer {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Process an OSC packet and send events
fn process_packet(
    packet: OscPacket, 
    sender: &mpsc::Sender<(OscEvent, InputSource)>,
    source: InputSource
) {
    match packet {
        OscPacket::Message(msg) => {
            if let Some(event) = parse_osc_message(&msg) {
                let _ = sender.send((event, source));
            }
        }
        OscPacket::Bundle(bundle) => {
            for packet in bundle.content {
                process_packet(packet, sender, source);
            }
        }
    }
}

/// Parse OSC message into event
fn parse_osc_message(msg: &OscMessage) -> Option<OscEvent> {
    let addr = msg.addr.as_str();
    let args = &msg.args;
    
    // Address patterns:
    // /rusty404/trigger 1       - trigger pad 1
    // /rusty404/release 1       - release pad 1
    // /rusty404/volume 1 0.8    - set pad 1 volume to 0.8
    // /rusty404/speed 1 1.5     - set pad 1 speed to 1.5
    // /rusty404/bpm 120         - set BPM to 120
    // /rusty404/stop            - stop all
    
    let parts: Vec<&str> = addr.split('/').filter(|s| !s.is_empty()).collect();
    
    match parts.as_slice() {
        ["rusty404", "trigger"] => {
            args.get(0).and_then(|a| a.clone().int()).map(|i| OscEvent::Trigger { pad: i as usize })
        }
        ["rusty404", "release"] => {
            args.get(0).and_then(|a| a.clone().int()).map(|i| OscEvent::Release { pad: i as usize })
        }
        ["rusty404", "volume"] => {
            if let (Some(pad), Some(vol)) = (
                args.get(0).and_then(|a| a.clone().int()),
                args.get(1).and_then(|a| match a {
                    OscType::Float(f) => Some(*f),
                    OscType::Int(i) => Some(*i as f32),
                    _ => None,
                })
            ) {
                Some(OscEvent::Volume { pad: pad as usize, value: vol.clamp(0.0, 1.0) })
            } else {
                None
            }
        }
        ["rusty404", "speed"] => {
            if let (Some(pad), Some(speed)) = (
                args.get(0).and_then(|a| a.clone().int()),
                args.get(1).and_then(|a| match a {
                    OscType::Float(f) => Some(*f),
                    OscType::Int(i) => Some(*i as f32),
                    _ => None,
                })
            ) {
                Some(OscEvent::Speed { pad: pad as usize, value: speed })
            } else {
                None
            }
        }
        ["rusty404", "bpm"] => {
            args.get(0).and_then(|a| match a {
                OscType::Float(f) => Some(OscEvent::Bpm(*f)),
                OscType::Int(i) => Some(OscEvent::Bpm(*i as f32)),
                _ => None,
            })
        }
        ["rusty404", "stop"] => {
            Some(OscEvent::Command("stop".to_string()))
        }
        _ => {
            log::debug!("Unknown OSC address: {}", addr);
            None
        }
    }
}

/// OSC mapping configuration
#[derive(Debug, Clone)]
pub struct OscMapping;

impl OscMapping {
    /// Convert OSC event to input event
    pub fn map_event(&self, osc: &OscEvent) -> Option<InputEvent> {
        match osc {
            OscEvent::Trigger { pad } => Some(InputEvent::PadTrigger { 
                pad: *pad, 
                velocity: 1.0 
            }),
            OscEvent::Release { pad } => Some(InputEvent::PadRelease { pad: *pad }),
            OscEvent::Volume { pad, value } => Some(InputEvent::PadVolume { 
                pad: *pad, 
                volume: *value 
            }),
            OscEvent::Speed { pad, value } => Some(InputEvent::PadSpeed { 
                pad: *pad, 
                speed: *value 
            }),
            OscEvent::Bpm(bpm) => Some(InputEvent::SetBpm(*bpm)),
            OscEvent::Command(cmd) => {
                match cmd.as_str() {
                    "stop" => Some(InputEvent::StopAll),
                    _ => None,
                }
            }
        }
    }
}

impl Default for OscMapping {
    fn default() -> Self {
        Self
    }
}
