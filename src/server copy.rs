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
    stream: TcpStream, // TCP stream for communication with the client
}

impl Client {
    /// Creates a new client handler
    /// 
    /// # Arguments
    /// * `stream` - The TCP stream representing the connection to the client.
    pub fn new(stream: TcpStream) -> Self {
        Client { stream } // Initialize a client with the provided stream
    }

    /// Handles incoming client messages
    /// 
    /// This function processes messages from the client and sends appropriate responses.
    pub fn handle(&mut self) -> io::Result<()> {
        // let mut size_of_buffer = self.stream.size_of_buffer()?;
        const HEADER_SIZE: usize = 4; // Header size in bytes
        const MAX_MESSAGE_SIZE: usize = 512;
    
        // Buffer to store the header
        let mut header_buffer = [0u8; HEADER_SIZE];
    
        // Read the header to determine the size of the payload
        self.stream.read_exact(&mut header_buffer)?;
    
        // Convert the header to a usize (assuming big-endian format)
        let payload_size = u32::from_be_bytes(header_buffer) as usize;
    
        // Log the received payload size
        println!("Received payload size: {}", payload_size);
    
        if payload_size > MAX_MESSAGE_SIZE {
            error!("Payload size exceeds maximum allowed size: {}", payload_size);

            let response = ServerMessage {
                message: None,
                status: 2,
            };
            let mut payload = Vec::new();
            response.encode(&mut payload).unwrap();
            self.stream.write_all(&payload)?;
    
            // Optionally: Read and log the oversized payload for debugging purposes
            let mut oversized_buffer = vec![0u8; payload_size];
            let _ = self.stream.read_exact(&mut oversized_buffer); // Ignore errors here
            println!("Oversized payload (truncated to 512 bytes): {:?}", &oversized_buffer[..MAX_MESSAGE_SIZE]);
    
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Payload size exceeds maximum allowed size",
            ));
        }
    
        // // Buffer to store the payload
        // let mut payload_buffer = vec![0u8; payload_size];
    
        // // Read the payload based on the size specified in the header
        // self.stream.read_exact(&mut payload_buffer)?;
    
        // // Log the payload contents
        // println!("Payload: {:?}", payload_buffer);
    
        // // Optional: Convert payload to string if it's UTF-8 encoded
        // if let Ok(payload_str) = std::str::from_utf8(&payload_buffer) {
        //     println!("Received message: {}", payload_str);
        // } else {
        //     println!("Received non-UTF8 payload.");
        // }

        // Check if the incoming message exceeds the maximum size

        let mut buffer = [0; MAX_MESSAGE_SIZE]; // Buffer to store incoming data from the client
    
        // Attempt to read data from the client's stream
        let bytes_read = self.stream.read(&mut buffer)?;
        // if bytes_read == 0 {
        //     // If no data is read, it indicates the client has disconnected
        //     info!("Client disconnected.");
        //     return Ok(());
        // }


    
        // Decode the incoming data into a ClientMessage
        if let Ok(client_message) = ClientMessage::decode(&buffer[..payload_size]) {
            // Match the message type and handle accordingly
            match client_message.message {
                // Handle EchoMessage type
                Some(crate::message::client_message::Message::EchoMessage(echo)) => {
                    // Verify the content of the EchoMessage
                    if echo.content.trim().is_empty() {
                        warn!("Received empty EchoMessage content.");
                        let response = ServerMessage {
                            message: Some(crate::message::server_message::Message::EchoMessage(
                                EchoMessage { content: "Received empty EchoMessage content.".to_string() },
                            )),
                            status : 2,
                        };
                        // return Ok(()); // Ignore empty messages
                        // Encode the response into bytes and send it back to the client
                        let mut payload = Vec::new();
                        response.encode(&mut payload).unwrap();
                        self.stream.write_all(&payload)?; // Write the response to the client's stream
                    }
                    else {
                        info!("Received EchoMessage: {}", echo.content); // Log the received EchoMessage

                        // Create a response message echoing back the received content
                        let response = ServerMessage {
                            message: Some(crate::message::server_message::Message::EchoMessage(
                                EchoMessage { content: echo.content },
                            )),
                            status : 1,
                        };
                        // Encode the response into bytes and send it back to the client
                        let mut payload = Vec::new();
                        response.encode(&mut payload).unwrap();
                        self.stream.write_all(&payload)?; // Write the response to the client's stream
                    }
                }
                // Handle AddRequest type
                Some(crate::message::client_message::Message::AddRequest(add_request)) => {
                    // Verify the numbers in the AddRequest
                    if add_request.a < 0 || add_request.b < 0 {
                        warn!("Received AddRequest with negative numbers: {} + {}", add_request.a, add_request.b);
                        return Ok(()); // Ignore invalid requests
                    }
    
                    info!("Received AddRequest: {} + {}", add_request.a, add_request.b); // Log the AddRequest details
    
                    // Compute the sum of the two numbers provided in the request
                    let result = add_request.a + add_request.b;
    
                    // Create a response message with the computed result
                    let response = ServerMessage {
                        message: Some(crate::message::server_message::Message::AddResponse(
                            AddResponse { result },
                        )),
                        status : 1,
                    };
    
                    // Encode the response into bytes and send it back to the client
                    let mut payload = Vec::new();
                    response.encode(&mut payload).unwrap();
                    self.stream.write_all(&payload)?; // Write the response to the client's stream
                }
                // Handle unsupported or unknown message types
                _ => {
                    error!("Received unsupported or unknown message type."); // Log an error for unsupported messages
                }
            }
        } else {
            // Log an error if the incoming data cannot be decoded into a ClientMessage
            error!("Failed to decode incoming ClientMessage.");
        }
    
        Ok(())
    }
    
}

// Server struct that listens for and handles clients
pub struct Server {
    listener: TcpListener, // TCP listener to accept incoming client connections
    is_running: Arc<AtomicBool>, // Atomic flag to indicate whether the server is running
}

impl Server {
    /// Creates a new server
    /// 
    /// # Arguments
    /// * `addr` - The address and port to bind the server to (e.g., "127.0.0.1:8080").
    /// 
    /// # Returns
    /// A new `Server` instance bound to the specified address.
    pub fn new(addr: &str) -> io::Result<Self> {
        let listener = TcpListener::bind(addr)?; // Attempt to bind the listener to the specified address
        let is_running = Arc::new(AtomicBool::new(false)); // Initialize the running flag as false
        Ok(Server {
            listener,
            is_running,
        })
    }

    /// Runs the server to accept and handle client connections
    /// 
    /// The server listens for incoming connections and spawns threads to handle each client.
    pub fn run(&self) -> io::Result<()> {
        self.is_running.store(true, Ordering::SeqCst); // Set the running flag to true
        info!("Server is running on {}", self.listener.local_addr()?); // Log the server's address and status

        // Set the listener to non-blocking mode to avoid blocking the main thread
        self.listener.set_nonblocking(true)?;

        // Main server loop to accept and handle clients
        while self.is_running.load(Ordering::SeqCst) {
            match self.listener.accept() {
                // Handle a new incoming connection
                Ok((stream, addr)) => {
                    info!("New client connected: {}", addr); // Log the address of the connected client

                    // Spawn a new thread to handle the connected client
                    thread::spawn(move || {
                        let mut client = Client::new(stream); // Create a client handler for the connection
                        loop {
                            // Continuously handle messages from the client
                            if let Err(e) = client.handle() {
                                error!("Error handling client {}: {}", addr, e); // Log any errors encountered
                                break; // Exit the loop if an error occurs or the client disconnects
                            }
                        }
                        info!("Client {} disconnected.", addr); // Log when the client disconnects
                    });
                }
                // Handle cases where no incoming connection is available
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    // Sleep briefly to reduce CPU usage during idle periods
                    thread::sleep(Duration::from_millis(100));
                }
                // Log other errors that occur while accepting connections
                Err(e) => {
                    error!("Error accepting connection: {}", e);
                }
            }
        }

        info!("Server stopped."); // Log when the server stops
        Ok(())
    }

    /// Stops the server
    /// 
    /// This method signals the server to stop accepting new connections and terminates the main loop.
    pub fn stop(&self) {
        if self.is_running.load(Ordering::SeqCst) {
            self.is_running.store(false, Ordering::SeqCst); // Set the running flag to false
            info!("Shutdown signal sent."); // Log that a shutdown signal was sent
        } else {
            warn!("Server was already stopped or not running."); // Log if the server was already stopped
        }
    }
}
