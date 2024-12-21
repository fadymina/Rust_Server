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

/// Represents a connected client.
///
/// This struct encapsulates the TCP stream for a single client and
/// provides functionality to handle incoming client messages.
struct Client {
    /// TCP stream for communication with the client.
    stream: TcpStream,
}

impl Client {
    /// Creates a new client handler.
    ///
    /// # Arguments
    /// * `stream` - A `TcpStream` representing the connection to the client.
    ///
    /// # Returns
    /// A new instance of the `Client` struct.
    pub fn new(stream: TcpStream) -> Self {
        debug!("Initializing new client with stream: {:?}", stream);
        Client { stream }
    }

    /// Handles incoming client messages.
    ///
    /// Reads messages from the client, processes them based on the message type,
    /// and sends appropriate responses.
    ///
    /// # Errors
    /// This method returns an `io::Result` if there is an issue reading from or
    /// writing to the TCP stream.
    pub fn handle(&mut self) -> io::Result<()> {
        const HEADER_SIZE: usize = 4; // Fixed size for the header in bytes
        const MAX_MESSAGE_SIZE: usize = 512; // Maximum allowed message size in bytes

        debug!("Starting to handle client messages.");

        // Buffer to read the message header (indicates payload size)
        let mut header_buffer = [0u8; HEADER_SIZE];
        self.stream.read_exact(&mut header_buffer)?;

        // Extract payload size from the header
        let payload_size = u32::from_be_bytes(header_buffer) as usize;
        info!(
            "Received header indicating payload size: {} bytes",
            payload_size
        );

        // Check if the payload size exceeds the maximum limit
        if payload_size > MAX_MESSAGE_SIZE {
            error!(
                "Payload size exceeds maximum allowed size: {} bytes",
                payload_size
            );

            // Send an error response to the client
            let response = ServerMessage {
                message: None,
                status: 2,
            };
            let mut payload = Vec::new();
            response.encode(&mut payload).unwrap();
            self.stream.write_all(&payload)?;

            // Read and discard the oversized payload
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

        // Read the payload data into a buffer
        let mut buffer = vec![0; payload_size];
        self.stream.read_exact(&mut buffer)?;
        debug!("Read {} bytes of payload from the stream.", payload_size);

        // Attempt to decode the payload into a ClientMessage
        if let Ok(client_message) = ClientMessage::decode(&buffer[..]) {
            debug!("Successfully decoded client message.");

            // Handle different types of client messages
            match client_message.message {
                // EchoMessage: Respond with the same content
                Some(crate::message::client_message::Message::EchoMessage(echo)) => {
                    debug!("Processing EchoMessage: {:?}", echo);

                    if echo.content.trim().is_empty() {
                        warn!("Received empty EchoMessage content.");

                        // Send a response indicating an empty message
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

                        // Echo back the content to the client
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

                // AddRequest: Perform addition and return the result
                Some(crate::message::client_message::Message::AddRequest(add_request)) => {
                    debug!("Processing AddRequest: {:?}", add_request);

                    info!("Received AddRequest: {} + {}", add_request.a, add_request.b);
                    let result = add_request.a + add_request.b;

                    // Send the addition result back to the client
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

                // Unsupported or unknown message type
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

/// Represents a server that listens for client connections and processes messages.
///
/// The server accepts client connections on a specified address and
/// spawns a new thread for each client to handle their messages.
pub struct Server {
    /// TCP listener to accept client connections.
    listener: TcpListener,

    /// Atomic flag to indicate whether the server is running.
    is_running: Arc<AtomicBool>,
}

impl Server {
    /// Creates a new server instance bound to the specified address.
    ///
    /// # Arguments
    /// * `addr` - A string slice specifying the address to bind the server to (e.g., "127.0.0.1:8080").
    ///
    /// # Returns
    /// A `Result` containing the `Server` instance on success, or an `io::Error` on failure.
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

    /// Runs the server to accept and handle client connections.
    ///
    /// This method listens for incoming client connections in a loop and spawns
    /// a new thread to handle each connected client.
    ///
    /// # Errors
    /// Returns an `io::Result` if there is an issue with the listener.
    pub fn run(&self) -> io::Result<()> {
        self.is_running.store(true, Ordering::SeqCst);
        info!("Server is running on {}", self.listener.local_addr()?);

        self.listener.set_nonblocking(true)?;
        debug!("Set listener to non-blocking mode.");

        // Main server loop to accept and handle clients
        while self.is_running.load(Ordering::SeqCst) {
            match self.listener.accept() {
                Ok((stream, addr)) => {
                    info!("New client connected: {}", addr);

                    // Spawn a new thread for each client
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

                // No new connections; wait briefly
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(100));
                    debug!("No incoming connections, sleeping briefly.");
                }

                // Log unexpected errors during connection acceptance
                Err(e) => {
                    error!("Error accepting connection: {}", e);
                }
            }
        }

        info!("Server stopped.");
        Ok(())
    }

    /// Stops the server gracefully.
    ///
    /// Sets the `is_running` flag to `false`, signaling the main loop to terminate.
    ///
    /// # Returns
    /// * `Ok(())` if the server was running and has been stopped.
    /// * `Err(String)` if the server was already stopped.
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
