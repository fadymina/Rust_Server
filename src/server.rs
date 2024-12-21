use log::{debug, error, info, warn};
use prost::Message;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;
use crate::message::{AddResponse, ClientMessage, EchoMessage, ServerMessage};

/// Represents a connected client.
struct Client {
    stream: TcpStream,
}

impl Client {
    pub fn new(stream: TcpStream) -> Self {
        debug!("Initializing new client with stream: {:?}", stream);
        Client { stream }
    }

    pub fn handle(&mut self, is_running: &Arc<AtomicBool>) -> io::Result<()> {
        const HEADER_SIZE: usize = 4;
        const MAX_MESSAGE_SIZE: usize = 512;

        debug!("Starting to handle client messages.");
        self.stream.set_nonblocking(true)?;

        while is_running.load(Ordering::SeqCst) {
            let mut header_buffer = [0u8; HEADER_SIZE];
            match self.stream.read_exact(&mut header_buffer) {
                Ok(_) => {
                    let payload_size = u32::from_be_bytes(header_buffer) as usize;
                    info!("Received header indicating payload size: {} bytes", payload_size);

                    if payload_size > MAX_MESSAGE_SIZE {
                        warn!(
                            "Payload size exceeds maximum allowed limit of {} bytes: {} bytes",
                            MAX_MESSAGE_SIZE, payload_size
                        );
                        self.respond_with_error(2, "Payload too large")?;
                        self.drain_payload(payload_size)?;
                        continue;
                    }

                    let mut buffer = vec![0; payload_size];
                    match self.stream.read_exact(&mut buffer) {
                        Ok(_) => {
                            debug!("Successfully read payload of {} bytes.", payload_size);
                            match ClientMessage::decode(&buffer[..]) {
                                Ok(client_message) => {
                                    info!("Decoded client message successfully.");
                                    self.process_message(client_message)?;
                                }
                                Err(e) => {
                                    warn!(
                                        "Failed to decode client message. Error: {}. Payload: {:?}",
                                        e, buffer
                                    );
                                    self.respond_with_error(3, "Invalid message format")?;
                                }
                            }
                        }
                        Err(e) => {
                            error!(
                                "Error reading payload of {} bytes from client: {}",
                                payload_size, e
                            );
                            return Err(e);
                        }
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(100));
                }
                Err(e) => {
                    error!("Error reading header from client: {}", e);
                    return Err(e);
                }
            }
        }

        Ok(())
    }

    fn respond_with_error(&mut self, status: i32, error_message: &str) -> io::Result<()> {
        let response = ServerMessage {
            message: Some(crate::message::server_message::Message::EchoMessage(EchoMessage {
                content: error_message.to_string(),
            })),
            status,
        };
        let mut payload = Vec::new();
        response.encode(&mut payload).map_err(|e| {
            error!(
                "Failed to encode error response (status: {}, message: '{}'). Error: {}",
                status, error_message, e
            );
            e
        })?;
        self.stream.write_all(&payload).map_err(|e| {
            error!(
                "Failed to send error response (status: {}, message: '{}'). Error: {}",
                status, error_message, e
            );
            e
        })?;
        debug!("Sent error response: {}", error_message);
        Ok(())
    }

    fn drain_payload(&mut self, size: usize) -> io::Result<()> {
        let mut buffer = vec![0; size];
        match self.stream.read_exact(&mut buffer) {
            Ok(_) => {
                debug!("Drained oversized payload of {} bytes.", size);
            }
            Err(e) => {
                error!(
                    "Error while draining oversized payload of {} bytes. Error: {}",
                    size, e
                );
            }
        }
        Ok(())
    }

    fn process_message(&mut self, client_message: ClientMessage) -> io::Result<()> {
        match client_message.message {
            Some(crate::message::client_message::Message::EchoMessage(echo)) => {
                info!("Received EchoMessage: {}", echo.content);
                let response = ServerMessage {
                    message: Some(crate::message::server_message::Message::EchoMessage(
                        EchoMessage {
                            content: echo.content,
                        },
                    )),
                    status: 1,
                };
                let mut payload = Vec::new();
                response.encode(&mut payload)?;
                self.stream.write_all(&payload)?;
                debug!("Sent EchoMessage response.");
            }
            Some(crate::message::client_message::Message::AddRequest(add_request)) => {
                info!("Received AddRequest: {} + {}", add_request.a, add_request.b);
                let result = add_request.a + add_request.b;
                let response = ServerMessage {
                    message: Some(crate::message::server_message::Message::AddResponse(
                        AddResponse { result },
                    )),
                    status: 1,
                };
                let mut payload = Vec::new();
                response.encode(&mut payload)?;
                self.stream.write_all(&payload)?;
                debug!("Sent AddResponse with result: {}", result);
            }
            _ => {
                warn!("Received unsupported message type.");
                self.respond_with_error(3, "Unsupported message type")?;
            }
        }
        Ok(())
    }
}

/// Multithreaded server struct.
pub struct Server {
    listener: TcpListener,
    is_running: Arc<AtomicBool>,
}

impl Server {
    pub fn new(addr: &str) -> io::Result<Self> {
        let listener = TcpListener::bind(addr)?;
        listener.set_nonblocking(true)?;
        info!("Server bound to address: {}", addr);
        Ok(Server {
            listener,
            is_running: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn run(&self) -> io::Result<()> {
        self.is_running.store(true, Ordering::SeqCst);
        info!("Server started on {}", self.listener.local_addr()?);

        while self.is_running.load(Ordering::SeqCst) {
            match self.listener.accept() {
                Ok((stream, addr)) => {
                    info!("New client connected: {}", addr);
                    let is_running = Arc::clone(&self.is_running);
                    thread::spawn(move || {
                        let mut client = Client::new(stream);
                        if let Err(e) = client.handle(&is_running) {
                            error!("Error handling client {}: {}", addr, e);
                        }
                        info!("Client {} disconnected.", addr);
                    });
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(100));
                }
                Err(e) => {
                    error!("Error accepting connection: {}", e);
                }
            }
        }

        info!("Server shutting down.");
        Ok(())
    }

    pub fn stop(&self) {
        self.is_running.store(false, Ordering::SeqCst);
        info!("Shutdown signal sent.");
    }
}
