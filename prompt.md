You are building a new lightweight, modern Network Video Recorder (NVR).

The project will eventually have a Rust backend for RTSP cameras, event-driven recording,
motion detection, playback, storage management, ONVIF events, and related functionality.

HOWEVER:

DO NOT IMPLEMENT THE REAL NVR BACKEND YET.

The first development milestone is exclusively the WEB GUI/UX and its frontend architecture.

The GUI must be fully navigable and visually complete using MOCK DATA and MOCK API calls.

The purpose of this phase is to design the product properly before implementing the
recording engine.

============================================================
PROJECT PHILOSOPHY
============================================================

This project exists because many existing NVR applications have powerful recording engines
but terrible administration interfaces.

The NVR should feel like a modern application where everything can be configured from the
browser.

The user must NEVER need to:

- stop the NVR daemon to add a camera
- edit configuration files manually
- use a terminal UI to configure cameras
- restart the application for ordinary configuration changes
- configure unnecessary substreams just to enable recording
- SSH into the server for normal administration

Eventually, the entire application should be manageable through the web interface.

The software is intended to run on lightweight Linux/NAS systems such as Synology NAS,
ordinary Linux servers, and small home servers.

The eventual backend will be written in Rust.

============================================================
CORE RECORDING PHILOSOPHY
============================================================

This is NOT primarily a 24/7 continuous recorder.

Recording must be configurable PER CAMERA.

Supported modes in the future:

1. Disabled
2. Manual recording
3. Motion/event recording
4. Continuous recording (optional)
5. Scheduled recording (future)
6. External/API triggered recording (future)

The preferred/default use case is EVENT-DRIVEN RECORDING.

For example:

Camera connected
      |
      +---- Live View
      |
      +---- Event Engine
               |
               +---- Motion
               +---- ONVIF event
               +---- Manual Record
               +---- API event
               |
               v
             Recorder
               |
               v
            Storage

The recorder should eventually support configurable pre-record and post-record buffers.

Example:

[ 5 sec pre-record ][ MOTION EVENT ][ 15 sec post-record ]

This allows an event recording to include video from BEFORE motion was detected.

DO NOT implement this engine during the GUI phase.

The GUI should simply expose these settings using mock state.

============================================================
PHASE 1 — APPLICATION SHELL
============================================================

Build the basic frontend application.

Create a modern dark NVR dashboard.

Main navigation:

Dashboard
Cameras
Live View
Events
Recordings
Storage
System
Settings

The layout should include:

- collapsible left sidebar
- top navigation/header
- server/NVR status
- responsive content area
- notification area
- connection indicator
- optional user menu

Visual direction:

Modern
Dark
Professional
Compact
Security/NVR oriented
Desktop-first but responsive
No excessive gradients
No giant rounded mobile-style components
No Bootstrap-looking admin template
No visual clutter

Think modern monitoring/control software rather than a generic SaaS dashboard.

============================================================
PHASE 2 — DASHBOARD
============================================================

Create the main dashboard using MOCK DATA.

Dashboard cards:

Cameras
Online cameras
Offline cameras
Active recordings
Events today
Storage usage
Server status

Example:

Cameras             5
Online              4
Offline             1
Recording           2
Events Today       17

Add a camera overview grid.

Example camera card:

------------------------------------------------
Front Door                         ONLINE

              [ CAMERA PREVIEW ]

192.168.1.26

Motion Detection                    ON
Recording Mode                   EVENTS

Last event:
Person detected - 2 minutes ago

[ LIVE ] [ RECORD ] [ ... ]
------------------------------------------------

Use placeholder images/video areas during this phase.

The RECORD button should visually toggle mock recording state.

============================================================
PHASE 3 — CAMERAS PAGE
============================================================

This is one of the most important interfaces.

Create a Cameras page showing every configured camera.

Support two presentation modes:

GRID
LIST

Each camera should display:

Camera name
Status
IP/hostname
RTSP status
Recording status
Recording mode
Motion status
Last event
Storage usage if available

Actions:

Live
Record Now / Stop Recording
Edit
Disable
Delete

All actions operate on mock state for now.

============================================================
PHASE 4 — ADD CAMERA
============================================================

Adding a camera MUST happen entirely from the browser.

Create:

Cameras -> Add Camera

Form fields:

Camera Name

Example:
Front Door

Host/IP

Example:
192.168.1.26

Username

Password

Main RTSP URL

Example:
rtsp://192.168.1.26:554/stream1

Optional Substream RTSP URL

Example:
rtsp://192.168.1.26:554/stream2

IMPORTANT:

A substream is OPTIONAL.

The application must never require a substream merely to enable recording.

Additional fields:

Description
Camera location
ONVIF URL (optional)
ONVIF username/password (optional)

Provide buttons:

TEST CONNECTION
TEST MAIN STREAM
TEST SUBSTREAM
SAVE CAMERA
CANCEL

During Phase 1 these buttons should use mock responses.

Example:

Testing stream...

Connection successful

Codec: H.264
Resolution: 1920x1080
FPS: 25
Audio: AAC
Latency: 84 ms

Do not implement real RTSP testing yet.

============================================================
PHASE 5 — CAMERA DETAILS
============================================================

Clicking a camera opens:

/cameras/:id

Tabs:

Live
Recording
Motion
Events
Stream
Storage
Advanced

------------------------------------------------------------
LIVE
------------------------------------------------------------

Large live-video placeholder.

Controls:

Record Now
Stop Recording
Fullscreen
Mute
Snapshot

Display:

ONLINE
1080p
25 FPS
H.264
bitrate
connection uptime

------------------------------------------------------------
RECORDING
------------------------------------------------------------

Recording Mode:

( ) Disabled
( ) Events / Motion
( ) Continuous
( ) Scheduled

Default:

Events / Motion

Settings:

Pre-record:
[ 5 ] seconds

Post-record:
[ 15 ] seconds

Minimum event duration

Maximum clip duration

Event merging interval

Example:

If motion stops and starts again within 10 seconds,
keep the same recording instead of creating another clip.

------------------------------------------------------------
MOTION
------------------------------------------------------------

Motion Detection:

[ ON ]

Detection source:

Camera / ONVIF
Software Detection
Future AI Detection

Sensitivity:

[---------|----]
60%

Allow future support for detection zones.

Show a camera preview with conceptual detection-zone overlay.

Do not implement actual computer vision yet.

------------------------------------------------------------
STREAM
------------------------------------------------------------

Main stream

RTSP URL
Codec
Resolution
FPS
Bitrate

Substream

Optional.

Allow:

Use main stream for recording
Use substream for motion detection

The application must work without a substream.

============================================================
PHASE 6 — LIVE VIEW
============================================================

Create a surveillance-style live view.

Layouts:

1 camera
2x2
3x3
4x4

Camera tiles should support drag/drop positioning if practical.

Each tile displays:

Camera name
Online/offline indicator
Recording indicator
Motion indicator

Hover controls:

Fullscreen
Record
Snapshot
Mute

Use placeholders/mock video.

============================================================
PHASE 7 — EVENTS
============================================================

Create an Events page.

Events should be the primary way of navigating recordings.

Example:

Today

06:14     MOTION       Front Door        00:23
08:31     VEHICLE      Driveway          00:41
12:07     PERSON       Front Door        01:04
15:22     MANUAL       Backyard          03:17

Filters:

Camera
Event type
Date
Time
Duration

Event types:

Motion
Person
Vehicle
Animal
ONVIF
Manual
External/API

Some types are future functionality but should already exist in the UI model.

Clicking an event opens an Event Details view.

Show:

Video player placeholder
Camera
Timestamp
Duration
Event type
Detection information
Previous event
Next event

Actions:

Download
Delete
Protect
Open camera

============================================================
PHASE 8 — RECORDINGS
============================================================

Create a recordings browser.

Two views:

EVENT VIEW
TIMELINE VIEW

Timeline concept:

00:00 ---------------------------------------- 24:00

Use visual segments:

motion
manual
continuous
other events

Allow:

zoom
date navigation
camera selection

Clicking a segment should open the corresponding recording.

Use mock recordings.

============================================================
PHASE 9 — STORAGE
============================================================

Create Storage page.

Display:

Storage location

Example:
/volume1/nvr

Total capacity
Used
Available
NVR usage

Camera usage breakdown.

Example:

Front Door       14.2 GB
Backyard          8.7 GB
Garage            4.1 GB

Retention settings:

Delete recordings older than:
[ 14 ] days

Maximum storage usage:
[ 500 ] GB

Minimum free disk:
[ 50 ] GB

Protected recordings should never be automatically deleted.

Again:

UI only during this phase.

============================================================
PHASE 10 — SYSTEM PAGE
============================================================

Create system monitoring interface.

Display mock values for:

NVR uptime
CPU usage
RAM usage
Disk usage
Network throughput
Active RTSP connections
Active recordings
Connected cameras

Also create a simple recent-log viewer.

Example:

15:31:04 INFO  Front Door connected
15:31:07 INFO  Motion detected
15:31:07 INFO  Recording started
15:31:31 INFO  Motion ended
15:31:46 INFO  Recording stopped

============================================================
PHASE 11 — SETTINGS
============================================================

Sections:

General
Recording
Storage
Network
Authentication
Notifications
Advanced

General settings:

NVR name
Timezone
Language
Date/time format

Recording defaults:

Default recording mode
Pre-record seconds
Post-record seconds

Network:

HTTP bind address
HTTP port
Future HTTPS configuration

Authentication:

Enable authentication
Users
Sessions

Do NOT implement actual authentication yet unless required by the existing frontend architecture.

============================================================
MOCK DATA ARCHITECTURE
============================================================

Do NOT scatter fake values throughout UI components.

Create a proper mock API/service layer.

For example:

api/
    cameras
    recordings
    events
    system
    storage

The UI should consume interfaces that can later be connected to the Rust backend.

Conceptually:

getCameras()
getCamera(id)
createCamera()
updateCamera()
deleteCamera()

startRecording(cameraId)
stopRecording(cameraId)

getEvents()
getRecordings()

getSystemStatus()
getStorageStatus()

For Phase 1 these return mock data.

Later they will call:

/api/v1/...

This separation is VERY IMPORTANT.

Do not tightly couple UI components to mock data.

============================================================
DATA MODEL
============================================================

Design frontend types/interfaces for at least:

Camera

id
name
description
location
enabled
status
host
username
mainStream
subStream
recordingMode
recording
motionEnabled
motionSource
preRecordSeconds
postRecordSeconds
lastEvent

Stream

url
status
codec
width
height
fps
bitrate
audioCodec

Event

id
cameraId
type
startTime
endTime
duration
recordingId
thumbnail
protected

Recording

id
cameraId
startTime
endTime
duration
reason
fileSize
protected

StorageStatus

path
total
used
available
recordingsSize

SystemStatus

uptime
cpuUsage
memoryUsage
networkRx
networkTx
activeStreams
activeRecordings

Design these cleanly because the future Rust API will use equivalent structures.

============================================================
IMPORTANT UX REQUIREMENTS
============================================================

Normal administration must eventually require ZERO terminal access.

Camera configuration must be possible while the NVR is running.

Recording settings must be changeable while running.

Adding/removing cameras must not require daemon restart.

Each camera is independent.

A camera may have only one RTSP stream.

Substreams are OPTIONAL.

Continuous recording is OPTIONAL.

Event recording is a first-class feature.

Manual recording can be started/stopped directly from the UI.

The interface must clearly distinguish:

Camera Online
Live Stream Active
Motion Detected
Recording Active

These are NOT the same state.

For example:

ONLINE     green
RECORDING  red
MOTION     amber

Do not represent all of them using one generic "active" indicator.

============================================================
DO NOT IMPLEMENT YET
============================================================

During the frontend milestone DO NOT implement:

RTSP ingest
FFmpeg integration
GStreamer integration
video decoding
video transcoding
MP4 muxing
HLS generation
WebRTC
motion detection algorithms
ONVIF discovery
ONVIF event subscriptions
SQLite/database
filesystem recording
retention deletion
Rust backend
authentication backend
AI/object detection

Those belong to later milestones.

Do not waste time creating fake backend servers unless necessary for frontend development.

Use an in-memory/mock frontend service.

============================================================
DEVELOPMENT ORDER
============================================================

Do NOT attempt the entire application in one giant implementation.

Work incrementally.

STEP 1

Inspect the existing web project.

Understand:

- framework
- directory structure
- styling
- reusable components
- routing
- dependencies

DO NOT rewrite the project unnecessarily.

Preserve the existing project's visual identity where practical.

Then provide a short implementation plan.

STEP 2

Implement:

Application shell
Sidebar
Header
Routing
Responsive layout

STOP and verify that it builds.

STEP 3

Implement:

Dashboard
Mock camera cards
System summary
Storage summary

STOP and verify.

STEP 4

Implement:

Cameras list/grid
Add Camera
Edit Camera
Camera Details

STOP and verify.

STEP 5

Implement:

Live View

STOP and verify.

STEP 6

Implement:

Events
Event Details

STOP and verify.

STEP 7

Implement:

Recordings
Timeline

STOP and verify.

STEP 8

Implement:

Storage
System
Settings

STOP and verify.

STEP 9

Polish:

Responsive behavior
Loading states
Empty states
Error states
Confirmation dialogs
Toasts
Animations/transitions where useful
Accessibility
Consistent spacing
Consistent iconography

STEP 10

Review the entire frontend architecture and prepare it for the future Rust API.

============================================================
EMPTY STATES ARE IMPORTANT
============================================================

The application must look correct even when there are ZERO cameras.

For example, a new installation should show:

"No cameras configured"

with:

[ + ADD YOUR FIRST CAMERA ]

Do not show broken charts or meaningless zero-filled widgets.

Likewise:

No recordings yet
No events yet
Storage unavailable
Camera offline

must have intentional UI states.

============================================================
DESIGN QUALITY
============================================================

Do not treat this as a prototype where appearance does not matter.

The frontend itself IS the deliverable during this milestone.

Pay attention to:

spacing
typography
hierarchy
contrast
icons
hover states
selected states
status colors
forms
dialogs
tables
camera grids
timeline readability

Avoid excessive rounded cards everywhere.

Avoid huge padding that wastes screen space.

An NVR interface benefits from information density.

Camera video should receive the majority of available screen space.

============================================================
FUTURE BACKEND — CONTEXT ONLY
============================================================

Do NOT implement this yet.

The future backend will likely use Rust and have components conceptually similar to:

RTSP Manager
      |
      +---- Stream Session
      |
      +---- Event Bus
              |
              +---- Manual Trigger
              +---- Motion Trigger
              +---- ONVIF Trigger
              +---- Future AI Trigger
              |
              v
          Recording Manager
              |
              +---- Pre-record Ring Buffer
              +---- Event Recording
              +---- Continuous Recording
              |
              v
            Storage

Metadata will likely be stored in SQLite.

The web backend/API may use Axum/Tokio.

The final system should avoid decoding/re-encoding video when it is unnecessary.

For recording, streams should ideally be remuxed/copied directly when codec/container
compatibility permits.

But NONE of this should be implemented during the current frontend milestone.

============================================================
FINAL GOAL OF CURRENT WORK
============================================================

At the end of the current frontend milestone I should be able to open the application
in a browser and interact with it as though a real NVR backend existed.

I should be able to:

Add a fake camera
Edit it
Delete it
Open its details
Toggle recording
Enable motion recording
Change pre/post recording
Open Live View
Browse fake events
Open an event
Browse recordings
Navigate a timeline
Inspect storage
Inspect system status
Change settings

All of this should work through the mock API layer.

The UI state should behave realistically enough that we can evaluate and improve the
entire workflow BEFORE writing the Rust recording engine.

Most importantly:

DO NOT prematurely implement backend functionality.

Build the GUI carefully, in stages, keep it running after every stage, and structure
everything so that replacing the mock API with the future Rust HTTP/WebSocket API is
straightforward.
