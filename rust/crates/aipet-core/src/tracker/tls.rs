//! A hosted tracker's mutual TLS (fnt-81), as the tracker's own enrolling side does it (`net::enroll::client`):
//!
//! - **The code** is `<secret>.<fingerprint>`, the fingerprint being the SHA-256 (hex) of the tracker's CA
//!   certificate. The first connection trusts the server only if its chain holds that CA and the CA issued its
//!   certificate ([`pin_ca`]): that is how the pet learns the CA, which no system trusts.
//! - **The key** is made here and never leaves the pet: the tracker gets a CSR for it ([`key_and_csr`]), and sends
//!   back the certificate, then the CA's.
//! - **Every later connection** presents that certificate and trusts that CA only ([`client_config`]; `ureq` gets
//!   the same through [`crate::http::Http::tracker`]).

use std::io;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rcgen::{CertificateParams, DistinguishedName, KeyPair};
use rustls::client::WebPkiServerVerifier;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::{ClientConfig, ClientConnection, DigitallySignedStruct, RootCertStore, SignatureScheme};

use crate::http::{base64, lock};

const BEGIN: &str = "-----BEGIN CERTIFICATE-----";

/// The code's two parts: the secret and the CA's fingerprint. A code without a fingerprint is a loopback code.
pub fn split_code(code: &str) -> Result<(&str, String), String> {
    let wrong = || "A hosted tracker's code looks like <secret>.<fingerprint>: copy it whole".to_owned();
    let (secret, fingerprint) = code.trim().split_once('.').ok_or_else(wrong)?;
    let fingerprint = fingerprint.to_ascii_lowercase();
    if secret.is_empty() || fingerprint.len() != 64 || !fingerprint.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(wrong());
    }
    Ok((secret, fingerprint))
}

/// A certificate's fingerprint: the SHA-256 of its DER, in lower-case hex.
pub fn fingerprint(der: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, der)
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// A new key (PEM, PKCS#8) and a CSR for it (PEM). The CSR's names don't matter: the tracker sets the identity.
pub fn key_and_csr() -> Result<(String, String), String> {
    let key = KeyPair::generate().map_err(|e| format!("Couldn't make a key: {e}"))?;
    let mut params = CertificateParams::default();
    params.distinguished_name = DistinguishedName::new();
    let csr = params
        .serialize_request(&key)
        .and_then(|csr| csr.pem())
        .map_err(|e| format!("Couldn't make a certificate request: {e}"))?;
    Ok((key.serialize_pem(), csr))
}

/// The CA certificate (PEM) of an issued chain: everything after its first certificate.
pub fn ca_of(chain_pem: &str) -> Result<String, String> {
    let second = chain_pem
        .match_indices(BEGIN)
        .nth(1)
        .map(|(at, _)| at)
        .ok_or("The tracker's certificate came without its CA")?;
    Ok(chain_pem[second..].to_owned())
}

/// The certificates of a PEM text.
pub fn certificates(pem: &str) -> Result<Vec<CertificateDer<'static>>, String> {
    let certificates = CertificateDer::pem_slice_iter(pem.as_bytes())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Unreadable certificate: {e}"))?;
    if certificates.is_empty() {
        return Err("No certificate".into());
    }
    Ok(certificates)
}

/// A certificate as PEM.
pub fn pem(der: &[u8]) -> String {
    let mut out = format!("{BEGIN}\n");
    for line in base64(der).as_bytes().chunks(64) {
        out.push_str(&String::from_utf8_lossy(line));
        out.push('\n');
    }
    out.push_str("-----END CERTIFICATE-----\n");
    out
}

fn provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// A client configuration presenting the pet's certificate (`chain_pem`, its key `key_pem`) and trusting only the
/// tracker's CA.
pub fn client_config(ca_pem: &str, chain_pem: &str, key_pem: &str) -> Result<Arc<ClientConfig>, String> {
    let mut roots = RootCertStore::empty();
    for ca in certificates(ca_pem)? {
        roots.add(ca).map_err(|e| format!("Unusable CA certificate: {e}"))?;
    }
    let key = PrivateKeyDer::from_pem_slice(key_pem.as_bytes()).map_err(|e| format!("Unreadable key: {e}"))?;
    let config = ClientConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_root_certificates(roots)
        .with_client_auth_cert(certificates(chain_pem)?, key)
        .map_err(|e| format!("Unusable certificate: {e}"))?;
    Ok(Arc::new(config))
}

/// A TCP connection to `host:port`: each of its addresses in turn (`localhost` may be `::1` first and the server
/// on 127.0.0.1 only), the last one's error when none answers.
pub fn connect_tcp(host: &str, port: u16, timeout: Duration) -> io::Result<TcpStream> {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    let mut last = io::Error::new(io::ErrorKind::NotFound, "no address");
    for address in (host, port).to_socket_addrs()? {
        match TcpStream::connect_timeout(&address, timeout) {
            Ok(tcp) => return Ok(tcp),
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// A TLS stream to `host:port` with `config`, ready for a request.
pub fn connect(
    host: &str,
    port: u16,
    config: Arc<ClientConfig>,
    timeout: Duration,
) -> io::Result<rustls::StreamOwned<ClientConnection, TcpStream>> {
    let mut tcp = connect_tcp(host, port, timeout)?;
    tcp.set_read_timeout(Some(timeout))?;
    tcp.set_write_timeout(Some(timeout))?;
    let name = ServerName::try_from(host.trim_start_matches('[').trim_end_matches(']').to_owned())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    let mut connection = ClientConnection::new(config, name).map_err(io::Error::other)?;
    while connection.is_handshaking() {
        connection.complete_io(&mut tcp)?;
    }
    Ok(rustls::StreamOwned::new(connection, tcp))
}

/// The tracker's CA certificate (PEM), from a first connection that trusts the server only when its chain holds
/// the CA with `fingerprint` and that CA issued its certificate for `host`.
pub fn pin_ca(host: &str, port: u16, fingerprint: &str, timeout: Duration) -> Result<String, String> {
    let pinned = Arc::new(PinnedCa {
        fingerprint: fingerprint.to_owned(),
        provider: provider(),
        found: Mutex::default(),
    });
    let config = ClientConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .dangerous()
        .with_custom_certificate_verifier(Arc::clone(&pinned) as Arc<dyn ServerCertVerifier>)
        .with_no_client_auth();
    connect(host, port, Arc::new(config), timeout)
        .map_err(|e| format!("can't reach the tracker at {host}:{port}: {e}"))?;
    let found = lock(&pinned.found).take();
    found
        .map(|ca| pem(&ca))
        .ok_or_else(|| "The tracker sent no CA certificate".into())
}

/// The first connection's server verifier: the CA by the code's fingerprint, then the usual checks against it.
#[derive(Debug)]
struct PinnedCa {
    fingerprint: String,
    provider: Arc<CryptoProvider>,
    found: Mutex<Option<Vec<u8>>>,
}

impl ServerCertVerifier for PinnedCa {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let ca = intermediates
            .iter()
            .find(|certificate| fingerprint(certificate) == self.fingerprint)
            .ok_or_else(|| rustls::Error::General("The tracker's CA isn't the one the code names".into()))?;
        let mut roots = RootCertStore::empty();
        roots.add(ca.clone().into_owned())?;
        let verified = WebPkiServerVerifier::builder_with_provider(Arc::new(roots), Arc::clone(&self.provider))
            .build()
            .map_err(|e| rustls::Error::General(e.to_string()))?
            .verify_server_cert(end_entity, intermediates, server_name, ocsp_response, now)?;
        *lock(&self.found) = Some(ca.to_vec());
        Ok(verified)
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            certificate,
            signature,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            certificate,
            signature,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hosted_code_names_its_ca() {
        let fp = "ab".repeat(32);
        assert_eq!(
            split_code(&format!(" s3cret.{} ", fp.to_uppercase())),
            Ok(("s3cret", fp.clone()))
        );
        for bad in [
            "s3cret".to_owned(),
            "s3cret.abc".to_owned(),
            format!(".{fp}"),
            format!("s3cret.{}", "zz".repeat(32)),
        ] {
            assert!(split_code(&bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_key_and_its_request_and_pem_round_trip() {
        let (key, csr) = key_and_csr().unwrap();
        assert!(key.starts_with("-----BEGIN PRIVATE KEY-----"), "{key}");
        assert!(csr.starts_with("-----BEGIN CERTIFICATE REQUEST-----"), "{csr}");
        let key = KeyPair::from_pem(&key).unwrap();
        let ca = CertificateParams::new(vec!["ca".into()])
            .unwrap()
            .self_signed(&key)
            .unwrap();
        let text = pem(ca.der());
        assert_eq!(certificates(&text).unwrap(), [ca.der().clone()]);
        assert_eq!(fingerprint(ca.der()).len(), 64);
        let chain = format!("{text}{text}");
        assert_eq!(ca_of(&chain).unwrap(), text);
        assert!(ca_of(&text).is_err());
    }
}
