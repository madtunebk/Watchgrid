You are working on Watchgrid, a self-hosted NVR written in Rust.

STACK / HARD CONSTRAINTS

- Rust
- Tokio
- Axum
- PostgreSQL / SQLx
- Rust/WASM frontend
- ZERO Node.js
- ZERO npm / yarn / pnpm / bun
- No React / Vue / Angular / TypeScript build pipeline
- Cargo-first build and dependency management
- Keep dependencies intentional and minimal
- Prefer small focused crates/modules over large frameworks
- Security-sensitive code must be explicit and auditable
- Do not introduce Redis unless there is a demonstrated need
- Do not introduce external identity infrastructure
- Do not redesign unrelated Watchgrid functionality

IMPORTANT:
This work is infrastructure for Watchgrid, not a standalone enterprise
identity platform.

Keep it small, modular, testable, and suitable for a self-hosted NVR.


============================================================
1. AUTHENTICATION MODEL
============================================================

Watchgrid user management is CLI ONLY.

The web interface must NEVER provide:

- user registration
- user creation
- user deletion
- password reset
- forgot-password
- account administration
- bootstrap/setup user creation
- POST /api/users

Administrative authority comes from the machine administrator/root,
not from the browser.

Required CLI interface:

    sudo watchgrid user create <username>
    sudo watchgrid user list
    sudo watchgrid user passwd <username>
    sudo watchgrid user enable <username>
    sudo watchgrid user disable <username>
    sudo watchgrid user delete <username>

Passwords MUST be entered interactively using a hidden prompt.

Never accept passwords through:

- command-line arguments
- environment variables
- URLs
- logs

Use Argon2id for password hashing.

Sensitive password buffers should be zeroized when practical.

If Watchgrid has no users, the web UI should simply report something
similar to:

    No Watchgrid users exist.

    Create one from the server:

        sudo watchgrid user create <username>

Do NOT create a web-based setup wizard.


============================================================
2. WEB AUTH API
============================================================

The browser only authenticates existing users.

Keep the initial API approximately:

    POST /api/auth/login
    POST /api/auth/logout
    GET  /api/auth/session

Prefer secure server-side sessions.

Cookie requirements:

- HttpOnly
- SameSite
- Secure when HTTPS is enabled
- appropriate expiration
- session rotation after authentication

Do not expose password hashes or sensitive authentication state to the
WASM frontend.

Authorization must always be enforced by the backend.

Hiding UI controls in WASM is NOT authorization.


============================================================
3. AUTHORIZATION
============================================================

Design authorization so roles can be added without rewriting auth.

Possible initial roles:

    admin
    operator
    viewer

Do not over-engineer RBAC yet.

The backend remains authoritative.

Example future distinctions:

admin:
    camera configuration
    storage configuration
    system settings

operator:
    live view
    recordings
    events
    manual recording

viewer:
    restricted live/playback access

Keep authorization policy separate from authentication.


============================================================
4. CACHE / EPHEMERAL STATE
============================================================

Watchgrid needs a lightweight in-process cache inspired by the
architecture of TierCache.

Do NOT run Python TierCache from Watchgrid.

Implement the useful concepts natively in Rust.

Possible module/crate:

    coaba_cache

The cache is intended for EPHEMERAL data such as:

- current camera ONLINE/OFFLINE state
- current RTSP health
- latest camera snapshot
- latest motion score
- latest AI/inference result
- current bitrate
- current FPS
- reconnect state
- temporary thumbnails
- temporary ONVIF state
- Live View metadata

These values generally should NOT generate continuous PostgreSQL writes.

Desired cache features:

- async-safe
- TTL
- optional tags/namespaces
- bounded memory
- predictable eviction
- cheap concurrent reads
- explicit invalidation
- optional hot/cold tiers only if useful

Avoid creating a giant generic distributed cache framework.

Start with the smallest implementation Watchgrid actually needs.


============================================================
5. PERSISTENT DATA BOUNDARY
============================================================

PostgreSQL remains the source of truth for durable state.

Examples:

    cameras
    users
    sessions if persisted
    events
    recordings
    tasks
    export jobs
    recording metadata
    rules
    audit/history

Do NOT persist every frame-level observation.

For example, instead of continuously writing:

    motion_score = 0.00
    motion_score = 0.01
    motion_score = 0.00
    ...

persist meaningful transitions/events:

    MotionStarted
    MotionEnded
    PersonDetected
    CameraOnline
    CameraOffline
    RecordingStarted
    RecordingFinalized

Think:

    high-frequency state -> cache
    meaningful history   -> PostgreSQL


============================================================
6. EVENT BUS INTERACTION
============================================================

Cache, database and Watchgrid's event bus must have clear responsibilities.

Use Tokio primitives where appropriate:

    broadcast
    mpsc
    watch

Do NOT introduce Redis just to move events inside one Watchgrid process.

Example:

RTSP Supervisor
      |
      +---- update current camera state -> cache
      |
      +---- state transition -----------> Event Bus
                                           |
                                           +-> Recorder
                                           +-> Rules
                                           +-> Notifications
                                           +-> DB persistence

A camera's current bitrate may live only in cache.

A CameraOffline transition may become a durable event in PostgreSQL.


============================================================
7. OAUTH / EXTERNAL PROVIDER CREDENTIALS
============================================================

Watchgrid will eventually support external storage/providers such as:

- Google Drive
- Dropbox
- S3 / MinIO
- Nextcloud
- other future providers

OAuth is NOT Watchgrid's primary login mechanism.

OAuth here is primarily for connecting Watchgrid to external services.

Design a provider-neutral credential abstraction.

For example:

    trait CredentialStore {
        async fn put(...);
        async fn get(...);
        async fn delete(...);
    }

or an equivalent clean Rust API.

Conceptually:

    CredentialStore
        provider
        account
        secret/token
        metadata

OAuth refresh tokens MUST remain server-side.

Never expose provider refresh tokens to the browser after authorization
has completed.


============================================================
8. ENCRYPTED CREDENTIAL STORAGE
============================================================

External provider secrets should be encrypted at rest.

Use established cryptographic primitives.

Possible design:

    Argon2id / stable machine secret / administrator secret
                        |
                        v
                  encryption key
                        |
                        v
                 AEAD encryption
                        |
                        v
                  encrypted secret

Use a standard AEAD construction supported by a mature Rust crate.

Do NOT invent cryptography.

Credential encryption should support future key rotation.

Do not permanently bind encrypted credentials to an unstable machine
fingerprint that would make backup/restore impossible.

Document backup/migration implications.


============================================================
9. EXISTING PROJECTS AS DESIGN REFERENCES
============================================================

There are existing projects from the same developer that may provide
useful patterns:

- TierCache
- OauthRS
- TempRS CredentialStore

Treat these as donors of ideas/patterns.

DO NOT blindly import the entire projects into Watchgrid.

Extract only concepts/code that fit Watchgrid cleanly.

Possible eventual boundaries:

    coaba_auth
    coaba_credentials
    coaba_cache
    coaba_db

But do not create crates merely for architectural aesthetics.
A module is sufficient until separation provides real value.


============================================================
10. SECURITY BOUNDARIES
============================================================

Maintain these boundaries:

Browser/WASM
    |
    | authenticated API
    v
Axum
    |
    +---- Auth / Authorization
    |
    +---- Watchgrid services
    |
    +---- Cache
    |
    +---- Event Bus
    |
    +---- PostgreSQL
    |
    +---- CredentialStore
              |
              +---- encrypted OAuth/API credentials

The browser must never have direct access to:

- PostgreSQL
- password hashes
- encryption master keys
- OAuth refresh tokens
- camera credentials unless explicitly required by a secure API operation
- filesystem secrets


============================================================
11. DO NOT DERAIL THE CURRENT WATCHGRID ROADMAP
============================================================

This is extremely important.

Watchgrid's immediate priority remains:

    Camera CRUD + PostgreSQL
        ->
    RTSP Supervisor
        ->
    Main/Sub stream handling
        ->
    Live View
        ->
    Manual Recording
        ->
    Recording metadata
        ->
    Playback
        ->
    Event Bus
        ->
    Motion / ONVIF
        ->
    Events

Authentication/cache/credential infrastructure must NOT become a large
side project before the basic NVR vertical slice works.

The critical vertical slice remains:

    Camera config
        -> PostgreSQL
        -> RTSP
        -> Live
        -> Manual Record
        -> file
        -> metadata
        -> Recordings
        -> Playback

Implement only the infrastructure required by the current milestone.

Fancy auth, OAuth providers, encrypted vault functionality and advanced
cache tiers can remain interfaces/stubs until needed.


============================================================
12. IMPLEMENTATION STYLE
============================================================

Before changing code:

1. Inspect the existing Watchgrid repository.
2. Identify current modules/crates and conventions.
3. Do not duplicate functionality that already exists.
4. Propose the smallest integration points.
5. Keep existing UI behavior intact.
6. Preserve mock mode while real services are introduced.
7. Avoid giant files and generic utils/helpers dumping grounds.
8. Add focused tests around security-sensitive behavior.

When implementing a feature:

    implement
    -> compile
    -> test
    -> verify behavior
    -> report exact files changed

Do not perform unrelated refactors.


============================================================
TARGET ARCHITECTURE
============================================================

Eventually the relationship should look roughly like:

                   ┌──────────────┐
                   │ Rust / WASM  │
                   │     UI       │
                   └──────┬───────┘
                          │
                          │ session
                          ▼
                   ┌──────────────┐
                   │    Axum      │
                   └──────┬───────┘
                          │
             ┌────────────┼────────────┐
             │            │            │
             ▼            ▼            ▼
          Auth        Watchgrid      OAuth
                       Services      Providers
                          │            │
                ┌─────────┼──────┐     ▼
                │         │      │ CredentialStore
                ▼         ▼      ▼
              Cache    Event   PostgreSQL
                        Bus
                         │
              ┌──────────┼───────────┐
              ▼          ▼           ▼
           Recorder    Rules    Notifications


FINAL RULE:

Do not build an enterprise IAM system.

Build boring, secure infrastructure that disappears into the background
and lets Watchgrid remain an NVR.
