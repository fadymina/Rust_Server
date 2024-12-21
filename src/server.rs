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
use std::sync::atomic::AtomicUsize;

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
    /// This method reads messages from the client, processes them based on the
    /// message type, and sends appropriate responses. It performs validation on the
    /// payload size and ensures robust handling of different message types.
    ///
    /// # Errors
    /// Returns an `io::Result` if there is an issue reading from or writing to the TCP stream.
    pub fn handle(&mut self) -> io::Result<()> {
        const HEADER_SIZE: usize = 4; // Fixed size for the header in bytes
        const MAX_MESSAGE_SIZE: usize = 512; // Maximum allowed message size in bytes

        debug!("Starting to handle client messages.");
        self.stream.set_nonblocking(true)?;

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
                    let response = ServerMessage {
                        message: None,
                        status: 2,
                    };
                    let mut payload = Vec::new();
                    response.encode(&mut payload).unwrap();
                    self.stream.write_all(&payload)?;
                }
            }
        } else {
            error!("Failed to decode ClientMessage");
            // Send an error response to the client
            let response = ServerMessage {
                message: None,
                status: 2, // Error status for decoding failure
            };
            let mut payload = Vec::new();
            response.encode(&mut payload).unwrap();
            self.stream.write_all(&payload)?;

            return Err(io::Error::new(io::ErrorKind::InvalidData, "Decoding failed"));
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

/// Represents a server that listens for client connections and processes messages.
///
/// The server listens on a specified address and spawns a new thread for each
/// connected client to handle their messages concurrently.
pub struct Server {
    /// Listener for incoming client connections.
    listener: TcpListener,
    /// Flag to indicate whether the server is running.
    is_running: Arc<AtomicBool>,
    /// Maximum allowed concurrent connections.
    max_connections: usize,
    /// Current count of active connections.
    active_connections: Arc<AtomicUsize>,
}

impl Server {
    /// Creates a new server instance bound to the specified address.
    ///
    /// # Arguments
    /// * `addr` - A string slice specifying the address to bind the server to (e.g., "127.0.0.1:8080").
    /// * `max_connections` - The maximum number of concurrent connections allowed.
    ///
    /// # Returns
    /// A `Result` containing the `Server` instance on success, or an `io::Error` on failure.
    pub fn new(addr: &str, max_connections: usize) -> io::Result<Self> {
        debug!("Attempting to bind server to address: {}", addr);
        let listener = TcpListener::bind(addr)?;
        let is_running = Arc::new(AtomicBool::new(false));
        let active_connections = Arc::new(AtomicUsize::new(0));
        info!("Server bound to address: {}", addr);
        Ok(Server {
            listener,
            is_running,
            max_connections,
            active_connections,
        })
    }

    /// Runs the server to accept and handle client connections.
    ///
    /// This method listens for incoming client connections in a loop and spawns
    /// a new thread to handle each connected client.
    ///
    /// # Errors
    /// Returns an `io::Result` if there is an issue with the listener or client connections.
    pub fn run(&self) -> io::Result<()> {
        self.is_running.store(true, Ordering::SeqCst);
        info!("Server started and running.");

        let mut handles = Vec::new(); // Store thread handles

        self.listener.set_nonblocking(true)?;

        while self.is_running.load(Ordering::SeqCst) {
            match self.listener.accept() {
                Ok((stream, addr)) => {
                    let current_connections = self.active_connections.load(Ordering::SeqCst);
                    info!(
                        "New connection attempt from {}. Active connections: {}",
                        addr, current_connections
                    );

                    if self.active_connections.fetch_add(1, Ordering::SeqCst) >= self.max_connections {
                        warn!("Connection rejected: server at max capacity.");
                        self.active_connections.fetch_sub(1, Ordering::SeqCst);
                        stream.shutdown(std::net::Shutdown::Both)?;
                        continue;
                    }

                    let mut client_stream = stream.try_clone()?;
                    client_stream.write_all(b"CONNECTED\n")?;
                    info!("Sent handshake to {}", addr);

                    let active_connections = Arc::clone(&self.active_connections);
                    let is_running = Arc::clone(&self.is_running);

                    // Spawn a thread to handle the client
                    let handle = thread::spawn(move || {
                        let mut client = Client::new(stream);

                        loop {
                            if !is_running.load(Ordering::SeqCst) {
                                info!("Server shutting down. Sending shutdown message to client: {}", addr);
                                let _ = client
                                    .stream
                                    .write_all(b"SHUTDOWN\n"); // Notify client of shutdown
                                break;
                            }

                            match client.handle() {
                                Ok(_) => debug!("Successfully handled client message."),
                                Err(ref e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                                    info!("Client {} disconnected (EOF).", addr);
                                    break;
                                }
                                Err(ref e) if e.kind() == io::ErrorKind::ConnectionReset => {
                                    info!("Client {} disconnected (connection reset).", addr);
                                    break;
                                }
                                Err(e) => {
                                    error!("Unexpected error for client {}: {}", addr, e);
                                    break;
                                }
                            }
                        }

                        active_connections.fetch_sub(1, Ordering::SeqCst);
                        info!(
                            "Client {} disconnected. Active connections: {}",
                            addr,
                            active_connections.load(Ordering::SeqCst)
                        );
                    });

                    handles.push(handle); // Store thread handle
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(100));
                }
                Err(e) => {
                    error!("Error accepting connection: {}", e);
                }
            }
        }

        info!("Server is shutting down. Waiting for client threads to finish...");
        for handle in handles {
            let _ = handle.join(); // Wait for all threads to complete
        }
        info!("Server has shut down gracefully.");
        Ok(())
    }

    // pub fn run_with_timeout(&self, timeout: Duration) -> io::Result<()> {
    //     self.is_running.store(true, Ordering::SeqCst);
    //     info!("Server started with idle timeout: {:?}", timeout);

    //     let active_connections = Arc::clone(&self.active_connections);
    //     let is_running = Arc::clone(&self.is_running);
    //     let listener = self.listener.try_clone()?;

    //     // Monitor idle connections in a separate thread
    //     let timeout_thread = std::thread::spawn(move || {
    //         while is_running.load(Ordering::SeqCst) {
    //             std::thread::sleep(Duration::from_millis(100));
    //             active_connections.store(0, Ordering::SeqCst); // Reset counter
    //         }
    //     });

    //     let mut handles = Vec::new();
    //     listener.set_nonblocking(true)?;

    //     while self.is_running.load(Ordering::SeqCst) {
    //         match listener.accept() {
    //             Ok((stream, addr)) => {
    //                 let current_connections = self.active_connections.load(Ordering::SeqCst);
    //                 if current_connections >= self.max_connections {
    //                     warn!("Connection rejected: server at max capacity.");
    //                     stream.shutdown(std::net::Shutdown::Both)?;
    //                     continue;
    //                 }

    //                 self.active_connections.fetch_add(1, Ordering::SeqCst);
    //                 let is_running = Arc::clone(&self.is_running);
    //                 let active_connections = Arc::clone(&self.active_connections);

    //                 let handle = std::thread::spawn(move || {
    //                     let mut client = Client::new(stream);
    //                     loop {
    //                         if Instant::now().duration_since(client.last_active) > timeout {
    //                             info!("Client timed out: {}", addr);
    //                             break;
    //                         }

    //                         if let Err(e) = client.handle() {
    //                             error!("Error handling client {}: {}", addr, e);
    //                             break;
    //                         }
    //                     }

    //                     active_connections.fetch_sub(1, Ordering::SeqCst);
    //                     info!("Client {} disconnected.", addr);
    //                 });

    //                 handles.push(handle);
    //             }
    //             Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
    //                 std::thread::sleep(Duration::from_millis(100));
    //             }
    //             Err(e) => {
    //                 error!("Error accepting connection: {}", e);
    //             }
    //         }
    //     }

    //     for handle in handles {
    //         let _ = handle.join();
    //     }

    //     timeout_thread.join().unwrap();
    //     info!("Server shut down.");
    //     Ok(())
    // }

    /// Stops the server and gracefully shuts down all active client connections.
    ///
    /// # Returns
    /// A `Result` indicating success or failure. On success, it returns `Ok(())`.
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

    /// Returns the number of currently active client connections.
    ///
    /// # Returns
    /// A `usize` representing the number of active connections.
    pub fn get_active_connections(&self) -> usize {
        self.active_connections.load(Ordering::SeqCst)
    }
}
