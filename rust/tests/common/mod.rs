//! Helpers shared by the integration tests that talk to a [`LinksServer`].

#![allow(dead_code)] // Every test crate compiles this module but uses only part of it.

use link_cli::protocol::{
    BinaryLinoOptions, BinaryLinoProtocol, LinksClient, LinksServer, LinoProtocol, ServerOptions,
    TextLinoProtocol,
};
use link_cli::{LinkStorage, NamedTypesDecorator};
use std::net::SocketAddr;
use std::thread::{self, JoinHandle};
use tempfile::NamedTempFile;

/// A [`LinksServer`] serving a temporary store on an ephemeral port; dropping
/// it stops the server.
pub struct RunningServer {
    pub address: SocketAddr,
    pub shutdown: link_cli::protocol::ShutdownHandle,
    pub thread: Option<JoinHandle<()>>,
    _database: NamedTempFile,
}

impl RunningServer {
    pub fn start(options: ServerOptions, named: bool) -> Self {
        let database = NamedTempFile::new().unwrap();
        let path = database.path().to_path_buf();
        let server = LinksServer::bind("127.0.0.1:0", options).unwrap();
        let address = server.local_addr().unwrap();
        let shutdown = server.shutdown_handle().unwrap();
        let thread = thread::spawn(move || {
            if named {
                let mut storage = NamedTypesDecorator::new(&path, false).unwrap();
                server.serve(&mut storage).unwrap();
            } else {
                let mut storage = LinkStorage::new(&path, false).unwrap();
                server.serve(&mut storage).unwrap();
            }
        });
        Self {
            address,
            shutdown,
            thread: Some(thread),
            _database: database,
        }
    }

    pub fn client(&self, protocol: impl LinoProtocol + 'static) -> LinksClient {
        LinksClient::connect(self.address, protocol).unwrap()
    }

    /// Stops the server and waits for it to finish.
    pub fn stop(&mut self) {
        self.shutdown.shutdown();
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn protocols() -> Vec<Box<dyn LinoProtocol>> {
    let mut protocols: Vec<Box<dyn LinoProtocol>> = vec![Box::new(TextLinoProtocol::new())];
    for external_references in [false, true] {
        for sequences in [false, true] {
            for progressive_widths in [false, true] {
                protocols.push(Box::new(BinaryLinoProtocol::with_options(
                    BinaryLinoOptions {
                        external_references,
                        sequences,
                        progressive_widths,
                    },
                )));
            }
        }
    }
    protocols
}
