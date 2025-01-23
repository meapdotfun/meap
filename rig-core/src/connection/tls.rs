//! TLS connection handling for secure WebSocket connections.

use crate::error::{Error, Result};
use crate::security::TlsConfig;
use rustls::{
    Certificate, PrivateKey,
    ServerConfig,
    server::{ClientCertVerified, ClientCertVerifier},
};
use std::{
    fs::File,
    io::BufReader,
    sync::Arc,
};
use tokio_rustls::TlsAcceptor;

/// Loads certificates from PEM files
fn load_certs(path: &str) -> Result<Vec<Certificate>> {
    let file = File::open(path)
        .map_err(|e| Error::Security(format!("Failed to open cert file: {}", e)))?;
    let mut reader = BufReader::new(file);
    let certs = rustls_pemfile::certs(&mut reader)
        .map_err(|e| Error::Security(format!("Failed to parse cert file: {}", e)))?;
    Ok(certs.into_iter().map(Certificate).collect())
}

/// Loads private key from PEM file
fn load_private_key(path: &str) -> Result<PrivateKey> {
    let file = File::open(path)
        .map_err(|e| Error::Security(format!("Failed to open key file: {}", e)))?;
    let mut reader = BufReader::new(file);
    let keys = rustls_pemfile::pkcs8_private_keys(&mut reader)
        .map_err(|e| Error::Security(format!("Failed to parse key file: {}", e)))?;
    
    if keys.is_empty() {
        return Err(Error::Security("No private key found".into()));
    }
    
    Ok(PrivateKey(keys[0].clone()))
}

/// Creates a TLS acceptor from configuration
pub async fn create_tls_acceptor(config: &TlsConfig) -> Result<TlsAcceptor> {
    let certs = load_certs(&config.cert_path)?;
    let key = load_private_key(&config.key_path)?;

    let mut server_config = ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth();

    server_config.set_single_cert(certs, key)
        .map_err(|e| Error::Security(format!("TLS config error: {}", e)))?;

    if let Some(ca_certs) = &config.ca_certs {
        let mut client_auth_roots = rustls::RootCertStore::empty();
        for ca_path in ca_certs {
            let ca_certs = load_certs(ca_path)?;
            for cert in ca_certs {
                client_auth_roots
                    .add(&cert)
                    .map_err(|e| Error::Security(format!("Failed to add CA cert: {}", e)))?;
            }
        }

        server_config.set_client_verifier(Arc::new(ClientCertVerifier::new(client_auth_roots)))
            .map_err(|e| Error::Security(format!("Failed to set client verifier: {}", e)))?;
    }

    Ok(TlsAcceptor::from(Arc::new(server_config)))
} 