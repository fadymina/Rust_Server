use embedded_recruitment_task::message::{client_message, ServerMessage};
use log::error;
use log::info;
use prost::Message;
use std::io::Read;
use std::io::Write;
use std::{
    io,
    net::{SocketAddr, TcpStream, ToSocketAddrs},
    time::Duration,
};

// TCP/IP Client
pub struct Client {
    ip: String,
    port: u32,
    timeout: Duration,
    stream: Option<TcpStream>,
}

impl Client {
    pub fn new(ip: &str, port: u32, timeout_ms: u64) -> Self {
        Client {
            ip: ip.to_string(),
            port,
            timeout: Duration::from_millis(timeout_ms),
            stream: None,
        }
    }

    // connect the client to the server
    pub fn connect(&mut self) -> io::Result<()> {
        println!("Connecting to {}:{}", self.ip, self.port);
    
        let address = format!("{}:{}", self.ip, self.port);
        let socket_addrs: Vec<SocketAddr> = address.to_socket_addrs()?.collect();
    
        if socket_addrs.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid IP or port",
            ));
        }
    
        let mut stream = TcpStream::connect_timeout(&socket_addrs[0], self.timeout)?;
    
        // Read handshake from the server
        let mut buffer = [0; 1024];
        let bytes_read = stream.read(&mut buffer)?;
        let response = String::from_utf8_lossy(&buffer[..bytes_read]);
    
        if response.trim() == "CONNECTED" {
            println!("Connected to the server!");
            self.stream = Some(stream);
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::ConnectionRefused,
                format!("Handshake failed: {}", response.trim()),
            ))
        }
    }
    
    


    // disconnect the client
    pub fn disconnect(&mut self) -> io::Result<()> {
        if let Some(stream) = self.stream.take() {
            stream.shutdown(std::net::Shutdown::Both)?;
        }

        println!("Disconnected from the server!");
        Ok(())
    }

    // generic message to send message to the server
    pub fn send(&mut self, message: client_message::Message) -> io::Result<()> {
        if let Some(ref mut stream) = self.stream {
            // Encode the message to a buffer
            let mut buffer = Vec::new();
            message.encode(&mut buffer);

            // Get the size of the buffer
            let payload_size = buffer.len();
            if payload_size > u32::MAX as usize {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Payload size exceeds maximum limit",
                ));
            }

            // Convert the size to a 4-byte big-endian array
            let size_header = (payload_size as u32).to_be_bytes();

            // Prepend the size header to the payload
            stream.write_all(&size_header)?;
            stream.write_all(&buffer)?;

            stream.flush()?;
            println!("Sent message: {:?}", message);

            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "No active connection",
            ))
        }
    }

    pub fn receive(&mut self) -> io::Result<ServerMessage> {
        if let Some(ref mut stream) = self.stream {
            info!("Receiving message from the server");
            let mut buffer = vec![0u8; 1024];
            let bytes_read = stream.read(&mut buffer)?;
            if bytes_read == 0 {
                info!("Server disconnected.");
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "Server disconnected",
                ));
            }
    
            let response = String::from_utf8_lossy(&buffer[..bytes_read]);
            if response.trim() == "SHUTDOWN" {
                info!("Server is shutting down.");
                self.disconnect()?; // Cleanly disconnect
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "Server shutting down",
                ));
            }
    
            // Decode the received message
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
    
}
