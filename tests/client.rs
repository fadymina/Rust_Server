use embedded_recruitment_task::message::{client_message, ServerMessage};
use log::error;
use log::info;
use prost::Message;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

/// Represents a TCP/IP Client.
///
/// The `Client` struct provides the functionality to connect to a server,
/// send and receive messages, and disconnect cleanly. It uses the TCP protocol
/// and supports timeout configuration for connection attempts.
pub struct Client {
    /// The server's IP address.
    ip: String,
    /// The server's port number.
    port: u32,
    /// Timeout duration for connecting to the server.
    timeout: Duration,
    /// An optional TCP stream representing the active connection.
    stream: Option<TcpStream>,
}

impl Client {
    /// Creates a new `Client` instance.
    ///
    /// # Arguments
    /// * `ip` - The IP address of the server as a string slice.
    /// * `port` - The port number to connect to.
    /// * `timeout_ms` - The timeout duration in milliseconds for connection attempts.
    ///
    /// # Returns
    /// A `Client` instance with the specified configuration.
    pub fn new(ip: &str, port: u32, timeout_ms: u64) -> Self {
        Client {
            ip: ip.to_string(),
            port,
            timeout: Duration::from_millis(timeout_ms),
            stream: None,
        }
    }

    /// Connects the client to the server.
    ///
    /// Attempts to establish a TCP connection with the server at the specified IP
    /// and port. Upon successful connection, it waits for a handshake response
    /// from the server to confirm the connection.
    ///
    /// # Errors
    /// Returns an `io::Error` if:
    /// - The IP or port is invalid.
    /// - The connection times out.
    /// - The handshake fails due to an unexpected response from the server.
    pub fn connect(&mut self) -> io::Result<()> {
        println!("Connecting to {}:{}", self.ip, self.port);

        // Format the address as "IP:Port" and resolve to socket addresses.
        let address = format!("{}:{}", self.ip, self.port);
        let socket_addrs: Vec<SocketAddr> = address.to_socket_addrs()?.collect();

        // Validate that at least one valid socket address is found.
        if socket_addrs.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid IP or port",
            ));
        }

        // Attempt to establish a connection within the timeout duration.
        let mut stream = TcpStream::connect_timeout(&socket_addrs[0], self.timeout)?;

        // Read the handshake response from the server.
        let mut buffer = [0; 1024];
        let bytes_read = stream.read(&mut buffer)?;
        let response = String::from_utf8_lossy(&buffer[..bytes_read]);

        // Check if the server sent the expected "CONNECTED" handshake response.
        if response.trim() == "CONNECTED" {
            println!("Connected to the server!");
            self.stream = Some(stream); // Store the active stream in the client.
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::ConnectionRefused,
                format!("Handshake failed: {}", response.trim()),
            ))
        }
    }

    /// Disconnects the client from the server.
    ///
    /// Closes the active TCP connection and cleans up the resources associated
    /// with the connection.
    ///
    /// # Errors
    /// Returns an `io::Error` if the disconnection fails unexpectedly.
    pub fn disconnect(&mut self) -> io::Result<()> {
        if let Some(stream) = self.stream.take() {
            stream.shutdown(std::net::Shutdown::Both)?; // Shut down the connection.
        }

        println!("Disconnected from the server!");
        Ok(())
    }

    /// Sends a message to the server.
    ///
    /// Encodes the provided `client_message::Message` into a binary payload
    /// and transmits it to the server. A 4-byte size header is prepended to the payload
    /// to indicate its length.
    ///
    /// # Arguments
    /// * `message` - A `client_message::Message` representing the data to send.
    ///
    /// # Errors
    /// Returns an `io::Error` if:
    /// - The connection is not active.
    /// - The message size exceeds the maximum allowable limit.
    /// - Writing to the server fails.
    pub fn send(&mut self, message: client_message::Message) -> io::Result<()> {
        if let Some(ref mut stream) = self.stream {
            // Encode the message into a buffer.
            let mut buffer = Vec::new();
            message.encode(&mut buffer);

            // Calculate the size of the payload and validate it.
            let payload_size = buffer.len();
            if payload_size > u32::MAX as usize {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Payload size exceeds maximum limit",
                ));
            }

            // Convert the size to a 4-byte big-endian array.
            let size_header = (payload_size as u32).to_be_bytes();

            // Send the size header and the payload to the server.
            stream.write_all(&size_header)?;
            stream.write_all(&buffer)?;

            stream.flush()?; // Ensure all data is sent.
            println!("Sent message: {:?}", message);

            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "No active connection",
            ))
        }
    }

    /// Receives a message from the server.
    ///
    /// Reads a binary payload from the server and decodes it into a `ServerMessage`.
    /// Handles server-initiated shutdowns gracefully by disconnecting cleanly.
    ///
    /// # Returns
    /// A `Result` containing the decoded `ServerMessage` or an `io::Error` if:
    /// - The connection is not active.
    /// - The server disconnects unexpectedly.
    /// - The received data cannot be decoded into a `ServerMessage`.
    pub fn receive(&mut self) -> io::Result<ServerMessage> {
        if let Some(ref mut stream) = self.stream {
            info!("Receiving message from the server");

            // Allocate a buffer to read the incoming payload.
            let mut buffer = vec![0u8; 1024];
            let bytes_read = stream.read(&mut buffer)?;
            if bytes_read == 0 {
                info!("Server disconnected.");
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "Server disconnected",
                ));
            }

            // Check for a server-initiated shutdown message.
            let response = String::from_utf8_lossy(&buffer[..bytes_read]);
            if response.trim() == "SHUTDOWN" {
                info!("Server is shutting down.");
                self.disconnect()?; // Cleanly disconnect the client.
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "Server shutting down",
                ));
            }

            // Attempt to decode the received payload into a `ServerMessage`.
            ServerMessage::decode(&buffer[..bytes_read]).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Failed to decode ServerMessage: {}", e),
                )
            })
        } else {
            error!("No active connection");
            Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "No active connection",
            ))
        }
    }
    #[cfg(test)]
    pub fn stream_mut(&mut self) -> Option<&mut TcpStream> {
        self.stream.as_mut()
    }
}
