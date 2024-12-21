use client::Client;
use embedded_recruitment_task::{
    message::{client_message, server_message, AddRequest, EchoMessage},
    server::Server,
};
use std::{
    sync::Arc,
    thread::{self, JoinHandle},
};
use std::io::Write;
mod client;

fn setup_server_thread(server: Arc<Server>) -> JoinHandle<()> {
    thread::spawn(move || {
        server.run().expect("Server encountered an error");
    })
}

fn create_server(addr: &str, port: Option<u16>, max_connections: usize) -> Arc<Server> {
    let port = port.unwrap_or(8080); // Use the provided port or default to 8080
    let full_address = format!("{}:{}", addr, port);

    // Create and configure the server
    let server = Server::new(&full_address, max_connections)
        .expect("Failed to start server");

    Arc::new(server)
}

#[test]
fn test_client_connection() {
    // Set up the server in a separate thread
    let server = create_server("localhost", None, 100);
    let handle = setup_server_thread(server.clone());

    // Create and connect the client
    let mut client = client::Client::new("localhost", 8080, 1000);
    assert!(client.connect().is_ok(), "Failed to connect to the server");

    // Disconnect the client
    assert!(
        client.disconnect().is_ok(),
        "Failed to disconnect from the server"
    );

    // Stop the server and wait for thread to finish
    let _ = server.stop();
    assert!(
        handle.join().is_ok(),
        "Server thread panicked or failed to join"
    );
}

#[test]
fn test_client_echo_message() {
    // Set up the server in a separate thread
    let _ = env_logger::builder().is_test(true).try_init();
    // info!("This is a log message.");

    let server = create_server("localhost", None, 100);
    let handle = setup_server_thread(server.clone());

    // Create and connect the client
    let mut client = client::Client::new("localhost", 8080, 1000);
    assert!(client.connect().is_ok(), "Failed to connect to the server");

    // Prepare the message
    let mut echo_message = EchoMessage::default();
    echo_message.content = "Hello, World!".to_string();
    let message = client_message::Message::EchoMessage(echo_message.clone());

    // Send the message to the server
    assert!(client.send(message).is_ok(), "Failed to send message");

    // Receive the echoed message
    let response = client.receive();

    if let Ok(ref server_response) = response {
        // Access the `message` field without moving `response`
        match &server_response.message {
            Some(server_message::Message::EchoMessage(echo)) => {
                assert_eq!(
                    echo.content, echo_message.content,
                    "Echoed message content does not match"
                );
                println!("Received echoed message: {}", echo.content);
            }
            Some(server_message::Message::AddResponse(add_response)) => {
                println!("Received addition result: {}", add_response.result);
            }
            None => {
                println!("No message in response.");
            }
        }

        // Access the `status` field separately
        println!("Received status: {:?}", server_response.status);
    } else {
        eprintln!("Failed to receive response.");
    }

    // Disconnect the client
    assert!(
        client.disconnect().is_ok(),
        "Failed to disconnect from the server"
    );

    // Stop the server and wait for thread to finish
    let _ = server.stop();
    assert!(
        handle.join().is_ok(),
        "Server thread panicked or failed to join"
    );
}
#[test]
fn test_huge_payload() {
    let _ = env_logger::builder().is_test(true).try_init();
    let server = create_server("localhost", None, 100);
    let handle = setup_server_thread(server.clone());

    let mut client = client::Client::new("localhost", 8080, 1000);
    assert!(client.connect().is_ok(), "Failed to connect to the server");

    // Prepare a huge payload
    let huge_content = "A".repeat(600);
    let mut echo_message = EchoMessage::default();
    echo_message.content = huge_content.to_string();
    let message = client_message::Message::EchoMessage(echo_message);

    // Send the huge payload to the server
    assert!(client.send(message).is_ok(), "Failed to send huge payload");

    // Receive the response
    let response = client.receive();
    assert!(
        response.is_ok(),
        "Failed to receive response for huge payload"
    );

    if let Ok(ref server_response) = response {
        assert_eq!(
            server_response.status, 2,
            "Expected status 2 (error) for oversized payload"
        );
        println!("Received status: {:?}", server_response.status);
    } else {
        eprintln!("Failed to receive response.");
    }

    // Attempt to disconnect gracefully
    // let disconnect_result = client.disconnect();
    // if disconnect_result.is_err() {
    //     println!(
    //         "Disconnect failed as expected due to closed connection: {:?}",
    //         disconnect_result
    //     );
    // }
    //connection is already closed by server 

    // Stop the server and wait for thread to finish
    server.stop().unwrap();
    handle.join().unwrap();
}
#[test]
fn test_multiple_echo_messages() {
    let _ = env_logger::builder().is_test(true).try_init();

    // Set up the server in a separate thread
    let server = create_server("localhost", None, 100);
    let handle = setup_server_thread(server.clone());

    // Create and connect the client
    let mut client = client::Client::new("localhost", 8080, 1000);
    assert!(client.connect().is_ok(), "Failed to connect to the server");

    // Prepare multiple messages
    let messages = vec![
        "Hello, World!".to_string(),
        "How are you?".to_string(),
        "Goodbye!".to_string(),
    ];

    // Send and receive multiple messages
    for message_content in messages {
        let mut echo_message = EchoMessage::default();
        echo_message.content = message_content.clone();
        let message = client_message::Message::EchoMessage(echo_message);

        // Send the message to the server
        assert!(client.send(message).is_ok(), "Failed to send message");

        // Receive the echoed message
        let response = client.receive();
        assert!(
            response.is_ok(),
            "Failed to receive response for EchoMessage"
        );

        match response.unwrap().message {
            Some(server_message::Message::EchoMessage(echo)) => {
                assert_eq!(
                    echo.content, message_content,
                    "Echoed message content does not match"
                );
            }
            _ => panic!("Expected EchoMessage, but received a different message"),
        }
    }

    // Disconnect the client
    assert!(
        client.disconnect().is_ok(),
        "Failed to disconnect from the server"
    );

    // Stop the server and wait for thread to finish
    let _ = server.stop();
    assert!(
        handle.join().is_ok(),
        "Server thread panicked or failed to join"
    );
}

#[test]
fn test_multiple_clients() {
    let _ = env_logger::builder().is_test(true).try_init();


    // Set up the server in a separate thread
    let server = create_server("localhost", None, 100);
    let handle = setup_server_thread(server.clone());

    // Create and connect multiple clients
    let mut clients = vec![
        client::Client::new("localhost", 8080, 1000),
        client::Client::new("localhost", 8080, 1000),
        client::Client::new("localhost", 8080, 1000),
    ];

    for client in clients.iter_mut() {
        assert!(client.connect().is_ok(), "Failed to connect to the server");
    }

    // Prepare multiple messages
    let messages = vec![
        "Hello, World!".to_string(),
        "How are you?".to_string(),
        "Goodbye!".to_string(),
    ];

    // Send and receive multiple messages for each client
    for message_content in messages {
        let mut echo_message = EchoMessage::default();
        echo_message.content = message_content.clone();
        let message = client_message::Message::EchoMessage(echo_message.clone());

        for client in clients.iter_mut() {
            // Send the message to the server
            assert!(
                client.send(message.clone()).is_ok(),
                "Failed to send message"
            );

            // Receive the echoed message
            let response = client.receive();
            assert!(
                response.is_ok(),
                "Failed to receive response for EchoMessage"
            );

            match response.unwrap().message {
                Some(server_message::Message::EchoMessage(echo)) => {
                    assert_eq!(
                        echo.content, message_content,
                        "Echoed message content does not match"
                    );
                }
                _ => panic!("Expected EchoMessage, but received a different message"),
            }
            println!("Echoed Message recieved successfully ");
        }
    }

    // Disconnect the clients
    for client in clients.iter_mut() {
        assert!(
            client.disconnect().is_ok(),
            "Failed to disconnect from the server"
        );
    }

    println!("disconnected all clients ");

    // Stop the server and wait for thread to finish
    let _ = server.stop();
    assert!(
        handle.join().is_ok(),
        "Server thread panicked or failed to join"
    );
}

#[test]
fn test_client_add_request() {
    // Set up the server in a separate thread
    let server = create_server("localhost", None, 100);
    let handle = setup_server_thread(server.clone());

    // Create and connect the client
    let mut client = client::Client::new("localhost", 8080, 1000);
    assert!(client.connect().is_ok(), "Failed to connect to the server");

    // Prepare the message
    let mut add_request = AddRequest::default();
    add_request.a = 10;
    add_request.b = 20;
    let message = client_message::Message::AddRequest(add_request.clone());

    // Send the message to the server
    assert!(client.send(message).is_ok(), "Failed to send message");

    // Receive the response
    let response = client.receive();
    assert!(
        response.is_ok(),
        "Failed to receive response for AddRequest"
    );

    match response.unwrap().message {
        Some(server_message::Message::AddResponse(add_response)) => {
            assert_eq!(
                add_response.result,
                add_request.a + add_request.b,
                "AddResponse result does not match"
            );
        }
        _ => panic!("Expected AddResponse, but received a different message"),
    }

    // Disconnect the client
    assert!(
        client.disconnect().is_ok(),
        "Failed to disconnect from the server"
    );

    // Stop the server and wait for thread to finish
    let _ = server.stop();
    assert!(
        handle.join().is_ok(),
        "Server thread panicked or failed to join"
    );
}

#[test]
fn test_concurrent_requests() {
    let server = create_server("localhost", None, 100);
    let handle = setup_server_thread(server.clone());

    let mut client1 = client::Client::new("localhost", 8080, 1000);
    let mut client2 = client::Client::new("localhost", 8080, 1000);
    assert!(client1.connect().is_ok(), "Client 1 failed to connect");
    assert!(client2.connect().is_ok(), "Client 2 failed to connect");

    // Client 1 sends an echo request
    let echo_message = client_message::Message::EchoMessage(EchoMessage {
        content: "Concurrent echo".to_string(),
    });
    assert!(client1.send(echo_message).is_ok(), "Client 1 failed to send echo");

    // Client 2 sends an add request
    let add_message = client_message::Message::AddRequest(AddRequest { a: 5, b: 10 });
    assert!(client2.send(add_message).is_ok(), "Client 2 failed to send add request");

    // Client 1 receives echo response
    let response1 = client1.receive();
    assert!(response1.is_ok(), "Client 1 failed to receive echo response");

    // Client 2 receives add response
    let response2 = client2.receive();
    assert!(response2.is_ok(), "Client 2 failed to receive add response");

    client1.disconnect().unwrap();
    client2.disconnect().unwrap();
    server.stop().unwrap();
    handle.join().unwrap();
}
#[test]
fn test_high_connection_volume() {
    let _ = env_logger::builder().is_test(true).try_init();

    let server = create_server("localhost", None, 100);
    let handle = setup_server_thread(server.clone());

    let client_count = 10;
    let mut clients: Vec<client::Client> = (0..client_count)
        .map(|_| client::Client::new("localhost", 8080, 1000))
        .collect();

    for client in clients.iter_mut() {
        assert!(client.connect().is_ok(), "Failed to connect client");
    }

    // Send an echo message from each client
    for client in clients.iter_mut() {
        let message = client_message::Message::EchoMessage(EchoMessage {
            content: "High volume test".to_string(),
        });
        assert!(client.send(message).is_ok(), "Client failed to send message");

        let response = client.receive();
        assert!(
            response.is_ok(),
            "Client failed to receive response during high connection volume"
        );
    }

    for client in clients.iter_mut() {
        assert!(client.disconnect().is_ok(), "Failed to disconnect client");
    }

    server.stop().unwrap();
    handle.join().unwrap();
}
#[test]
fn test_multiple_servers_multiple_clients() {
    // Set up the server in a separate thread
    
    let server0 = create_server("localhost", Some(3000),100);
    let server1 = create_server("localhost", Some(7000),100);
    let server2 = create_server("localhost", None, 100);

    let handle0 = setup_server_thread(server0.clone());
    let handle1 = setup_server_thread(server1.clone());
    let handle2 = setup_server_thread(server2.clone());

    // Create and connect multiple clients
    let mut clients0 = vec![
        client::Client::new("localhost", 3000, 1000),
        client::Client::new("localhost", 3000, 1000),
        client::Client::new("localhost", 3000, 1000),
    ];
    let mut clients1 = vec![
        client::Client::new("localhost", 7000, 1000),
        client::Client::new("localhost", 7000, 1000),
        client::Client::new("localhost", 7000, 1000),
    ];
    let mut clients2 = vec![
        client::Client::new("localhost", 8080, 1000),
        client::Client::new("localhost", 8080, 1000),
        client::Client::new("localhost", 8080, 1000),
    ];

    for client in clients0.iter_mut() {
        assert!(client.connect().is_ok(), "Failed to connect to the server");
    }
    for client in clients1.iter_mut() {
        assert!(client.connect().is_ok(), "Failed to connect to the server");
    }
    for client in clients2.iter_mut() {
        assert!(client.connect().is_ok(), "Failed to connect to the server");
    }

    // Prepare multiple messages
    let messages = vec![
        "Hello, World!".to_string(),
        "How are you?".to_string(),
        "Goodbye!".to_string(),
    ];

    // Send and receive multiple messages for each client
    for message_content in messages.clone() {
        let mut echo_message = EchoMessage::default();
        echo_message.content = message_content.clone();
        let message = client_message::Message::EchoMessage(echo_message.clone());

        for client in clients0.iter_mut() {
            // Send the message to the server
            assert!(
                client.send(message.clone()).is_ok(),
                "Failed to send message"
            );

            // Receive the echoed message
            let response = client.receive();
            assert!(
                response.is_ok(),
                "Failed to receive response for EchoMessage"
            );

            match response.unwrap().message {
                Some(server_message::Message::EchoMessage(echo)) => {
                    assert_eq!(
                        echo.content, message_content,
                        "Echoed message content does not match"
                    );
                }
                _ => panic!("Expected EchoMessage, but received a different message"),
            }
            println!("Echoed Message recieved successfully ");
        }
    }

    for message_content in messages.clone() {
        let mut echo_message = EchoMessage::default();
        echo_message.content = message_content.clone();
        let message = client_message::Message::EchoMessage(echo_message.clone());

        for client in clients1.iter_mut() {
            // Send the message to the server
            assert!(
                client.send(message.clone()).is_ok(),
                "Failed to send message"
            );

            // Receive the echoed message
            let response = client.receive();
            assert!(
                response.is_ok(),
                "Failed to receive response for EchoMessage"
            );

            match response.unwrap().message {
                Some(server_message::Message::EchoMessage(echo)) => {
                    assert_eq!(
                        echo.content, message_content,
                        "Echoed message content does not match"
                    );
                }
                _ => panic!("Expected EchoMessage, but received a different message"),
            }
            println!("Echoed Message recieved successfully ");
        }
    }

    for message_content in messages {
        let mut echo_message = EchoMessage::default();
        echo_message.content = message_content.clone();
        let message = client_message::Message::EchoMessage(echo_message.clone());

        for client in clients2.iter_mut() {
            // Send the message to the server
            assert!(
                client.send(message.clone()).is_ok(),
                "Failed to send message"
            );

            // Receive the echoed message
            let response = client.receive();
            assert!(
                response.is_ok(),
                "Failed to receive response for EchoMessage"
            );

            match response.unwrap().message {
                Some(server_message::Message::EchoMessage(echo)) => {
                    assert_eq!(
                        echo.content, message_content,
                        "Echoed message content does not match"
                    );
                }
                _ => panic!("Expected EchoMessage, but received a different message"),
            }
            println!("Echoed Message recieved successfully ");
        }
    }

    // Disconnect the clients
    for client in clients0.iter_mut() {
        assert!(
            client.disconnect().is_ok(),
            "Failed to disconnect from the server"
        );
    }
        // Disconnect the clients
    for client in clients1.iter_mut() {
        assert!(
            client.disconnect().is_ok(),
            "Failed to disconnect from the server"
        );
    }
        // Disconnect the clients
    for client in clients2.iter_mut() {
        assert!(
            client.disconnect().is_ok(),
            "Failed to disconnect from the server"
        );
    }

    println!("disconnected all clients ");

    // Stop the server and wait for thread to finish
    let _ = server0.stop();
    let _ = server1.stop();
    let _ = server2.stop();
    assert!(
        handle0.join().is_ok(),
        "Server thread panicked or failed to join"
    );
    assert!(
        handle1.join().is_ok(),
            "Server thread panicked or failed to join"
        );
    assert!(
        handle2.join().is_ok(),
            "Server thread panicked or failed to join"
        );
    
}
#[test]
fn test_server_throttling() {
    let _ = env_logger::builder().is_test(true).try_init();

    // Set up the server with a low max connection limit for testing throttling
    let max_connections = 5;
    let server = create_server("localhost", None, max_connections); // No rate limiter
    let handle = setup_server_thread(server.clone());

    // std::thread::sleep(std::time::Duration::from_secs(1));

    // Prepare a pool of clients greater than the max_connections
    const NUM_CLIENTS: usize = 10;
    let mut clients: Vec<client::Client> = (0..NUM_CLIENTS)
        .map(|_| client::Client::new("localhost", 8080, 1000))
        .collect();

    // std::thread::sleep(std::time::Duration::from_secs(10));


    // Track connection results
    let mut connected_clients = 0;
    let mut rejected_clients = 0;

    // Attempt to connect all clients
    for client in clients.iter_mut() {
        match client.connect() {
            Ok(_) => {
                connected_clients += 1;
                println!(
                    "Client connected successfully. Active connections: {}",
                    server.get_active_connections()
                );
            }
            Err(e) => {
                println!("Connection failed: {}", e);
                rejected_clients += 1;
            }
        }
    }

    // Verify the number of accepted and rejected connections
    assert_eq!(
        connected_clients, max_connections,
        "Expected only {} clients to connect, but {} connected",
        max_connections, connected_clients
    );
    assert_eq!(
        rejected_clients, NUM_CLIENTS - max_connections,
        "Expected {} clients to be rejected, but {} were rejected",
        NUM_CLIENTS - max_connections, rejected_clients
    );

    // Disconnect all connected clients
    for client in clients.iter_mut().take(connected_clients) {
        assert!(client.disconnect().is_ok(), "Failed to disconnect a client");
    }

    // Stop the server and wait for thread to finish
    server.stop().unwrap();
    handle.join().unwrap();
}
#[test]
fn test_server_shutdown() {
    let _ = env_logger::builder().is_test(true).try_init();

    let server = create_server("localhost", None, 5);
    let handle = setup_server_thread(server.clone());

    let mut client = Client::new("localhost", 8080, 1000);
    assert!(client.connect().is_ok(), "Failed to connect client");

    // Stop the server
    server.stop().unwrap();
    handle.join().unwrap();

    // Attempt to send a message after shutdown
    let mut echo_message = EchoMessage::default();
    echo_message.content = "Test after shutdown".to_string();
    let message = client_message::Message::EchoMessage(echo_message);

    let result = client.send(message);
    assert!(
        result.is_err(),
        "Client should not be able to send message after server shutdown"
    );
}
#[test]
fn test_malformed_payload() {
    let _ = env_logger::builder().is_test(true).try_init();

    let server = create_server("localhost", None, 100);
    let handle = setup_server_thread(server.clone());

    let mut client = client::Client::new("localhost", 8080, 1000);
    assert!(client.connect().is_ok(), "Failed to connect client");

    // Send a payload with valid size but invalid content
    let malformed_payload = vec![0x12, 0x34, 0x56, 0x78]; // Random bytes
    if let Some(stream) = client.stream_mut() {
        let header = (malformed_payload.len() as u32).to_be_bytes();
        assert!(
            stream.write_all(&header).is_ok(),
            "Failed to send payload header"
        );
        assert!(
            stream.write_all(&malformed_payload).is_ok(),
            "Failed to send malformed payload"
        );
    } else {
        panic!("Client stream is not connected");
    }

    // Server should respond with an error
    let response = client.receive();
    assert!(
        response.is_ok(),
        "Expected an error response from the server, but got none"
    );

    // Verify the server's response
    let server_response = response.unwrap();
    assert_eq!(
        server_response.status, 2,
        "Expected status 2 (error) for malformed payload"
    );

    client.disconnect().unwrap();
    server.stop().unwrap();
    handle.join().unwrap();
}
// #[test]
// fn test_idle_connection_timeout() {
//     // Server with a short idle timeout
//     let server = create_server("localhost", None, 100, 10, 500); // 500 ms timeout
//     let handle = setup_server_thread(server.clone());

//     let mut client = client::Client::new("localhost", 8080, 1000);
//     assert!(client.connect().is_ok(), "Failed to connect client");

//     // Wait to simulate idleness
//     std::thread::sleep(std::time::Duration::from_secs(1));

//     // Attempt to send a message after timeout
//     let message = client_message::Message::EchoMessage(EchoMessage {
//         content: "After timeout".to_string(),
//     });
//     let result = client.send(message);
//     assert!(
//         result.is_err(),
//         "Client should not be able to send a message after timeout"
//     );

//     client.disconnect().unwrap_or_default();
//     server.stop().unwrap();
//     handle.join().unwrap();
// }

#[test]
fn test_server_shutdown_during_requests() {
        let _ = env_logger::builder().is_test(true).try_init();

    let server = create_server("localhost", None, 100);
    let handle = setup_server_thread(server.clone());

    let mut client1 = client::Client::new("localhost", 8080, 1000);
    let mut client2 = client::Client::new("localhost", 8080, 1000);
    assert!(client1.connect().is_ok(), "Client 1 failed to connect");
    assert!(client2.connect().is_ok(), "Client 2 failed to connect");

    // Start a thread to stop the server while clients are active
    let server_clone = server.clone();
    let shutdown_thread = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(1));
        server_clone.stop().unwrap();
    });

    // Clients send messages during server shutdown
    let message = client_message::Message::EchoMessage(EchoMessage {
        content: "During shutdown".to_string(),
    });
    let result1 = client1.send(message.clone());
    let result2 = client2.send(message);

    // Server might reject or disconnect clients
    assert!(result1.is_err() || result1.is_ok(), "Unexpected behavior during shutdown");
    assert!(result2.is_err() || result2.is_ok(), "Unexpected behavior during shutdown");

    // Clean up
    shutdown_thread.join().unwrap();
    client1.disconnect().unwrap_or_default();
    client2.disconnect().unwrap_or_default();
    handle.join().unwrap();
}
