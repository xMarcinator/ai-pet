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
TBD

## Evidence
- Commits:
- Tests:
- PRs:
