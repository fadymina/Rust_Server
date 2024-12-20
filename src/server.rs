use crate::message::{AddResponse, ClientMessage, EchoMessage, ServerMessage};
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

// Represents a connected client
struct Client {
    stream: TcpStream, // TCP stream for communication with the client
}

impl Client {
    /// Creates a new client handler
    pub fn new(stream: TcpStream) -> Self {
        debug!("Initializing new client with stream: {:?}", stream);
        Client { stream }
    }

    /// Handles incoming client messages
    pub fn handle(&mut self) -> io::Result<()> {
        const HEADER_SIZE: usize = 4; // Header size in bytes
        const MAX_MESSAGE_SIZE: usize = 512; // Maximum allowed message size

        debug!("Starting to handle client messages.");

        let mut header_buffer = [0u8; HEADER_SIZE];
        self.stream.read_exact(&mut header_buffer)?;

        let payload_size = u32::from_be_bytes(header_buffer) as usize;
        info!(
            "Received header indicating payload size: {} bytes",
            payload_size
        );

        if payload_size > MAX_MESSAGE_SIZE {
            error!(
                "Payload size exceeds maximum allowed size: {}",
                payload_size
            );

            let response = ServerMessage {
                message: None,
                status: 2,
            };
            let mut payload = Vec::new();
            response.encode(&mut payload).unwrap();
            self.stream.write_all(&payload)?;

            let mut oversized_buffer = vec![0u8; payload_size];
            let _ = self.stream.read_exact(&mut oversized_buffer);
            debug!(
                "Oversized payload (truncated to 512 bytes): {:?}",
                &oversized_buffer[..MAX_MESSAGE_SIZE]
            );

            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Payload size exceeds maximum allowed size",
            ));
        }

        let mut buffer = vec![0; payload_size];
        self.stream.read_exact(&mut buffer)?;
        debug!("Read {} bytes of payload from the stream.", payload_size);

        if let Ok(client_message) = ClientMessage::decode(&buffer[..]) {
            debug!("Successfully decoded client message.");
            match client_message.message {
                Some(crate::message::client_message::Message::EchoMessage(echo)) => {
                    debug!("Processing EchoMessage: {:?}", echo);
                    if echo.content.trim().is_empty() {
                        warn!("Received empty EchoMessage content.");
                        let response = ServerMessage {
                            message: Some(crate::message::server_message::Message::EchoMessage(
                                EchoMessage {
                                    content: "Received empty EchoMessage content.".to_string(),
                                },
                            )),
                            status: 2,
                        };
                        let mut payload = Vec::new();
                        response.encode(&mut payload).unwrap();
                        self.stream.write_all(&payload)?;
                        debug!("Sent response for empty EchoMessage.");
                    } else {
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
                        response.encode(&mut payload).unwrap();
                        self.stream.write_all(&payload)?;
                        debug!("Sent response for valid EchoMessage.");
                    }
                }
                Some(crate::message::client_message::Message::AddRequest(add_request)) => {
                    debug!("Processing AddRequest: {:?}", add_request);
                    // if add_request.a < 0 || add_request.b < 0 {
                    //     warn!(
                    //         "Received AddRequest with negative numbers: {} + {}",
                    //         add_request.a, add_request.b
                    //     );
                    //     return Ok(());
                    // }

                    info!("Received AddRequest: {} + {}", add_request.a, add_request.b);
                    let result = add_request.a + add_request.b;
                    let response = ServerMessage {
                        message: Some(crate::message::server_message::Message::AddResponse(
                            AddResponse { result },
                        )),
                        status: 1,
                    };
                    let mut payload = Vec::new();
                    response.encode(&mut payload).unwrap();
                    self.stream.write_all(&payload)?;
                    debug!("Sent response for AddRequest with result: {}", result);
                }
                _ => {
                    error!("Received unsupported or unknown message type.");
                }
            }
        } else {
            error!("Failed to decode incoming ClientMessage.");
        }

        Ok(())
    }
}

// Server struct that listens for and handles clients
pub struct Server {
    listener: TcpListener,
    is_running: Arc<AtomicBool>,
}

impl Server {
    /// Creates a new server
    pub fn new(addr: &str) -> io::Result<Self> {
        debug!("Attempting to bind server to address: {}", addr);
        let listener = TcpListener::bind(addr)?;
        let is_running = Arc::new(AtomicBool::new(false));
        info!("Server bound to address: {}", addr);
        Ok(Server {
            listener,
            is_running,
        })
    }

    /// Runs the server to accept and handle client connections
    pub fn run(&self) -> io::Result<()> {
        self.is_running.store(true, Ordering::SeqCst);
        info!("Server is running on {}", self.listener.local_addr()?);

        self.listener.set_nonblocking(true)?;
        debug!("Set listener to non-blocking mode.");

        while self.is_running.load(Ordering::SeqCst) {
            match self.listener.accept() {
                Ok((stream, addr)) => {
                    info!("New client connected: {}", addr);
                    thread::spawn(move || {
                        let mut client = Client::new(stream);
                        loop {
                            debug!("Handling messages for client: {}", addr);
                            if let Err(e) = client.handle() {
                                error!("Error handling client {}: {}", addr, e);
                                break;
                            }
                        }
                        info!("Client {} disconnected.", addr);
                    });
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(100));
                    debug!("No incoming connections, sleeping briefly.");
                }
                Err(e) => {
                    error!("Error accepting connection: {}", e);
                }
            }
        }

        info!("Server stopped.");
        Ok(())
    }

    /// Stops the server
    pub fn stop(&self) -> Result<(), String> {
        if self.is_running.load(Ordering::SeqCst) {
            self.is_running.store(false, Ordering::SeqCst);
            info!("Shutdown signal sent.");
            Ok(())
        } else {
            warn!("Server was already stopped or not running.");
            Err("Server was already stopped or not running.".to_string())
        }
    }
}
