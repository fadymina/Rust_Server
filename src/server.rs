use crate::message::{
    ClientMessage, ServerMessage, EchoMessage, AddResponse,
};
use log::{error, info, warn};
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
    stream: TcpStream,
}

impl Client {
    /// Creates a new client handler
    pub fn new(stream: TcpStream) -> Self {
        Client { stream }
    }

    /// Handles incoming client messages
    pub fn handle(&mut self) -> io::Result<()> {
        let mut buffer = [0; 512];

        // Read data from the client
        let bytes_read = self.stream.read(&mut buffer)?;
        if bytes_read == 0 {
            info!("Client disconnected.");
            return Ok(());
        }

        // Decode the ClientMessage
        if let Ok(client_message) = ClientMessage::decode(&buffer[..bytes_read]) {
            match client_message.message {
                Some(crate::message::client_message::Message::EchoMessage(echo)) => {
                    info!("Received EchoMessage: {}", echo.content);

                    // Prepare EchoResponse
                    let response = ServerMessage {
                        message: Some(crate::message::server_message::Message::EchoMessage(
                            EchoMessage { content: echo.content },
                        )),
                    };

                    // Send response to client
                    let mut payload = Vec::new();
                    response.encode(&mut payload).unwrap();
                    self.stream.write_all(&payload)?;
                }
                Some(crate::message::client_message::Message::AddRequest(add_request)) => {
                    info!("Received AddRequest: {} + {}", add_request.a, add_request.b);

                    // Compute the sum and prepare AddResponse
                    let result = add_request.a + add_request.b;
                    let response = ServerMessage {
                        message: Some(crate::message::server_message::Message::AddResponse(
                            AddResponse { result },
                        )),
                    };

                    // Send response to client
                    let mut payload = Vec::new();
                    response.encode(&mut payload).unwrap();
                    self.stream.write_all(&payload)?;
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
        let listener = TcpListener::bind(addr)?;
        let is_running = Arc::new(AtomicBool::new(false));
        Ok(Server {
            listener,
            is_running,
        })
    }

    /// Runs the server to accept and handle clients
    pub fn run(&self) -> io::Result<()> {
        self.is_running.store(true, Ordering::SeqCst);
        info!("Server is running on {}", self.listener.local_addr()?);

        // Set the listener to non-blocking mode
        self.listener.set_nonblocking(true)?;

        while self.is_running.load(Ordering::SeqCst) {
            match self.listener.accept() {
                Ok((stream, addr)) => {
                    info!("New client connected: {}", addr);

                    // Spawn a new thread to handle the client
                    thread::spawn(move || {
                        let mut client = Client::new(stream);
                        loop {
                            if let Err(e) = client.handle() {
                                error!("Error handling client {}: {}", addr, e);
                                break; // Exit the loop on client disconnect or error
                            }
                        }
                        info!("Client {} disconnected.", addr);
                    });
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    // No incoming connection; sleep briefly to reduce CPU usage
                    thread::sleep(Duration::from_millis(100));
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
    pub fn stop(&self) {
        if self.is_running.load(Ordering::SeqCst) {
            self.is_running.store(false, Ordering::SeqCst);
            info!("Shutdown signal sent.");
        } else {
            warn!("Server was already stopped or not running.");
        }
    }
}
