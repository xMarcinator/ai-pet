//! The tracker watcher against a fake hosted tracker's mutual-TLS listener (fnt-81 task 6): the CA pinned by the
//! code's fingerprint, enrolling with a CSR for a key made in the pet, every later call and the feed with the issued
//! certificate (and no bearer token), and a revoked certificate refused. The fake has a CA of its own and a server
//! certificate for `localhost`, as the tracker's `host` does; nothing leaves this computer.

use std::collections::HashSet;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use aipet_core::secrets::{MemorySecrets, SecretStore, TRACKER};
use aipet_core::tracker::tls::fingerprint;
use aipet_core::tracker::{Event, Status, TrackerWatcher};
use rcgen::{
    BasicConstraints, CertificateParams, IsCa, Issuer, KeyPair, PKCS_ECDSA_P256_SHA256, PublicKeyData,
    SignatureAlgorithm,
};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, CertificateSigningRequestDer, PrivateKeyDer};
use rustls::server::WebPkiClientVerifier;
use rustls::{RootCertStore, ServerConfig, ServerConnection, StreamOwned};
use serde_json::{Value, json};
use tungstenite::Message;

const PATIENCE: Duration = Duration::from_secs(15);

#[derive(Default)]
struct Fake {
    /// The fingerprints of the certificates issued, and of those revoked.
    issued: HashSet<String>,
    revoked: HashSet<String>,
    /// Whether any request carried an Authorization header.
    saw_bearer: bool,
    feeds: usize,
}

struct Tracker {
    base: String,
    ca_fingerprint: String,
    fake: Arc<Mutex<Fake>>,
}

/// The pet's public key in its CSR, for rcgen to sign (the CSR's SubjectPublicKeyInfo's BIT STRING).
struct CsrKey(Vec<u8>);

impl PublicKeyData for CsrKey {
    fn der_bytes(&self) -> &[u8] {
        &self.0
    }

    fn algorithm(&self) -> &'static SignatureAlgorithm {
        &PKCS_ECDSA_P256_SHA256
    }
}

/// One DER element: its tag, its content, and what follows it.
fn tlv(der: &[u8]) -> (u8, &[u8], &[u8]) {
    let (tag, first) = (der[0], der[1]);
    let (length, start) = if first < 0x80 {
        (first as usize, 2)
    } else {
        let n = (first & 0x7f) as usize;
        (der[2..2 + n].iter().fold(0, |l, b| l << 8 | *b as usize), 2 + n)
    };
    (tag, &der[start..start + length], &der[start + length..])
}

/// A CSR's public key: CertificationRequest { info { version, subject, spki { algorithm, key } }, .. }.
fn csr_key(csr: &[u8]) -> CsrKey {
    let (_, request, _) = tlv(csr);
    let (_, info, _) = tlv(request);
    let (_, _, rest) = tlv(info); // version
    let (_, _, rest) = tlv(rest); // subject
    let (_, spki, _) = tlv(rest);
    let (_, _, rest) = tlv(spki); // algorithm
    let (tag, bits, _) = tlv(rest);
    assert_eq!(tag, 0x03, "a BIT STRING");
    CsrKey(bits[1..].to_vec())
}

impl Tracker {
    fn start() -> Tracker {
        let ca_key = KeyPair::generate().unwrap();
        let mut ca_params = CertificateParams::new(Vec::new()).unwrap();
        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        let ca = ca_params.self_signed(&ca_key).unwrap();
        let issuer = Arc::new(Issuer::new(ca_params, ca_key));
        let server_key = KeyPair::generate().unwrap();
        let server = CertificateParams::new(vec!["localhost".into()])
            .unwrap()
            .signed_by(&server_key, &issuer)
            .unwrap();
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let mut roots = RootCertStore::empty();
        roots.add(ca.der().clone()).unwrap();
        let clients = WebPkiClientVerifier::builder_with_provider(Arc::new(roots), Arc::clone(&provider))
            .allow_unauthenticated()
            .build()
            .unwrap();
        let config = Arc::new(
            ServerConfig::builder_with_provider(provider)
                .with_safe_default_protocol_versions()
                .unwrap()
                .with_client_cert_verifier(clients)
                .with_single_cert(
                    vec![server.der().clone(), ca.der().clone()],
                    PrivateKeyDer::from_pem_slice(server_key.serialize_pem().as_bytes()).unwrap(),
                )
                .unwrap(),
        );
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("https://localhost:{}", listener.local_addr().unwrap().port());
        let fake = Arc::new(Mutex::new(Fake::default()));
        let ca_pem = ca.pem();
        let shared = Arc::clone(&fake);
        thread::spawn(move || {
            for tcp in listener.incoming().flatten() {
                let (fake, config, issuer, ca_pem) = (
                    Arc::clone(&shared),
                    Arc::clone(&config),
                    Arc::clone(&issuer),
                    ca_pem.clone(),
                );
                thread::spawn(move || {
                    let _ = serve(tcp, config, &fake, &issuer, &ca_pem);
                });
            }
        });
        Tracker {
            base,
            ca_fingerprint: fingerprint(ca.der()),
            fake,
        }
    }

    fn fake(&self) -> std::sync::MutexGuard<'_, Fake> {
        self.fake.lock().unwrap()
    }
}

/// A TLS stream whose first bytes, already read, are given back before the rest: for tungstenite to read the head.
struct Replay {
    head: io::Cursor<Vec<u8>>,
    tls: StreamOwned<ServerConnection, TcpStream>,
}

impl Read for Replay {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.head.read(buf)? {
            0 => self.tls.read(buf),
            n => Ok(n),
        }
    }
}

impl Write for Replay {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.tls.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.tls.flush()
    }
}

fn serve(
    mut tcp: TcpStream,
    config: Arc<ServerConfig>,
    fake: &Mutex<Fake>,
    issuer: &Issuer<'static, KeyPair>,
    ca_pem: &str,
) -> io::Result<()> {
    tcp.set_read_timeout(Some(PATIENCE))?;
    let mut connection = ServerConnection::new(config).map_err(io::Error::other)?;
    while connection.is_handshaking() {
        connection.complete_io(&mut tcp)?;
    }
    let client = connection
        .peer_certificates()
        .and_then(|chain| chain.first())
        .map(|leaf| fingerprint(leaf));
    let mut tls = StreamOwned::new(connection, tcp);
    let mut head = Vec::new();
    let mut byte = [0];
    while !head.ends_with(b"\r\n\r\n") {
        tls.read_exact(&mut byte)?;
        head.push(byte[0]);
    }
    let text = String::from_utf8_lossy(&head).into_owned();
    let mut words = text.split_whitespace();
    let (method, target) = (
        words.next().unwrap_or("").to_owned(),
        words.next().unwrap_or("").to_owned(),
    );
    let header = |name: &str| {
        text.lines().skip(1).find_map(|line| {
            let (k, v) = line.split_once(':')?;
            k.trim().eq_ignore_ascii_case(name).then(|| v.trim().to_owned())
        })
    };
    let mut fake_now = fake.lock().unwrap();
    fake_now.saw_bearer |= header("authorization").is_some();
    let known = client
        .as_ref()
        .is_some_and(|c| fake_now.issued.contains(c) && !fake_now.revoked.contains(c));
    drop(fake_now);
    if target.starts_with("/api/v1/companion/feed") && known {
        fake.lock().unwrap().feeds += 1;
        let replay = Replay {
            head: io::Cursor::new(head),
            tls,
        };
        let mut socket = tungstenite::accept(replay).map_err(io::Error::other)?;
        let snapshot = json!({"type": "snapshot", "feed": "f1", "seq": 1, "tasks": [{
            "task_id": "fnt-9-hosted", "short_id": "FNT-9", "title": "Over mutual TLS",
            "agents": {"runs": 0, "sessions": 0, "subagents": 0, "total": 0}, "tone": "normal",
            "last_change": {"at": "2026-11-10T10:00:00Z"}, "links": {"task": "fnt://task/FNT-9"}
        }]});
        socket
            .send(Message::text(snapshot.to_string()))
            .map_err(io::Error::other)?;
        // the feed stays open until the pet goes
        while socket.read().is_ok() {}
        return Ok(());
    }
    let length: usize = header("content-length").and_then(|n| n.parse().ok()).unwrap_or(0);
    let mut body = vec![0; length];
    tls.read_exact(&mut body)?;
    let (status, reply) = match (method.as_str(), target.as_str()) {
        ("POST", "/enroll") => {
            let request: Value = serde_json::from_slice(&body).unwrap_or_default();
            let code = format!(
                "good.{}",
                fingerprint(&CertificateDer::from_pem_slice(ca_pem.as_bytes()).unwrap())
            );
            match request["csr"].as_str() {
                Some(csr) if request["code"] == code && request["role"] == "companion" => {
                    let csr = CertificateSigningRequestDer::from_pem_slice(csr.as_bytes()).unwrap();
                    let leaf = CertificateParams::default().signed_by(&csr_key(&csr), issuer).unwrap();
                    fake.lock().unwrap().issued.insert(fingerprint(leaf.der()));
                    let chain = format!("{}{ca_pem}", leaf.pem());
                    (200, json!({"id": "pet-1", "token": "tok", "certificate": chain}))
                }
                Some(_) => (403, json!({"error": "code already used"})),
                None => (400, json!({"error": "A certificate signing request is required"})),
            }
        }
        _ if !known => (401, json!({"error": "Device credential no longer valid"})),
        ("GET", "/api/attach/hello") => (200, json!({"companion": {"actions_allowed": false}})),
        ("GET", path) if path.starts_with("/api/v1/runs/by-session/") => {
            (404, json!({"error": "no run has that session"}))
        }
        _ => (404, json!({"error": "not found"})),
    };
    let reply = reply.to_string();
    write!(
        tls,
        "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{reply}",
        reply.len()
    )?;
    tls.flush()?;
    tls.conn.send_close_notify();
    let _ = tls.flush();
    Ok(())
}

fn until(what: &str, holds: impl Fn() -> bool) {
    let deadline = Instant::now() + PATIENCE;
    while !holds() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_pet_enrolls_on_a_hosted_tracker_with_its_own_key_and_is_refused_once_revoked() {
    let tracker = Tracker::start();
    let dir = std::env::temp_dir().join(format!("aipet-tracker-tls-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let secrets = Arc::new(MemorySecrets::default());
    let (tx, _rx): (_, Receiver<Event>) = mpsc::channel();
    let watcher = TrackerWatcher::new(&dir, Arc::clone(&secrets) as Arc<dyn SecretStore>, tx);
    watcher.set_chats(|| vec!["claude:s1".into()]);

    // a code naming another CA: the first connection doesn't trust the server, and nothing is sent
    let other = format!("good.{}", "ab".repeat(32));
    let error = watcher.enroll(&tracker.base, &other).unwrap_err();
    assert!(error.contains("isn't the one the code names"), "{error}");
    assert!(tracker.fake().issued.is_empty());
    let error = watcher.enroll(&tracker.base, "good").unwrap_err();
    assert!(error.starts_with("A hosted tracker's code looks like"), "{error}");

    let code = format!("good.{}", tracker.ca_fingerprint);
    watcher.enroll(&tracker.base, &code).unwrap();
    let saved: Value = serde_json::from_str(&secrets.read(TRACKER).unwrap()).unwrap();
    assert_eq!(saved["base_url"], tracker.base);
    assert!(saved["cert"].as_str().unwrap().matches("BEGIN CERTIFICATE").count() == 2);
    assert!(
        saved["key"]
            .as_str()
            .unwrap()
            .starts_with("-----BEGIN PRIVATE KEY-----")
    );

    until("connected over mutual TLS", || {
        watcher.status() == Status::Connected { actions_allowed: false } && watcher.tasks().len() == 1
    });
    assert_eq!(watcher.tasks()[0].short_id, "FNT-9");
    assert_eq!(tracker.fake().feeds, 1);
    assert!(
        !tracker.fake().saw_bearer,
        "a hosted tracker gets the certificate, never the token"
    );

    // revoked in the tracker: the certificate is refused from then on
    let issued: Vec<String> = tracker.fake().issued.iter().cloned().collect();
    tracker.fake().revoked.extend(issued);
    watcher.restart();
    until("the refusal", || watcher.status() == Status::Revoked);
    assert!(watcher.tasks().is_empty());
    let _ = std::fs::remove_dir_all(dir);
}
