---
satisfies: [R6]
---
# fnt-81-tracker-task-bubbles-in-the-pet-fnt-64.6 Hosted tracker: CSR enrolment and mutual TLS

## Description
Enrol on a hosted tracker's mutual-TLS listener with a CSR (rcgen), store cert and key in AiPet:Tracker, call every route with the client certificate (ureq + rustls client auth, tungstenite the same). Revoked certificate handled as revocation.

**Size:** M

## Acceptance
- [ ] enrol with a CSR against a fake mTLS tracker
- [ ] calls and the feed use the certificate
- [ ] a refused certificate shows as revocation

## Done summary
Hosted trackers on branch fnt-81-tracker (5b1bf6a), following the tracker's net::enroll::client. tracker/tls.rs: split_code (<secret>.<64-hex fingerprint>), fingerprint (SHA-256 hex of the DER, ring), key_and_csr (rcgen P-256, empty DN), pin_ca (rustls handshake with a PinnedCa verifier: the CA by fingerprint among the server's intermediates, then WebPkiServerVerifier against it for the host; returns the CA PEM), ca_of/certificates/pem, client_config (CA as only root + client cert), connect/connect_tcp (each resolved address in turn; found because localhost resolved to ::1 first). http.rs Http::tracker(ca, client cert, timeout): ureq with RootCerts::Specific and ClientCert, https only. Watcher: base() accepts https://host[:port] (default 443; no path, query or userinfo) and keeps http loopback-only ("A tracker on another computer is reached over https:// (its mutual-TLS listener)"); enroll on https pins the CA, posts {code, role, csr} through Http::tracker(ca), checks the returned chain's CA fingerprint against the code's, stores cert and key in AiPet:Tracker (serde default for older credentials). Credential::http()/bearer(): hosted calls carry the certificate and no Authorization header; the feed is wss over a FeedStream (plain TcpStream or rustls StreamOwned), read timeout on the inner TCP. Revocation: 401/403 as on loopback. Settings › Tracker hints mention both kinds. Tests: tls unit tests (code shapes, key/CSR/PEM round trip); tests/tracker_tls.rs against a fake hosted tracker (own CA, server cert for localhost, rustls server with optional client auth, signs the pet's CSR by reading its SPKI): wrong-CA code refused before anything is sent, a code without fingerprint refused, enrol stores a 2-cert chain and a PKCS#8 key, connected with its snapshot over mutual TLS, no bearer header ever, revoked certificate → Revoked. rcgen/ring/rustls direct deps; notices 382 crates; hakari unchanged and verified. Workspace clippy clean; aipet-core, aipet-ui, aipet-ipc and aipet tests pass. Not verified against a real hosted tracker (task 7's hands-on).

stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: 5b1bf6ad378e8170a3b9f94cbe6926d5660e74ac
- Tests: cargo test -p aipet-core --test tracker_tls; cargo test -p aipet-core -p aipet-ui -p aipet-ipc
- PRs: