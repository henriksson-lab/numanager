# Example Outputs

These are recorded outputs from user-facing workflow examples. They should show
public runtime behavior only: discovered devices, typed properties, operation
completion, frame handles, stream status, and filtered events. Event ordering
can vary when runtime events are drained after asynchronous operations, so use
these blocks to check the public surface and representative output content, not
as byte-for-byte driver conformance tests. Do not use this file to document
serial bytes, HID reports, SDK calls, raw registers, or parser fixtures.

## Camera Acquisition

Command:

```sh
cargo run -p numanager-examples -- camera_acquisition
```

Recorded output excerpt:

```text
added driver with 1 device(s)
device arrived: toupcam-0
source: toupcam
camera: toupcam-0 ["camera", "trigger.sink"]
capture capability: CameraCapture request=CameraCapture (CameraCapture)
trigger capability: TriggerSink request=Trigger (TriggerSink)
property: exposure type=TimeInterval writable=true sequenceable=true
property: gain type=Ratio writable=true sequenceable=true
property: pixel_format type=String writable=true sequenceable=true
property: trigger_mode type=String writable=true sequenceable=false
property: roi_width type=PixelCount writable=true sequenceable=false
property: roi_height type=PixelCount writable=true sequenceable=false
property: binning type=I64 writable=true sequenceable=false
property: black_level type=I64 writable=true sequenceable=false
property: white_balance_red type=Ratio writable=true sequenceable=false
property: white_balance_blue type=Ratio writable=true sequenceable=false
property: sensor_temperature type=Temperature writable=false sequenceable=false
property: supported_pixel_formats type=List writable=false sequenceable=false
camera setup completed: map keys=[remuxed]
trigger mode completed: map keys=[remuxed]
trigger pulse completed: map keys=[trigger_mode, triggered]
timing arm completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
timing start completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
timing stop completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
capture completed: map keys=[frame, height, pixel_format, stream, width]
capture frame handle: FrameHandle { stream: StreamId(100), frame: FrameId(4) } size=Some(640)xSome(480) format=Some("Mono8")
device+operation-filtered status: OperationId(7) devices=[toupcam-0] -> running
device+operation-filtered status: OperationId(7) devices=[toupcam-0] -> completed map keys=[frame, height, pixel_format, stream, width]
frame: 640x480 307200 bytes format=Mono8 metadata keys=[binning, black_level, exposure, gain, source, trigger_mode, white_balance_blue, white_balance_red]
exposure readback: TimeInterval(TimeInterval { value: 0.025, unit: Seconds })
gain readback: Ratio(Ratio { value: 100.0, unit: Percent })
pixel_format readback: String("Mono8")
trigger_mode readback: String("external")
roi_width readback: PixelCount(PixelCount(640))
roi_height readback: PixelCount(PixelCount(480))
binning readback: I64(1)
black_level readback: I64(0)
white_balance_red readback: Ratio(Ratio { value: 100.0, unit: Percent })
white_balance_blue readback: Ratio(Ratio { value: 100.0, unit: Percent })
sensor_temperature readback: Temperature(Temperature { value: 28.0, unit: Celsius })
supported_pixel_formats readback: List([String("Raw8"), String("Mono8"), String("Mono16"), String("Rgb8")])
removed driver with 1 device(s)
```

Additional source variants use the same public workflow:

```sh
cargo run -p numanager-examples -- camera_acquisition platform
cargo run -p numanager-examples -- camera_acquisition gige
cargo run -p numanager-examples -- camera_acquisition usb3
cargo run -p numanager-examples -- camera_acquisition genicam
```

Recorded output excerpts:

```text
source: platform
camera: platform-camera-fixture ["camera", "platform.camera", "trigger.sink", "trigger.source"]
property: exposure type=TimeInterval writable=true sequenceable=true
property: gain type=Ratio writable=true sequenceable=true
property: pixel_format type=String writable=true sequenceable=true
property: width type=PixelCount writable=false sequenceable=false
property: height type=PixelCount writable=false sequenceable=false
camera setup completed: map keys=[exposure, frame_interval, gain, pixel_format]
trigger pulse completed: map keys=[action, backend, capability, triggered]
capture completed: map keys=[frame, frames, height, pixel_format, stream, width]
frame: 1280x720 921600 bytes format=Mono8 metadata keys=[backend, exposure, frame_interval, gain, pixel_format]
gain readback: Ratio(Ratio { value: 10.0, unit: Percent })
width readback: PixelCount(PixelCount(1280))
```

```text
source: gige
camera: gige-vision-camera-0 ["camera", "gige.vision", "genicam.transport", "trigger.sink", "trigger.source"]
property: width type=PixelCount writable=true sequenceable=true
property: height type=PixelCount writable=true sequenceable=true
property: exposure type=TimeInterval writable=true sequenceable=true
property: gain type=Decibel writable=true sequenceable=true
property: pixel_format type=String writable=true sequenceable=true
capture completed: map keys=[frame, frames, height, pixel_format, stream, transport, width]
frame: 640x480 307200 bytes format=Mono8 metadata keys=[chunk_frame_id, chunk_metadata, exposure, gain, gvsp_status, hardware_timestamp, inter_packet_delay, packet_size, stream_channel_port]
```

```text
source: usb3
camera: usb3-vision-camera-0 ["camera", "usb3.vision", "genicam.transport", "trigger.sink", "trigger.source"]
property: width type=PixelCount writable=true sequenceable=true
property: height type=PixelCount writable=true sequenceable=true
property: exposure type=TimeInterval writable=true sequenceable=true
property: gain type=Decibel writable=true sequenceable=true
property: pixel_format type=String writable=true sequenceable=true
capture completed: map keys=[frame, frames, height, pixel_format, stream, transport, width]
frame: 640x480 307200 bytes format=Mono8 metadata keys=[chunk_frame_id, chunk_metadata, exposure, gain, hardware_timestamp, stream_endpoint, transfer_queue_depth, transfer_size]
```

```text
source: genicam
camera: genicam-local-camera ["camera", "genicam", "genicam.node_map", "fixture"]
capture capability: CameraCapture request=CameraCapture (GenICamCapture)
trigger capability: TriggerSink request=Trigger (GenICamAcquisitionTriggerSink)
capture completed: map keys=[frame, height, pixel_format, stream, width]
frame: 1024x768 786432 bytes format=Mono8 metadata keys=[chunk_frame_id, chunk_metadata, exposure, hardware_timestamp, payload_size, source]
```

## Runtime Config Round-Trip

Command:

```sh
cargo run -p numanager-examples -- config_roundtrip
```

Recorded output:

```text
serialized config:
[[resources]]
id = 100
label = "Camera USB transport"
driver = "toupcam"
param.frame_interval = "20 ms"
param.transfer_size = "1048576 bytes"

[[devices]]
id = 101
label = "Main camera"
driver = "toupcam"
property.exposure = "12.5 ms"
property.gain = "120 percent"
property.pixel_format = "Mono8"
property.roi_height = "768 px"
property.roi_width = "1024 px"

[[devices]]
id = 102
label = "Z focus drive"
driver = "asi"
property.position = "2500 um"

[[remux_groups]]
name = "camera-and-focus-transport"
devices = [101, 102]
resource = 100

[[dependencies]]
from = 102
to = 101
role = "z_stage"

loaded resources: 1
resource Camera USB transport driver=toupcam params=["frame_interval", "transfer_size"]
  param.frame_interval: TimeInterval(TimeInterval { value: 20.0, unit: Milliseconds })
  param.transfer_size: ByteCount(ByteCount(1048576))
loaded devices: 2
device Main camera driver=toupcam properties=["exposure", "gain", "pixel_format", "roi_height", "roi_width"]
  property.exposure: TimeInterval(TimeInterval { value: 12.5, unit: Milliseconds })
  property.gain: Ratio(Ratio { value: 120.0, unit: Percent })
  property.pixel_format: String("Mono8")
  property.roi_height: PixelCount(PixelCount(768))
  property.roi_width: PixelCount(PixelCount(1024))
device Z focus drive driver=asi properties=["position"]
  property.position: Position(Position { value: 2500.0, unit: Micrometers })
loaded remux groups: 1
loaded dependencies: 1
```

## Software Test GUI

Command:

```sh
cargo run -p numanager-examples --features gui -- software_gui --smoke
```

Recorded output:

```text
software gui smoke
imagers:
  sim-microscope-camera [camera, simulator] stream=true
pan stages:
  sim-microscope-xy [stage.xy, axis.xy, simulator]
focus stages:
  sim-microscope-z [stage.z, axis.z, simulator]
objectives:
  sim-microscope-camera -> sim-microscope-objective [objective.turret, state.device, simulator]
optics: 0.325 um per image pixel, from sim-microscope-camera pixel pitch 6.5 um x binning 1 / sim-microscope-objective magnification 20
properties:
  sim-microscope.model = composed brightfield microscope simulation writable=false
  sim-microscope.sample_seed = 6840136679005487105 writable=false
  sim-microscope-camera.exposure = 0.02 s writable=true
  sim-microscope-camera.gain = 100 % writable=true
  sim-microscope-camera.frame_interval = 0.05 s writable=true
  sim-microscope-camera.binning = 1x1 writable=true
  sim-microscope-camera.pixel_pitch = 6.5 um writable=false
  sim-microscope-camera.sensor_width = 512 px writable=false
  sim-microscope-camera.sensor_height = 512 px writable=false
  sim-microscope-camera.sample_pixel_size = 0.325 um writable=false
  sim-microscope-camera.pixel_format = Mono8 writable=false
  sim-microscope-xy.x = 0 um writable=true
  sim-microscope-xy.y = 0 um writable=true
  sim-microscope-xy.speed = 2500 um/s writable=true
  sim-microscope-xy.busy = false writable=false
  sim-microscope-z.z = 4250 um writable=true
  sim-microscope-z.speed = 400 um/s writable=true
  sim-microscope-z.busy = false writable=false
  sim-microscope-objective.position = 2 writable=true
  sim-microscope-objective.magnification = 20 writable=false
  sim-microscope-objective.numerical_aperture = 0.45 writable=false
  sim-microscope-objective.busy = false writable=false
  sim-microscope-lamp.enabled = true writable=true
  sim-microscope-lamp.power = 100 % writable=true
  sim-microscope-lamp.interlock_closed = true writable=false
  sim-microscope-lamp.fault = No Fault writable=false
safety:
  sim-microscope-lamp = active enabled=true  interlock=true  fault=No Fault
objective 4x / 0.13 NA air: 1.625 um per image pixel, from sim-microscope-camera pixel pitch 6.5 um x binning 1 / sim-microscope-objective magnification 4
objective 20x / 0.45 NA air: 0.325 um per image pixel, from sim-microscope-camera pixel pitch 6.5 um x binning 1 / sim-microscope-objective magnification 20
capture: map keys=[frame, height, pixel_format, sample_pixel_size, stream, width] frame=FrameHandle { stream: StreamId(1852), frame: FrameId(0) } 512x512 Mono8 histogram_bins=256
stream: map keys=[frames, height, pixel_format, stream, width] frames=12 frame=FrameHandle { stream: StreamId(47), frame: FrameId(11) } 512x512 Mono8 histogram_bins=256
stream status: depth=8 capacity=8 dropped=4 latest=Some(FrameHandle { stream: StreamId(47), frame: FrameId(11) }) map keys=[capacity, depth, dropped_frames, overflow_policy, retained_frames, stream]
```

## Camera Stream

Command:

```sh
cargo run -p numanager-examples -- camera_stream
```

Recorded output:

```text
source: toupcam
camera: toupcam-0 ["camera", "trigger.sink"]
stream capability: CameraStream request=CameraStream (CameraStream)
stream setup completed: map keys=[remuxed]
drop_oldest stream completed: map keys=[frame, frames, height, pixel_format, stream, width] stream=StreamId(2) frames=Some(6) size=640x480 format=Some("Mono8")
drop_oldest frame FrameHandle { stream: StreamId(2), frame: FrameId(0) }: not retained by ring buffer metadata keys=[dropped_frames, exposure, gain, overflow_policy, ring_capacity, ring_depth, source, trigger_mode]
drop_oldest frame FrameHandle { stream: StreamId(2), frame: FrameId(1) }: not retained by ring buffer metadata keys=[dropped_frames, exposure, gain, overflow_policy, ring_capacity, ring_depth, source, trigger_mode]
drop_oldest frame FrameHandle { stream: StreamId(2), frame: FrameId(2) }: 640x480 307200 bytes metadata keys=[dropped_frames, exposure, gain, overflow_policy, ring_capacity, ring_depth, source, trigger_mode]
drop_oldest frame FrameHandle { stream: StreamId(2), frame: FrameId(3) }: 640x480 307200 bytes metadata keys=[dropped_frames, exposure, gain, overflow_policy, ring_capacity, ring_depth, source, trigger_mode]
drop_oldest frame FrameHandle { stream: StreamId(2), frame: FrameId(4) }: 640x480 307200 bytes metadata keys=[dropped_frames, exposure, gain, overflow_policy, ring_capacity, ring_depth, source, trigger_mode]
drop_oldest stream telemetry: keys=[dropped_frames, overflow_policy, ring_capacity, ring_depth, stream]
drop_oldest frame FrameHandle { stream: StreamId(2), frame: FrameId(5) }: 640x480 307200 bytes metadata keys=[dropped_frames, exposure, gain, overflow_policy, ring_capacity, ring_depth, source, trigger_mode]
drop_oldest stream telemetry: keys=[dropped_frames, overflow_policy, ring_capacity, ring_depth, stream]
drop_oldest received 6 frame-ready event(s), 2 drop telemetry event(s), and 0 fault event(s)
drop_oldest stream status: depth=4 capacity=4 dropped=2 latest=Some(FrameHandle { stream: StreamId(2), frame: FrameId(5) }) map keys=[capacity, depth, dropped_frames, overflow_policy, retained_frames, stream]
drop_newest stream completed: map keys=[frame, frames, height, pixel_format, stream, width] stream=StreamId(3) frames=Some(6) size=640x480 format=Some("Mono8")
drop_newest frame FrameHandle { stream: StreamId(3), frame: FrameId(0) }: 640x480 307200 bytes metadata keys=[dropped_frames, exposure, gain, overflow_policy, ring_capacity, ring_depth, source, trigger_mode]
drop_newest frame FrameHandle { stream: StreamId(3), frame: FrameId(1) }: 640x480 307200 bytes metadata keys=[dropped_frames, exposure, gain, overflow_policy, ring_capacity, ring_depth, source, trigger_mode]
drop_newest frame FrameHandle { stream: StreamId(3), frame: FrameId(2) }: 640x480 307200 bytes metadata keys=[dropped_frames, exposure, gain, overflow_policy, ring_capacity, ring_depth, source, trigger_mode]
drop_newest frame FrameHandle { stream: StreamId(3), frame: FrameId(3) }: 640x480 307200 bytes metadata keys=[dropped_frames, exposure, gain, overflow_policy, ring_capacity, ring_depth, source, trigger_mode]
drop_newest frame FrameHandle { stream: StreamId(3), frame: FrameId(4) }: not retained by ring buffer metadata keys=[dropped_frames, exposure, gain, overflow_policy, ring_capacity, ring_depth, source, trigger_mode]
drop_newest stream telemetry: keys=[dropped_frames, overflow_policy, ring_capacity, ring_depth, stream]
drop_newest frame FrameHandle { stream: StreamId(3), frame: FrameId(5) }: not retained by ring buffer metadata keys=[dropped_frames, exposure, gain, overflow_policy, ring_capacity, ring_depth, source, trigger_mode]
drop_newest stream telemetry: keys=[dropped_frames, overflow_policy, ring_capacity, ring_depth, stream]
drop_newest received 6 frame-ready event(s), 2 drop telemetry event(s), and 0 fault event(s)
drop_newest stream status: depth=4 capacity=4 dropped=2 latest=Some(FrameHandle { stream: StreamId(3), frame: FrameId(3) }) map keys=[capacity, depth, dropped_frames, overflow_policy, retained_frames, stream]
error stream completed: map keys=[frame, frames, height, pixel_format, stream, width] stream=StreamId(4) frames=Some(6) size=640x480 format=Some("Mono8")
error frame FrameHandle { stream: StreamId(4), frame: FrameId(0) }: 640x480 307200 bytes metadata keys=[dropped_frames, exposure, gain, overflow_policy, ring_capacity, ring_depth, source, trigger_mode]
error frame FrameHandle { stream: StreamId(4), frame: FrameId(1) }: 640x480 307200 bytes metadata keys=[dropped_frames, exposure, gain, overflow_policy, ring_capacity, ring_depth, source, trigger_mode]
error frame FrameHandle { stream: StreamId(4), frame: FrameId(2) }: 640x480 307200 bytes metadata keys=[dropped_frames, exposure, gain, overflow_policy, ring_capacity, ring_depth, source, trigger_mode]
error frame FrameHandle { stream: StreamId(4), frame: FrameId(3) }: 640x480 307200 bytes metadata keys=[dropped_frames, exposure, gain, overflow_policy, ring_capacity, ring_depth, source, trigger_mode]
error received 4 frame-ready event(s), 0 drop telemetry event(s), and 0 fault event(s)
error stream status: depth=4 capacity=4 dropped=0 latest=Some(FrameHandle { stream: StreamId(4), frame: FrameId(3) }) map keys=[capacity, depth, dropped_frames, overflow_policy, retained_frames, stream]
```

Additional source variants use the same high-throughput ring-buffer workflow:

```sh
cargo run -p numanager-examples -- camera_stream platform
cargo run -p numanager-examples -- camera_stream gige
cargo run -p numanager-examples -- camera_stream usb3
cargo run -p numanager-examples -- camera_stream genicam
```

Recorded output excerpts:

```text
source: platform
camera: platform-camera-fixture ["camera", "platform.camera", "trigger.sink", "trigger.source"]
stream setup completed: map keys=[exposure, pixel_format]
drop_oldest stream completed: map keys=[frame, frames, height, pixel_format, stream, width] stream=StreamId(2) frames=Some(6) size=1280x720 format=Some("Mono8")
drop_oldest frame FrameHandle { stream: StreamId(2), frame: FrameId(0) }: not retained by ring buffer metadata keys=[backend, dropped_frames, exposure, frame_interval, gain, overflow_policy, pixel_format, ring_capacity, ring_depth]
drop_oldest stream telemetry: keys=[dropped_frames, overflow_policy, ring_capacity, ring_depth, stream]
drop_oldest received 6 frame-ready event(s), 2 drop telemetry event(s), and 0 fault event(s)
drop_oldest stream status: depth=4 capacity=4 dropped=2 latest=Some(FrameHandle { stream: StreamId(2), frame: FrameId(5) }) map keys=[capacity, depth, dropped_frames, overflow_policy, retained_frames, stream]
```

```text
source: gige
camera: gige-vision-camera-0 ["camera", "gige.vision", "genicam.transport", "trigger.sink", "trigger.source"]
drop_oldest stream completed: map keys=[frame, frames, height, pixel_format, stream, transport, width] stream=StreamId(2) frames=Some(6) size=640x480 format=Some("Mono8")
drop_oldest frame FrameHandle { stream: StreamId(2), frame: FrameId(0) }: not retained by ring buffer metadata keys=[chunk_frame_id, chunk_metadata, dropped_frames, exposure, gain, gvsp_status, hardware_timestamp, inter_packet_delay, overflow_policy, packet_size, ring_capacity, ring_depth, stream_channel_port]
drop_oldest received 6 frame-ready event(s), 2 drop telemetry event(s), and 0 fault event(s)
drop_oldest stream status: depth=4 capacity=4 dropped=2 latest=Some(FrameHandle { stream: StreamId(2), frame: FrameId(5) }) map keys=[capacity, depth, dropped_frames, overflow_policy, retained_frames, stream]
```

```text
source: usb3
camera: usb3-vision-camera-0 ["camera", "usb3.vision", "genicam.transport", "trigger.sink", "trigger.source"]
drop_oldest stream completed: map keys=[frame, frames, height, pixel_format, stream, transport, width] stream=StreamId(2) frames=Some(6) size=640x480 format=Some("Mono8")
drop_oldest frame FrameHandle { stream: StreamId(2), frame: FrameId(0) }: not retained by ring buffer metadata keys=[chunk_frame_id, chunk_metadata, dropped_frames, exposure, gain, hardware_timestamp, overflow_policy, ring_capacity, ring_depth, stream_endpoint, transfer_queue_depth, transfer_size]
drop_oldest received 6 frame-ready event(s), 2 drop telemetry event(s), and 0 fault event(s)
drop_oldest stream status: depth=4 capacity=4 dropped=2 latest=Some(FrameHandle { stream: StreamId(2), frame: FrameId(5) }) map keys=[capacity, depth, dropped_frames, overflow_policy, retained_frames, stream]
```

```text
source: genicam
camera: genicam-local-camera ["camera", "genicam", "genicam.node_map", "fixture"]
drop_oldest stream completed: map keys=[frame_count, height, pixel_format, stream, width] stream=StreamId(1) frames=Some(6) size=1024x768 format=Some("Mono8")
drop_oldest frame FrameHandle { stream: StreamId(1), frame: FrameId(0) }: not retained by ring buffer metadata keys=[chunk_frame_id, chunk_metadata, dropped_frames, exposure, hardware_timestamp, overflow_policy, payload_size, ring_capacity, ring_depth, source]
drop_oldest received 6 frame-ready event(s), 2 drop telemetry event(s), and 0 fault event(s)
drop_oldest stream status: depth=4 capacity=4 dropped=2 latest=Some(FrameHandle { stream: StreamId(1), frame: FrameId(5) }) map keys=[capacity, depth, dropped_frames, overflow_policy, retained_frames, stream]
```

## Timing Plan

Command:

```sh
cargo run -p numanager-examples -- timing_plan
```

Recorded output:

```text
participants:
  platform-camera-v4l2 [camera, platform.camera, trigger.sink, trigger.source]
  asi-ms2000-xy [axis.xy, stage.xy]
  asi-ms2000-z [axis.z, stage.z]
  coolled-pe300-hub [hub, light.engine, shutter]
  coolled-pe300-channel-1 [light.source, led.channel, trigger.sink]
camera setup: map keys=[exposure, frame_interval, pixel_format]
stage setup: map keys=[x, y, z]
light setup: map keys=[enabled, intensity, selected]
armed timing plan: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
light timing after arm: map keys=[armed, route_count, routed_channels, running, sequence_count, starts, stops]
started timing plan: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
light timing after start: map keys=[armed, route_count, routed_channels, running, sequence_count, starts, stops]
camera stream: map keys=[frame, frames, height, pixel_format, stream, width] stream=StreamId(2) frames=Some(4) format=Some("Mono8")
stopped timing plan: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
light timing after stop: map keys=[armed, route_count, routed_channels, running, sequence_count, starts, stops]
event: operation on [platform-camera-v4l2] running
event: operation on [platform-camera-v4l2] completed map keys=[exposure, frame_interval, pixel_format]
event: operation on [asi-ms2000-xy, asi-ms2000-z] running
event: operation on [asi-ms2000-xy, asi-ms2000-z] completed map keys=[x, y, z]
event: operation on [coolled-pe300-channel-1, coolled-pe300-hub] running
event: operation on [coolled-pe300-channel-1, coolled-pe300-hub] completed map keys=[enabled, intensity, selected]
event: operation on [platform-camera-v4l2, coolled-pe300-channel-1, asi-ms2000-xy, coolled-pe300-hub, asi-ms2000-z] running
event: operation on [platform-camera-v4l2, coolled-pe300-channel-1, asi-ms2000-xy, coolled-pe300-hub, asi-ms2000-z] completed map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
event: operation on [coolled-pe300-hub] running
event: operation on [coolled-pe300-hub] completed map keys=[armed, route_count, routed_channels, running, sequence_count, starts, stops]
event: operation on [platform-camera-v4l2, coolled-pe300-channel-1, asi-ms2000-xy, coolled-pe300-hub, asi-ms2000-z] running
event: operation on [platform-camera-v4l2, coolled-pe300-channel-1, asi-ms2000-xy, coolled-pe300-hub, asi-ms2000-z] completed map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
event: operation on [coolled-pe300-hub] running
event: operation on [coolled-pe300-hub] completed map keys=[armed, route_count, routed_channels, running, sequence_count, starts, stops]
event: operation on [platform-camera-v4l2] running
event: frame ready from platform-camera-v4l2 1280x720 Mono8
event: frame ready from platform-camera-v4l2 1280x720 Mono8
event: frame ready from platform-camera-v4l2 1280x720 Mono8
event: frame ready from platform-camera-v4l2 1280x720 Mono8
event: operation on [platform-camera-v4l2] completed map keys=[frame, frames, height, pixel_format, stream, width]
event: operation on [platform-camera-v4l2, coolled-pe300-channel-1, asi-ms2000-xy, coolled-pe300-hub, asi-ms2000-z] running
event: operation on [platform-camera-v4l2, coolled-pe300-channel-1, asi-ms2000-xy, coolled-pe300-hub, asi-ms2000-z] completed map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
event: operation on [coolled-pe300-hub] running
event: operation on [coolled-pe300-hub] completed map keys=[armed, route_count, routed_channels, running, sequence_count, starts, stops]
```

## NI-DAQmx External Gates Audit

Command:

```sh
scripts/audit-ni-daqmx-external-gates.sh
```

Recorded output excerpt:

```text
# NI-DAQmx External Gates Audit

| Gate | Status |
| --- | --- |
| License and redistribution review remains explicit | ok |
| Installed Windows package/license review remains explicit | ok |
| Installed Linux/Windows 26.5 header audit remains explicit | ok |
| NI-PAL/device inventory and runtime publication need bench evidence | ok |
| Bench safety preconditions remain explicit before execute helpers | ok |
| Live NI-DAQmx task execution remains unexposed pending hardware validation | ok |

This audit checks that non-code external gates for the NI-DAQmx backend remain documented and visible. It does not complete legal review, audit installed Windows headers, initialize NI-PAL, approve bench wiring/safety, create NI-DAQmx tasks, or provide hardware validation evidence.

```

## NI-DAQmx Target Scope Audit

Command:

```sh
scripts/audit-ni-daqmx-target-scope.sh
```

Recorded output excerpt:

```text
# NI-DAQmx Target Scope Audit

| Boundary | Status |
| --- | --- |
| `ni-daqmx-sys` dependency target-scoped to Linux/Windows | ok |
| `ni-daqmx-sdk` feature maps only to optional `ni-daqmx-sys` | ok |
| Helper binaries require `ni-daqmx-sdk` | ok |
| Helper wrappers use Linux/Windows implementation cfgs | ok |
| Helper wrappers provide unsupported-target failure stubs | ok |
| Helper wrappers do not reference NI-DAQmx FFI directly | ok |
| Helper implementation files contain NI-DAQmx FFI references | ok |
| Runtime readiness reports Linux/Windows target support boundary | ok |

This audit checks numanager source boundaries only. It does not prove Windows ABI compatibility, NI-DAQmx runtime installation, task behavior, or hardware behavior.

```

## NI-DAQmx No-Hardware Helper Audit

Command:

```sh
scripts/audit-ni-daqmx-no-hardware-helpers.sh
```

Recorded output excerpt:

```text
# NI-DAQmx No-Hardware Helper Audit

| Workflow | Status |
| --- | --- |
| SDK-feature helper build | ok |
| Task lifecycle dry run | ok |
| Task lifecycle cleanup simulation | ok |
| Raster/signal plan preflight | ok |
| Plan setup cleanup simulation | ok |
| Channel setup dry runs | ok |
| I/O smoke dry runs | ok |
| I/O cleanup simulation | ok |
| Invalid numeric/range/transfer/raster/signal guards | ok |

This audit runs only helper build, dry-run, preflight-only, simulated-cleanup, and invalid-input paths. It does not execute NI-DAQmx tasks, write outputs, read inputs, or provide hardware evidence.

```

## NI-DAQmx Plan Validation Audit

Command:

```sh
scripts/audit-ni-daqmx-plan-validation.sh
```

Recorded output excerpt:

```text
# NI-DAQmx Plan Validation Audit

| Workflow | Status |
| --- | --- |
| Valid raster plan keeps helper commands runnable | ok |
| Valid signal plan keeps helper commands runnable | ok |
| Invalid raster role plan suppresses helper commands | ok |
| Invalid signal channel plan suppresses helper commands | ok |
| Execution gate remains non-live | ok |

This audit runs the public `lsm_daqmx_plan_validation` example and checks configured plan-validation markers plus helper-command suppression for invalid plans. It does not create NI-DAQmx tasks, write outputs, read inputs, execute scans, or provide hardware evidence.

```

## NI-DAQmx Live Gate Audit

Command:

```sh
scripts/audit-ni-daqmx-live-gate.sh
```

Recorded output excerpt:

```text
# NI-DAQmx Live Gate Audit

| Workflow | Status |
| --- | --- |
| Confocal capture live intent remains gated | ok |
| Confocal stream live intent remains gated | ok |
| Scan-signal stream live intent remains gated | ok |
| LSM GUI ImSwitch live intent remains gated and simulator controls stay inactive | ok |

This audit sets `NUMANAGER_DAQMX_LIVE_TASK_EXECUTION=1` and verifies that public ImSwitch DAQmx APIs record live-task intent while still reporting `live_task_execution_ready=false` and `execution=not_live_task_execution`. The GUI smoke path must not emit simulator-only scene, objective, or detector control writebacks for the configured ImSwitch source. It does not create NI-DAQmx tasks, write outputs, read inputs, publish hardware frames, or provide hardware evidence.

```

## NI-DAQmx Runtime Probe Audit

Command:

```sh
scripts/audit-ni-daqmx-runtime-probe.sh
```

Recorded output excerpt:

```text
# NI-DAQmx Runtime Probe Audit

| Workflow | Status |
| --- | --- |
| Config-only readiness probe avoids runtime loading | ok |
| Configured package-version metadata parses without runtime loading | ok |
| Process-isolated runtime-version probe remains probe-only | ok |
| Configured runtime-version mismatch or partial detection blocks live execution | ok |
| Live-task intent with metadata and runtime probe reaches hardware-validation blocker | ok |
| Process-isolated inventory probe remains evidence-only | ok |
| Compact inventory readiness summaries are emitted | ok |

This audit runs public `daqmx_runtime_probe` workflows through the optional SDK feature. The config-only paths avoid loading the vendor runtime. The isolated probe may load NI-DAQmx in the helper process, but the runtime process stays in `runtime_probe_only` and keeps `live_task_execution_ready=false`, even when the helper reports a contained runtime-version failure. When package/header metadata, runtime probing, and live-task intent are all present, the blocker advances only to `pending_hardware_validation`. It does not create NI-DAQmx tasks, write outputs, read inputs, execute scans, or provide hardware evidence.

```

## LSM DAQmx Bring-Up Plan

Command:

```sh
cargo run -p numanager-examples -- lsm_daqmx_bringup_plan
```

Recorded output excerpt:

```text
source: imswitch
hub: Dev1-imswitch-daqmx-hub
capture_api: ConfocalImageCapture request=ConfocalImageCapture
signal_api: ScanSignalStream request=ScanSignalStream
backend_readiness: execution=not_live_backend; live_ready=false; live_requested=false; blocker=feature_ni_daqmx_sdk; runtime_version=not_configured(matches=unknown,basis=configured_runtime_version_missing); missing=runtime_package+runtime_version+runtime_platform+runtime_license+sdk_header_path+sdk_header_sha256+feature_ni_daqmx_sdk+api_audit_and_hardware_validation; promotion_gate_statuses=[pending=9]
capture_plan: execution=not_live_task_execution, blocker=feature_ni_daqmx_sdk, readiness=[ready=false;blocker=feature_ni_daqmx_sdk;missing=runtime_package+runtime_version+runtime_platform+runtime_license+sdk_header_path+sdk_header_sha256+feature_ni_daqmx_sdk+api_audit_and_hardware_validation;hardware=pending], routing=pending_hardware_validation, roles=[x_galvo=Dev1/ao0,y_galvo=Dev1/ao1,laser_gate=Dev1/port0/line0,detector=Dev1/ctr0,sample_clock=Dev1/ctr2], clock=/Dev1/Ctr2InternalOutput, buffers=[scan=512x512x1:262144 samples;tasks=ao_scan:write:f64_volts:2chx262144|do_laser_gate:write:u8_line_state:1chx262144|ci_detector:read:u32_counts:1chx262144|co_sample_clock:generate:counter_pulse_train:1chx262144], cleanup_timeout_s=10.000, waveforms=[ao_scan:x_fast_sawtooth_y_slow_step:pending_hardware_validation|do_laser_gate:high_during_active_pixels:pending_hardware_validation], routes=[clock:/Dev1/Ctr2InternalOutput:co_sample_clock->ci_detector+ao_scan+do_laser_gate;trigger:none->ci_detector+ao_scan+do_laser_gate], sequence=[setup:ao_scan>do_laser_gate>ci_detector>co_sample_clock;write:ao_scan>do_laser_gate;start:ci_detector>ao_scan>do_laser_gate>co_sample_clock;read:ci_detector;wait:co_sample_clock;stop:co_sample_clock>do_laser_gate>ao_scan>ci_detector;clear:co_sample_clock>ci_detector>do_laser_gate>ao_scan], completion=[mode=finite;samples=262144;timeout_s=10.000;evidence=pending_hardware_validation], contract=[mode=raster_finite;write=ao_scan>do_laser_gate;read=ci_detector;wait=co_sample_clock;auto_start=false;timeout_s=10.000;evidence=pending_hardware_validation], executor=[mode=raster_finite;status=not_enabled_pending_hardware_validation;backend=ni_daqmx_sdk_task_wrapper;phases=validate_readiness>setup>write>start>read>wait>publish>cleanup>clear;evidence=pending_hardware_validation], reconstruction=[mode=one_detector_sample_per_pixel;input=ci_detector;scan=512x512;recon=512x512;pixel_format=Mono16;evidence=pending_hardware_validation], publication=[FrameReady:final_reconstructed_frame:scan=512x512:recon=512x512:Mono16:pending_hardware_validation], cancel=[strategy=request_stop_then_clear_created_tasks;stop=co_sample_clock>do_laser_gate>ao_scan>ci_detector;clear=co_sample_clock>ci_detector>do_laser_gate>ao_scan;evidence=pending_hardware_validation], start=[ci_detector>ao_scan>do_laser_gate>co_sample_clock], read=[ci_detector], clear=[co_sample_clock>ci_detector>do_laser_gate>ao_scan], cleanup=stop_started_tasks_then_clear_all_created_tasks
signal_plan: execution=not_live_task_execution, blocker=feature_ni_daqmx_sdk, readiness=[ready=false;blocker=feature_ni_daqmx_sdk;missing=runtime_package+runtime_version+runtime_platform+runtime_license+sdk_header_path+sdk_header_sha256+feature_ni_daqmx_sdk+api_audit_and_hardware_validation;hardware=pending], routing=pending_hardware_validation, buffers=[signal=1024x1:1024 samples chunk=256;tasks=ci_signal:read:u32_counts:1chx1024|ai_signal:read:f64_volts:1chx1024], cleanup_timeout_s=10.000, routes=[clock:unspecified:none->ci_signal+ai_signal;trigger:none->ci_signal+ai_signal], sequence=[setup:ci_signal>ai_signal;start:ci_signal>ai_signal;read:ci_signal>ai_signal;stop:ai_signal>ci_signal;clear:ai_signal>ci_signal], completion=[mode=finite;samples=1024;timeout_s=10.000;evidence=pending_hardware_validation], contract=[mode=signal_finite;write=none;read=ci_signal>ai_signal;wait=none;auto_start=false;timeout_s=10.000;evidence=pending_hardware_validation], executor=[mode=signal_finite;status=not_enabled_pending_hardware_validation;backend=ni_daqmx_sdk_task_wrapper;phases=validate_readiness>setup>write>start>read>wait>publish>cleanup>clear;evidence=pending_hardware_validation], publication=[ScanSignalChunk:raw_signal_chunks:channels=2:chunk=256:pending_hardware_validation], cancel=[strategy=request_stop_then_clear_created_tasks;stop=ai_signal>ci_signal;clear=ai_signal>ci_signal;evidence=pending_hardware_validation], start=[ci_signal>ai_signal], read=[ci_signal,ai_signal], clear=[ai_signal>ci_signal], cleanup=stop_started_tasks_then_clear_all_created_tasks
bench_evidence_commands:
scripts/audit-ni-daqmx-external-gates.sh
scripts/audit-ni-daqmx-package-inputs.sh <installer-file-or-directory>
scripts/audit-ni-daqmx-sdk-headers.sh <header-file-or-directory>
scripts/audit-ni-daqmx-sys-source.sh <ni-daqmx-sys-repo>
scripts/audit-ni-daqmx-target-scope.sh
scripts/audit-ni-daqmx-no-hardware-helpers.sh
scripts/audit-ni-daqmx-plan-validation.sh
scripts/audit-ni-daqmx-live-gate.sh
scripts/audit-ni-daqmx-runtime-probe.sh
scripts/audit-ni-daqmx-example-output-sync.sh
bench_runtime_probe_commands:
NUMANAGER_DAQMX_CONFIG_ONLY=1 cargo run -p numanager-examples --features ni-daqmx-sdk -- daqmx_runtime_probe
cargo run -p numanager-examples --features ni-daqmx-sdk -- daqmx_runtime_probe
bench_helper_build_commands:
cargo build -p numanager-imswitch-daqmx --features ni-daqmx-sdk --bins
bench_inventory_commands:
target/debug/numanager-daqmx-inventory-helper --device Dev1 --version-only
NUMANAGER_DAQMX_RUNTIME_HELPER=target/debug/numanager-daqmx-inventory-helper cargo run -p numanager-examples --features ni-daqmx-sdk -- daqmx_runtime_probe
target/debug/numanager-daqmx-inventory-helper --device Dev1
NUMANAGER_DAQMX_INVENTORY=1 NUMANAGER_DAQMX_INVENTORY_HELPER=target/debug/numanager-daqmx-inventory-helper cargo run -p numanager-examples --features ni-daqmx-sdk -- daqmx_runtime_probe
bench_preflight_commands:
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 262144 --width 512 --height 512 --frames 1 --ao-task ao_scan --do-task do_laser_gate --ci-task ci_detector --co-task co_sample_clock --ao Dev1/ao0 --ao Dev1/ao1 --do Dev1/port0/line0 --ci Dev1/ctr0 --co Dev1/ctr2 --min-volts -10.000000 --max-volts 10.000000 --sample-clock-source /Dev1/Ctr2InternalOutput --timeout 10.000000 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1024 --signal-lines 1 --chunk-size 256 --ci-task ci_signal --ai-task ai_signal --ci Dev1/ctr0 --ai Dev1/ai0 --timeout 10.000000 --preflight-only
bench_lifecycle_dry_run_commands:
target/debug/numanager-daqmx-task-lifecycle-helper --dry-run
target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --start --wait-seconds 0.250000
bench_lifecycle_cleanup_simulation_commands:
target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --start --wait-seconds 0.250000 --simulate-error-after-start
bench_plan_setup_cleanup_simulation_commands:
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 262144 --width 512 --height 512 --frames 1 --ao-task ao_scan --do-task do_laser_gate --ci-task ci_detector --co-task co_sample_clock --ao Dev1/ao0 --ao Dev1/ao1 --do Dev1/port0/line0 --ci Dev1/ctr0 --co Dev1/ctr2 --min-volts -10.000000 --max-volts 10.000000 --sample-clock-source /Dev1/Ctr2InternalOutput --timeout 10.000000 --preflight-only --simulate-setup-error-after 1
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1024 --signal-lines 1 --chunk-size 256 --ci-task ci_signal --ai-task ai_signal --ci Dev1/ctr0 --ai Dev1/ai0 --timeout 10.000000 --preflight-only --simulate-setup-error-after 1
bench_invalid_numeric_guard_commands:
target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --wait-seconds NaN
target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --name ''
target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --name ' lifecycle '
target/debug/numanager-daqmx-task-lifecycle-helper --simulate-error-after-start
target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --simulate-error-after-start
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate NaN --samples 1 --ci Dev1/ctr0 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 0 --samples 1 --ci Dev1/ctr0 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --timeout NaN --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --timeout 0 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --sample-clock-source '' --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --sample-clock-source ' /Dev1/Ctr0InternalOutput ' --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --start-trigger '' --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --start-trigger ' /Dev1/PFI0 ' --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci '' --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci ' Dev1/ctr0 ' --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --ci-task '' --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --ci-task ' signal ' --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --signal-lines 0 --ci Dev1/ctr0 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 5 --signal-lines 2 --ci Dev1/ctr0 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --chunk-size 1 --ci Dev1/ctr0 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --signal-lines 1 --chunk-size 2 --ci Dev1/ctr0 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --co Dev1/ctr0 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --ai Dev1/ai0 --ci-task signal --ai-task signal --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 0 --ci Dev1/ctr0 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 2147483648 --ci Dev1/ctr0 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 3 --width 2 --height 2 --frames 1 --ci Dev1/ctr0 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --width 1 --ci Dev1/ctr0 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --width 18446744073709551615 --height 2 --frames 1 --ci Dev1/ctr0 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --width 2 --height 2 --frames 4611686018427387904 --ci Dev1/ctr0 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --width 0 --height 1 --frames 1 --ci Dev1/ctr0 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --width 1 --height 0 --frames 1 --ci Dev1/ctr0 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --width 1 --height 1 --frames 0 --ci Dev1/ctr0 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1073741824 --ao Dev1/ao0 --ao Dev1/ao1 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ao Dev1/ao0 --min-volts NaN --max-volts 1 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ao Dev1/ao0 --min-volts 1 --max-volts -1 --preflight-only
target/debug/numanager-daqmx-channel-setup-helper --kind co --channel Dev1/ctr2 --name '' --dry-run
target/debug/numanager-daqmx-channel-setup-helper --kind co --channel Dev1/ctr2 --name ' channel-setup ' --dry-run
target/debug/numanager-daqmx-channel-setup-helper --kind co --channel '' --dry-run
target/debug/numanager-daqmx-channel-setup-helper --kind co --channel ' Dev1/ctr2 ' --dry-run
target/debug/numanager-daqmx-channel-setup-helper --kind co --channel Dev1/ctr2 --frequency inf --dry-run
target/debug/numanager-daqmx-channel-setup-helper --kind co --channel Dev1/ctr2 --frequency 0 --dry-run
target/debug/numanager-daqmx-channel-setup-helper --kind co --channel Dev1/ctr2 --duty-cycle NaN --dry-run
target/debug/numanager-daqmx-channel-setup-helper --kind co --channel Dev1/ctr2 --duty-cycle 1.5 --dry-run
target/debug/numanager-daqmx-io-smoke-helper --kind co --channel '' --samples 1
target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --name '' --samples 1
target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --name ' io-smoke ' --samples 1
target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --frequency inf --samples 1
target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --frequency 0 --samples 1
target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --duty-cycle NaN --samples 1
target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --duty-cycle 1.5 --samples 1
target/debug/numanager-daqmx-channel-setup-helper --kind ai --channel Dev1/ai0 --min-volts NaN --max-volts 1 --dry-run
target/debug/numanager-daqmx-channel-setup-helper --kind ai --channel Dev1/ai0 --min-volts 1 --max-volts -1 --dry-run
target/debug/numanager-daqmx-io-smoke-helper --kind ai --channel Dev1/ai0 --samples 1 --timeout NaN
target/debug/numanager-daqmx-io-smoke-helper --kind ai --channel Dev1/ai0 --samples 1 --timeout 0
target/debug/numanager-daqmx-io-smoke-helper --kind ai --channel Dev1/ai0 --samples 0
target/debug/numanager-daqmx-io-smoke-helper --kind ai --channel Dev1/ai0 --samples 2147483648
target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --min-volts NaN --max-volts 1 --volts 0
target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --min-volts 1 --max-volts -1 --volts 0
target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --volts NaN
target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --min-volts -1 --max-volts 1 --volts 2
target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --min-volts 1 --max-volts 5 --volts 2
target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --simulate-error-after-start
target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --volts 0 --execute
bench_channel_setup_dry_run_commands:
target/debug/numanager-daqmx-channel-setup-helper --kind ai --channel Dev1/ai0 --dry-run
target/debug/numanager-daqmx-channel-setup-helper --kind ao --channel Dev1/ao0 --dry-run
target/debug/numanager-daqmx-channel-setup-helper --kind ao --channel Dev1/ao1 --dry-run
target/debug/numanager-daqmx-channel-setup-helper --kind ci --channel Dev1/ctr0 --dry-run
target/debug/numanager-daqmx-channel-setup-helper --kind co --channel Dev1/ctr2 --dry-run
target/debug/numanager-daqmx-channel-setup-helper --kind do --channel Dev1/port0/line0 --dry-run
bench_setup_commands:
target/debug/numanager-daqmx-task-lifecycle-helper
bench_plan_setup_commands:
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 262144 --width 512 --height 512 --frames 1 --ao-task ao_scan --do-task do_laser_gate --ci-task ci_detector --co-task co_sample_clock --ao Dev1/ao0 --ao Dev1/ao1 --do Dev1/port0/line0 --ci Dev1/ctr0 --co Dev1/ctr2 --min-volts -10.000000 --max-volts 10.000000 --sample-clock-source /Dev1/Ctr2InternalOutput --timeout 10.000000
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1024 --signal-lines 1 --chunk-size 256 --ci-task ci_signal --ai-task ai_signal --ci Dev1/ctr0 --ai Dev1/ai0 --timeout 10.000000
bench_channel_setup_commands:
target/debug/numanager-daqmx-channel-setup-helper --kind ai --channel Dev1/ai0
target/debug/numanager-daqmx-channel-setup-helper --kind ao --channel Dev1/ao0
target/debug/numanager-daqmx-channel-setup-helper --kind ao --channel Dev1/ao1
target/debug/numanager-daqmx-channel-setup-helper --kind ci --channel Dev1/ctr0
target/debug/numanager-daqmx-channel-setup-helper --kind co --channel Dev1/ctr2
target/debug/numanager-daqmx-channel-setup-helper --kind do --channel Dev1/port0/line0
bench_io_smoke_dry_run_commands:
target/debug/numanager-daqmx-io-smoke-helper --kind ai --channel Dev1/ai0 --samples 1
target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --volts 0
target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao1 --volts 0
target/debug/numanager-daqmx-io-smoke-helper --kind ci --channel Dev1/ctr0 --samples 1
target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --frequency 10 --samples 1
target/debug/numanager-daqmx-io-smoke-helper --kind do --channel Dev1/port0/line0 --line-state false
bench_io_smoke_cleanup_simulation_commands:
target/debug/numanager-daqmx-io-smoke-helper --kind ai --channel Dev1/ai0 --samples 1 --simulate-error-after-start
target/debug/numanager-daqmx-io-smoke-helper --kind ci --channel Dev1/ctr0 --samples 1 --simulate-error-after-start
target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --frequency 10 --samples 1 --simulate-error-after-start
bench_io_smoke_execute_commands:
target/debug/numanager-daqmx-io-smoke-helper --kind ai --channel Dev1/ai0 --samples 1 --execute --bench-safety-reviewed
target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --volts 0 --execute --bench-safety-reviewed
target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao1 --volts 0 --execute --bench-safety-reviewed
target/debug/numanager-daqmx-io-smoke-helper --kind ci --channel Dev1/ctr0 --samples 1 --execute --bench-safety-reviewed
target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --frequency 10 --samples 1 --execute --bench-safety-reviewed
target/debug/numanager-daqmx-io-smoke-helper --kind do --channel Dev1/port0/line0 --line-state false --execute --bench-safety-reviewed
execution_gate: not_live_task_execution

```

## LSM DAQmx Plan Validation

Command:

```sh
cargo run -p numanager-examples -- lsm_daqmx_plan_validation
```

Recorded output excerpt:

```text
source: imswitch
hub: Dev1-imswitch-daqmx-hub
capture_api: ConfocalImageCapture request=ConfocalImageCapture
signal_api: ScanSignalStream request=ScanSignalStream
valid_raster_request: 256x256 with configured role channels
valid_raster_result: api_status=declared_not_live, completion_basis=configured_api_only, daqmx_task_plan=map(39 keys), evidence_status=pending_ni_daqmx_runtime_evidence, live_task_execution_blocker=feature_ni_daqmx_sdk, live_task_execution_readiness=map(17 keys), live_task_execution_ready=false, live_task_execution_requested=false, reconstruction_fields=5, result=final_image_pending, scan_fields=12
valid_raster_plan: execution=not_live_task_execution, blocker=feature_ni_daqmx_sdk, readiness=[ready=false;blocker=feature_ni_daqmx_sdk;missing=runtime_package+runtime_version+runtime_platform+runtime_license+sdk_header_path+sdk_header_sha256+feature_ni_daqmx_sdk+api_audit_and_hardware_validation;hardware=pending], routing=pending_hardware_validation, roles=[x_galvo=Dev1/ao0,y_galvo=Dev1/ao1,laser_gate=Dev1/port0/line0,detector=Dev1/ctr0,sample_clock=Dev1/ctr2], clock=/Dev1/Ctr2InternalOutput, buffers=[scan=256x256x1:65536 samples;tasks=ao_scan:write:f64_volts:2chx65536|do_laser_gate:write:u8_line_state:1chx65536|ci_detector:read:u32_counts:1chx65536|co_sample_clock:generate:counter_pulse_train:1chx65536], cleanup_timeout_s=10.000, waveforms=[ao_scan:x_fast_sawtooth_y_slow_step:pending_hardware_validation|do_laser_gate:high_during_active_pixels:pending_hardware_validation], routes=[clock:/Dev1/Ctr2InternalOutput:co_sample_clock->ci_detector+ao_scan+do_laser_gate;trigger:none->ci_detector+ao_scan+do_laser_gate], sequence=[setup:ao_scan>do_laser_gate>ci_detector>co_sample_clock;write:ao_scan>do_laser_gate;start:ci_detector>ao_scan>do_laser_gate>co_sample_clock;read:ci_detector;wait:co_sample_clock;stop:co_sample_clock>do_laser_gate>ao_scan>ci_detector;clear:co_sample_clock>ci_detector>do_laser_gate>ao_scan], completion=[mode=finite;samples=65536;timeout_s=10.000;evidence=pending_hardware_validation], contract=[mode=raster_finite;write=ao_scan>do_laser_gate;read=ci_detector;wait=co_sample_clock;auto_start=false;timeout_s=10.000;evidence=pending_hardware_validation], executor=[mode=raster_finite;status=not_enabled_pending_hardware_validation;backend=ni_daqmx_sdk_task_wrapper;phases=validate_readiness>setup>write>start>read>wait>publish>cleanup>clear;evidence=pending_hardware_validation], reconstruction=[mode=one_detector_sample_per_pixel;input=ci_detector;scan=256x256;recon=256x256;pixel_format=Mono16;evidence=pending_hardware_validation], publication=[FrameReady:final_reconstructed_frame:scan=256x256:recon=256x256:Mono16:pending_hardware_validation], cancel=[strategy=request_stop_then_clear_created_tasks;stop=co_sample_clock>do_laser_gate>ao_scan>ci_detector;clear=co_sample_clock>ci_detector>do_laser_gate>ao_scan;evidence=pending_hardware_validation], start=[ci_detector>ao_scan>do_laser_gate>co_sample_clock], read=[ci_detector], clear=[co_sample_clock>ci_detector>do_laser_gate>ao_scan], cleanup=stop_started_tasks_then_clear_all_created_tasks
valid_raster_validation: status=valid runnable=true recognized_tasks=4 unrecognized_count=0 invalid_role_count=0
valid_raster_helper_commands: setup=string preflight=string
valid_signal_request: one 512-sample line over configured channels, chunk_size=128
valid_signal_result: api_status=declared_not_live, channel_count=2, channel_names=list(2), chunk_size=128, completion_basis=configured_api_only, daqmx_task_plan=map(34 keys), evidence_status=pending_ni_daqmx_runtime_evidence, live_task_execution_blocker=feature_ni_daqmx_sdk, live_task_execution_readiness=map(17 keys), live_task_execution_ready=false, live_task_execution_requested=false, result=raw_signal_stream_pending, timing_fields=6
valid_signal_plan: execution=not_live_task_execution, blocker=feature_ni_daqmx_sdk, readiness=[ready=false;blocker=feature_ni_daqmx_sdk;missing=runtime_package+runtime_version+runtime_platform+runtime_license+sdk_header_path+sdk_header_sha256+feature_ni_daqmx_sdk+api_audit_and_hardware_validation;hardware=pending], routing=pending_hardware_validation, buffers=[signal=512x1:512 samples chunk=128;tasks=ci_signal:read:u32_counts:1chx512|ai_signal:read:f64_volts:1chx512], cleanup_timeout_s=10.000, routes=[clock:unspecified:none->ci_signal+ai_signal;trigger:none->ci_signal+ai_signal], sequence=[setup:ci_signal>ai_signal;start:ci_signal>ai_signal;read:ci_signal>ai_signal;stop:ai_signal>ci_signal;clear:ai_signal>ci_signal], completion=[mode=finite;samples=512;timeout_s=10.000;evidence=pending_hardware_validation], contract=[mode=signal_finite;write=none;read=ci_signal>ai_signal;wait=none;auto_start=false;timeout_s=10.000;evidence=pending_hardware_validation], executor=[mode=signal_finite;status=not_enabled_pending_hardware_validation;backend=ni_daqmx_sdk_task_wrapper;phases=validate_readiness>setup>write>start>read>wait>publish>cleanup>clear;evidence=pending_hardware_validation], publication=[ScanSignalChunk:raw_signal_chunks:channels=2:chunk=128:pending_hardware_validation], cancel=[strategy=request_stop_then_clear_created_tasks;stop=ai_signal>ci_signal;clear=ai_signal>ci_signal;evidence=pending_hardware_validation], start=[ci_signal>ai_signal], read=[ci_signal,ai_signal], clear=[ai_signal>ci_signal], cleanup=stop_started_tasks_then_clear_all_created_tasks
valid_signal_validation: status=valid runnable=true recognized_tasks=2 unrecognized_count=0 invalid_role_count=0
valid_signal_helper_commands: setup=string preflight=string
raster_request: 256x256 with x_galvo mapped to ai0
raster_result: api_status=declared_not_live, completion_basis=configured_api_only, daqmx_task_plan=map(39 keys), evidence_status=pending_ni_daqmx_runtime_evidence, live_task_execution_blocker=feature_ni_daqmx_sdk, live_task_execution_readiness=map(17 keys), live_task_execution_ready=false, live_task_execution_requested=false, reconstruction_fields=5, result=final_image_pending, scan_fields=12
raster_plan: execution=not_live_task_execution, blocker=feature_ni_daqmx_sdk, readiness=[ready=false;blocker=feature_ni_daqmx_sdk;missing=runtime_package+runtime_version+runtime_platform+runtime_license+sdk_header_path+sdk_header_sha256+feature_ni_daqmx_sdk+api_audit_and_hardware_validation;hardware=pending], validation=status=invalid_role_channels runnable=false invalid_roles=x_galvo:ai0, routing=pending_hardware_validation, roles=[x_galvo=Dev1/ai0,y_galvo=Dev1/ao1,laser_gate=Dev1/port0/line0,detector=Dev1/ctr0,sample_clock=Dev1/ctr2], clock=/Dev1/Ctr2InternalOutput, buffers=[scan=256x256x1:65536 samples;tasks=ao_scan:write:f64_volts:2chx65536|do_laser_gate:write:u8_line_state:1chx65536|ci_detector:read:u32_counts:1chx65536|co_sample_clock:generate:counter_pulse_train:1chx65536], cleanup_timeout_s=10.000, waveforms=[ao_scan:x_fast_sawtooth_y_slow_step:pending_hardware_validation|do_laser_gate:high_during_active_pixels:pending_hardware_validation], routes=[clock:/Dev1/Ctr2InternalOutput:co_sample_clock->ci_detector+ao_scan+do_laser_gate;trigger:none->ci_detector+ao_scan+do_laser_gate], sequence=[setup:ao_scan>do_laser_gate>ci_detector>co_sample_clock;write:ao_scan>do_laser_gate;start:ci_detector>ao_scan>do_laser_gate>co_sample_clock;read:ci_detector;wait:co_sample_clock;stop:co_sample_clock>do_laser_gate>ao_scan>ci_detector;clear:co_sample_clock>ci_detector>do_laser_gate>ao_scan], completion=[mode=finite;samples=65536;timeout_s=10.000;evidence=pending_hardware_validation], contract=[mode=raster_finite;write=ao_scan>do_laser_gate;read=ci_detector;wait=co_sample_clock;auto_start=false;timeout_s=10.000;evidence=pending_hardware_validation], executor=[mode=raster_finite;status=not_enabled_pending_hardware_validation;backend=ni_daqmx_sdk_task_wrapper;phases=validate_readiness>setup>write>start>read>wait>publish>cleanup>clear;evidence=pending_hardware_validation], reconstruction=[mode=one_detector_sample_per_pixel;input=ci_detector;scan=256x256;recon=256x256;pixel_format=Mono16;evidence=pending_hardware_validation], publication=[FrameReady:final_reconstructed_frame:scan=256x256:recon=256x256:Mono16:pending_hardware_validation], cancel=[strategy=request_stop_then_clear_created_tasks;stop=co_sample_clock>do_laser_gate>ao_scan>ci_detector;clear=co_sample_clock>ci_detector>do_laser_gate>ao_scan;evidence=pending_hardware_validation], start=[ci_detector>ao_scan>do_laser_gate>co_sample_clock], read=[ci_detector], clear=[co_sample_clock>ci_detector>do_laser_gate>ao_scan], cleanup=stop_started_tasks_then_clear_all_created_tasks
raster_helper_commands: setup=null preflight=null
raster_validation: status=invalid_role_channels runnable=false recognized_tasks=4 unrecognized_count=0 invalid_role_count=1 invalid_roles=x_galvo:ai0
signal_request: one 512-sample line over unsupported_detector, chunk_size=128
signal_result: api_status=declared_not_live, channel_count=1, channel_names=list(1), chunk_size=128, completion_basis=configured_api_only, daqmx_task_plan=map(34 keys), evidence_status=pending_ni_daqmx_runtime_evidence, live_task_execution_blocker=feature_ni_daqmx_sdk, live_task_execution_readiness=map(17 keys), live_task_execution_ready=false, live_task_execution_requested=false, result=raw_signal_stream_pending, timing_fields=6
signal_plan: execution=not_live_task_execution, blocker=feature_ni_daqmx_sdk, readiness=[ready=false;blocker=feature_ni_daqmx_sdk;missing=runtime_package+runtime_version+runtime_platform+runtime_license+sdk_header_path+sdk_header_sha256+feature_ni_daqmx_sdk+api_audit_and_hardware_validation;hardware=pending], validation=status=invalid_no_recognized_channels runnable=false unrecognized=unsupported_detector, routing=pending_hardware_validation, buffers=[signal=512x1:512 samples chunk=128], cleanup_timeout_s=10.000, routes=[clock:unspecified:none->none;trigger:none->none], sequence=[setup:none;start:none;stop:none;clear:none], completion=[mode=finite;samples=512;timeout_s=10.000;evidence=pending_hardware_validation], contract=[mode=signal_finite;write=none;read=none;wait=none;auto_start=false;timeout_s=10.000;evidence=pending_hardware_validation], executor=[mode=signal_finite;status=not_enabled_pending_hardware_validation;backend=ni_daqmx_sdk_task_wrapper;phases=validate_readiness>setup>write>start>read>wait>publish>cleanup>clear;evidence=pending_hardware_validation], publication=[ScanSignalChunk:raw_signal_chunks:channels=1:chunk=128:pending_hardware_validation], cancel=[strategy=request_stop_then_clear_created_tasks;stop=none;clear=none;evidence=pending_hardware_validation], start=[], read=[], clear=[], cleanup=stop_started_tasks_then_clear_all_created_tasks
signal_helper_commands: setup=null preflight=null
signal_validation: status=invalid_no_recognized_channels runnable=false recognized_tasks=0 unrecognized_count=1 invalid_role_count=0 unrecognized=unsupported_detector
execution_gate: not_live_task_execution

```

## LSM DAQmx Validation Note Scaffold

Command:

```sh
cargo run -p numanager-examples -- lsm_daqmx_validation_note
```

Recorded output excerpt:

```text
# NI-DAQmx Bench Validation Note

This generated note is a scaffold, not a validation result.
It does not create NI tasks, write outputs, read inputs, or claim hardware support.

## Run Identity

| Field | Value |
| --- | --- |
| Driver crate | `numanager-imswitch-daqmx` |
| Device page | `docs/devices/imswitch-daqmx.md` |
| Hub | `Dev1-imswitch-daqmx-hub` |
| NI device model |  |
| NI device name | `Dev1` |
| Serial number or asset tag |  |
| Firmware/software version |  |
| Transport | NI-DAQmx vendor runtime / PCIe, PXI, USB, Ethernet, or cDAQ chassis |
| NI-DAQmx runtime version |  |
| NI-DAQmx package / installer |  |
| Host OS and driver stack | `linux/x86_64` |
| Date | YYYY-MM-DD |
| Operator |  |
| Config file or discovery record | generated from public `imswitch` descriptor |
| `lsm_x_galvo` / `lsm_y_galvo` | `Dev1/ao0` / `Dev1/ao1` |
| `lsm_laser_gate` | `Dev1/port0/line0` |
| `lsm_detector` | `Dev1/ctr0` |
| `lsm_sample_clock` | `Dev1/ctr2` |
| `lsm_sample_clock_source` | `/Dev1/Ctr2InternalOutput` |
| `lsm_start_trigger_source` | `<unset>` |
| Signal channels | `Dev1/ctr0,Dev1/ai0` |
| `daqmx_timeout` | `10.000000s` |
| `inventory_helper_timeout` | `<driver_default>` |

## Evidence Sources

| Source class | Reference | Covered behavior |
| --- | --- | --- |
| Audited SDK/header | Header inventory output | Available NI-DAQmx symbols and header identity only |
| Audited FFI source | FFI source inventory output | Generated binding source, platform cfgs, and symbol availability only |
| Audited target scope | Target-scope audit output | numanager Cargo feature, target cfg, helper-wrapper, and readiness boundary only |
| Vendor package/runtime | Package input inventory and runtime probe outputs | Package identity and loaded runtime version only |
| Bench run | Command output log, inventory output, electrical readback, and runtime API output | Physical channel mapping, task behavior, I/O behavior, cleanup, and runtime publication |

## Setup And Safety

| Area | Observed or enforced behavior |
| --- | --- |
| Motion limits and homing state | Unknown |
| Laser/light output limits and interlocks | Unknown |
| Voltage/current/load limits | Unknown |
| Emergency stop or safe shutdown | Unknown |
| DAQmx safe output state after stop/clear | Unknown |
| Fault injection or recovery tested | Unknown |

## Required Artifacts

| Artifact | Path or value |
| --- | --- |
| External-gates audit command | `scripts/audit-ni-daqmx-external-gates.sh` |
| External-gates audit output |  |
| Package input inventory command | `scripts/audit-ni-daqmx-package-inputs.sh <installer-file-or-directory>` |
| Package input inventory output |  |
| SDK header path or archive |  |
| Header inventory command | `scripts/audit-ni-daqmx-sdk-headers.sh <header-file-or-directory>` |
| Header inventory SHA-256 |  |
| Header inventory NIDAQmx.h count |  |
| Header inventory NIDAQmx.h path |  |
| Installed target-platform NIDAQmx.h used for bindgen |  |
| Bindgen regeneration command |  |
| FFI source inventory command | `scripts/audit-ni-daqmx-sys-source.sh <ni-daqmx-sys-repo>` |
| FFI source inventory output |  |
| Target-scope audit command | `scripts/audit-ni-daqmx-target-scope.sh` |
| Target-scope audit output |  |
| No-hardware helper audit command | `scripts/audit-ni-daqmx-no-hardware-helpers.sh` |
| No-hardware helper audit output |  |
| Plan-validation audit command | `scripts/audit-ni-daqmx-plan-validation.sh` |
| Plan-validation audit output |  |
| Live-gate audit command | `scripts/audit-ni-daqmx-live-gate.sh` |
| Live-gate audit output |  |
| Runtime-probe audit command | `scripts/audit-ni-daqmx-runtime-probe.sh` |
| Runtime-probe audit output |  |
| Example-output sync audit command | `scripts/audit-ni-daqmx-example-output-sync.sh` |
| Example-output sync audit output |  |
| Runtime probe output |  |
| Backend inventory readiness table | `## Backend Inventory` |
| Bench safety preconditions table | `## Setup And Safety` |
| LSM bring-up plan output |  |
| LSM bring-up backend_readiness line | `backend_readiness: ... runtime_version=... promotion_gate_statuses=[pending=9]` |
| Helper build output |  |
| Inventory helper output |  |
| Task lifecycle helper output |  |
| Channel setup helper output |  |
| Plan setup helper output |  |
| Electrical readback or loopback log |  |
| Runtime API output for promoted operation |  |

## Public API Plan Source

| Field | Value |
| --- | --- |
| Source | imswitch |
| Hub | Dev1-imswitch-daqmx-hub |
| Capture API | ConfocalImageCapture request=ConfocalImageCapture |
| Signal API | ScanSignalStream request=ScanSignalStream |
| Execution gate | not_live_task_execution |

## Backend Readiness

| Field | Value |
| --- | --- |
| Execution status | `not_live_backend` |
| Live task execution ready | `false` |
| Live task execution blocker | `feature_ni_daqmx_sdk` |
| Live task execution requested | `false` |
| Feature requested | `false` |
| Feature enabled | `false` |
| Target supported | `true` |
| Runtime detected | `false` |
| Runtime version comparison | `not_configured` |
| Runtime version matches | `unknown` |
| Runtime version comparison basis | `configured_runtime_version_missing` |
| Package identity recorded | `false` |
| SDK header recorded | `false` |
| Hardware validation status | `pending` |
| Evidence status | `pending_ni_daqmx_runtime_evidence` |
| Missing evidence | `runtime_package+runtime_version+runtime_platform+runtime_license+sdk_header_path+sdk_header_sha256+feature_ni_daqmx_sdk+api_audit_and_hardware_validation` |
| External promotion gates | `legal_review+installed_windows_package_license_review+installed_linux_26_5_header_audit+installed_windows_26_5_header_audit+ni_pal_device_inventory+bench_safety_preconditions+task_ordering_routing_completion_cleanup_bench_validation+runtime_publication_hardware_validation+hardware_validation_note` |
| Task-plan readiness agreement | `capture=true;signal=true;basis=backend_status_runtime_version_and_daqmx_task_plan` |

## Backend Inventory

| Field | Value |
| --- | --- |
| Device inventory requested | `false` |
| Inventory helper configured | `false` |
| Inventory helper timeout | `8.000000s` |
| Detected device count | `0` |
| Detected devices | `none` |
| Configured device detected | `false` |
| Configured device identity | `none` |
| Device inventory error | `none` |
| Configured device error | `none` |

## External Promotion Gates

| Gate | Required evidence | Status |
| --- | --- | --- |
| `legal_review` | Completed package-intake legal review for exact Linux and Windows inputs | pending |
| `installed_windows_package_license_review` | Installed Windows package/license boundary audit recorded | pending |
| `installed_linux_26_5_header_audit` | Installed Linux 26.5 NIDAQmx.h inventory, digest, and bindgen command recorded | pending |
| `installed_windows_26_5_header_audit` | Installed Windows 26.5 NIDAQmx.h inventory, digest, and bindgen command recorded | pending |
| `ni_pal_device_inventory` | Process-isolated NI-PAL/device inventory and configured-device identity recorded | pending |
| `bench_safety_preconditions` | Completed Setup And Safety table plus reviewed wiring, load, safe output state, interlocks, emergency stop, cleanup, and fault-recovery constraints | pending |
| `task_ordering_routing_completion_cleanup_bench_validation` | Bench logs for task order, routing, completion, stop/clear, cleanup, and safe output state | pending |
| `runtime_publication_hardware_validation` | Hardware-backed FrameReady and ScanSignalChunk runtime output logs | pending |
| `hardware_validation_note` | Completed hardware validation note following docs/devices/hardware-validation-template.md | pending |

## Current Task Plans

- Capture: execution=not_live_task_execution, blocker=feature_ni_daqmx_sdk, readiness=[ready=false;blocker=feature_ni_daqmx_sdk;missing=runtime_package+runtime_version+runtime_platform+runtime_license+sdk_header_path+sdk_header_sha256+feature_ni_daqmx_sdk+api_audit_and_hardware_validation;hardware=pending], routing=pending_hardware_validation, roles=[x_galvo=Dev1/ao0,y_galvo=Dev1/ao1,laser_gate=Dev1/port0/line0,detector=Dev1/ctr0,sample_clock=Dev1/ctr2], clock=/Dev1/Ctr2InternalOutput, buffers=[scan=512x512x1:262144 samples;tasks=ao_scan:write:f64_volts:2chx262144|do_laser_gate:write:u8_line_state:1chx262144|ci_detector:read:u32_counts:1chx262144|co_sample_clock:generate:counter_pulse_train:1chx262144], cleanup_timeout_s=10.000, waveforms=[ao_scan:x_fast_sawtooth_y_slow_step:pending_hardware_validation|do_laser_gate:high_during_active_pixels:pending_hardware_validation], routes=[clock:/Dev1/Ctr2InternalOutput:co_sample_clock->ci_detector+ao_scan+do_laser_gate;trigger:none->ci_detector+ao_scan+do_laser_gate], sequence=[setup:ao_scan>do_laser_gate>ci_detector>co_sample_clock;write:ao_scan>do_laser_gate;start:ci_detector>ao_scan>do_laser_gate>co_sample_clock;read:ci_detector;wait:co_sample_clock;stop:co_sample_clock>do_laser_gate>ao_scan>ci_detector;clear:co_sample_clock>ci_detector>do_laser_gate>ao_scan], completion=[mode=finite;samples=262144;timeout_s=10.000;evidence=pending_hardware_validation], contract=[mode=raster_finite;write=ao_scan>do_laser_gate;read=ci_detector;wait=co_sample_clock;auto_start=false;timeout_s=10.000;evidence=pending_hardware_validation], executor=[mode=raster_finite;status=not_enabled_pending_hardware_validation;backend=ni_daqmx_sdk_task_wrapper;phases=validate_readiness>setup>write>start>read>wait>publish>cleanup>clear;evidence=pending_hardware_validation], reconstruction=[mode=one_detector_sample_per_pixel;input=ci_detector;scan=512x512;recon=512x512;pixel_format=Mono16;evidence=pending_hardware_validation], publication=[FrameReady:final_reconstructed_frame:scan=512x512:recon=512x512:Mono16:pending_hardware_validation], cancel=[strategy=request_stop_then_clear_created_tasks;stop=co_sample_clock>do_laser_gate>ao_scan>ci_detector;clear=co_sample_clock>ci_detector>do_laser_gate>ao_scan;evidence=pending_hardware_validation], start=[ci_detector>ao_scan>do_laser_gate>co_sample_clock], read=[ci_detector], clear=[co_sample_clock>ci_detector>do_laser_gate>ao_scan], cleanup=stop_started_tasks_then_clear_all_created_tasks
- Signal: execution=not_live_task_execution, blocker=feature_ni_daqmx_sdk, readiness=[ready=false;blocker=feature_ni_daqmx_sdk;missing=runtime_package+runtime_version+runtime_platform+runtime_license+sdk_header_path+sdk_header_sha256+feature_ni_daqmx_sdk+api_audit_and_hardware_validation;hardware=pending], routing=pending_hardware_validation, buffers=[signal=1024x1:1024 samples chunk=256;tasks=ci_signal:read:u32_counts:1chx1024|ai_signal:read:f64_volts:1chx1024], cleanup_timeout_s=10.000, routes=[clock:unspecified:none->ci_signal+ai_signal;trigger:none->ci_signal+ai_signal], sequence=[setup:ci_signal>ai_signal;start:ci_signal>ai_signal;read:ci_signal>ai_signal;stop:ai_signal>ci_signal;clear:ai_signal>ci_signal], completion=[mode=finite;samples=1024;timeout_s=10.000;evidence=pending_hardware_validation], contract=[mode=signal_finite;write=none;read=ci_signal>ai_signal;wait=none;auto_start=false;timeout_s=10.000;evidence=pending_hardware_validation], executor=[mode=signal_finite;status=not_enabled_pending_hardware_validation;backend=ni_daqmx_sdk_task_wrapper;phases=validate_readiness>setup>write>start>read>wait>publish>cleanup>clear;evidence=pending_hardware_validation], publication=[ScanSignalChunk:raw_signal_chunks:channels=2:chunk=256:pending_hardware_validation], cancel=[strategy=request_stop_then_clear_created_tasks;stop=ai_signal>ci_signal;clear=ai_signal>ci_signal;evidence=pending_hardware_validation], start=[ci_signal>ai_signal], read=[ci_signal,ai_signal], clear=[ai_signal>ci_signal], cleanup=stop_started_tasks_then_clear_all_created_tasks

## Preflight Evidence Targets

### Capture

- Tasks: ao_scan:analog_output:Dev1/ao0+Dev1/ao1; do_laser_gate:digital_output:Dev1/port0/line0; ci_detector:counter_input:Dev1/ctr0; co_sample_clock:counter_output:Dev1/ctr2
- Live readiness: ready=false;blocker=feature_ni_daqmx_sdk;missing=runtime_package+runtime_version+runtime_platform+runtime_license+sdk_header_path+sdk_header_sha256+feature_ni_daqmx_sdk+api_audit_and_hardware_validation;hardware=pending
- Start order: ci_detector>ao_scan>do_laser_gate>co_sample_clock
- Read order: ci_detector
- Clear order: co_sample_clock>ci_detector>do_laser_gate>ao_scan
- Routes: sample_clock source=/Dev1/Ctr2InternalOutput producer=co_sample_clock consumers=ci_detector+ao_scan+do_laser_gate; start_trigger source=<empty> consumers=ci_detector+ao_scan+do_laser_gate
- Timing: ao_scan:analog_output:sample_rate=100000.000000Hz:samples=262144; do_laser_gate:digital_output:sample_rate=100000.000000Hz:samples=262144; ci_detector:counter_input:sample_rate=100000.000000Hz:samples=262144; co_sample_clock:counter_output:sample_rate=100000.000000Hz:samples=262144
- Waveforms: ao_scan:x_fast_sawtooth_y_slow_step:pending_hardware_validation; do_laser_gate:high_during_active_pixels:pending_hardware_validation
- Transfers: ao_scan:analog_output:write:f64_volts:2chx262144; do_laser_gate:digital_output:write:u8_line_state:1chx262144; ci_detector:counter_input:read:u32_counts:1chx262144; co_sample_clock:counter_output:generate:counter_pulse_train:1chx262144
- Runtime sequence: step=1:setup:ao_scan>do_laser_gate>ci_detector>co_sample_clock:create_channels_and_timing; step=2:write:ao_scan>do_laser_gate:buffered_output_before_start; step=3:start:ci_detector>ao_scan>do_laser_gate>co_sample_clock:inputs_outputs_then_clock; step=4:read:ci_detector:finite_samples; step=5:wait:co_sample_clock:counter_output_done_or_timeout; step=6:stop:co_sample_clock>do_laser_gate>ao_scan>ci_detector:reverse_started_order; step=7:clear:co_sample_clock>ci_detector>do_laser_gate>ao_scan:reverse_setup_order
- Completion: mode=finite:samples=262144:timeout=10.000000s:evidence=pending_hardware_validation
- Execution contract: mode=raster_finite:write=ao_scan>do_laser_gate:read=ci_detector:wait=co_sample_clock:write_auto_start=false:write_layout=GroupByScanNumber:read_layout=GroupByScanNumber_for_analog_input:timeout=10.000000s:publication_policy=publish_only_after_validated_read_and_reconstruction:evidence=pending_hardware_validation
- Live executor: mode=raster_finite:status=not_enabled_pending_hardware_validation:backend=ni_daqmx_sdk_task_wrapper:phases=phase=1:validate_readiness:none:check_feature_target_package_header_runtime_live_request_and_external_gates,phase=2:setup:ao_scan>do_laser_gate>ci_detector>co_sample_clock:DAQmxCreateTask+channel_creation+timing_and_trigger_configuration,phase=3:write:ao_scan>do_laser_gate:DAQmxWriteAnalogF64+DAQmxWriteDigitalLines_buffered_auto_start_false,phase=4:start:ci_detector>ao_scan>do_laser_gate>co_sample_clock:DAQmxStartTask,phase=5:read:ci_detector:DAQmxReadCounterU32+DAQmxReadAnalogF64_finite_expected_samples,phase=6:wait:co_sample_clock:DAQmxWaitUntilTaskDone,phase=7:publish:none:publish_public_FrameReady_or_ScanSignalChunk_after_validated_read,phase=8:cleanup:co_sample_clock>do_laser_gate>ao_scan>ci_detector:DAQmxStopTask_then_DAQmxClearTask_for_created_tasks,phase=9:clear:co_sample_clock>ci_detector>do_laser_gate>ao_scan:DAQmxClearTask_reverse_setup_order:required_validation=legal_review+installed_header_audit+ni_pal_device_inventory+bench_safety_preconditions+task_ordering_routing_completion_cleanup_bench_validation+runtime_publication_hardware_validation+hardware_validation_note:evidence=pending_hardware_validation
- Reconstruction: mode=one_detector_sample_per_pixel:input=ci_detector:scan=512x512:reconstruction=512x512:pixel_format=Mono16:mapping=row_major_unidirectional_one_sample_per_pixel:accumulation=sum_samples_per_reconstructed_pixel:saturation=clip_to_pixel_format_and_report_saturated_pixels:evidence=pending_hardware_validation
- Publication: FrameReady:final_reconstructed_frame:scan=512x512:reconstruction=512x512:pixel_format=Mono16:required_metadata=frame_handle+stream+scan_width+scan_height+reconstruction_width+reconstruction_height+reconstruction_pixel_size+sample_rate+line_dwell+detectors+saturated_pixels+progress_status:evidence=pending_hardware_validation
- Cancel: strategy=request_stop_then_clear_created_tasks:stop=co_sample_clock>do_laser_gate>ao_scan>ci_detector:clear=co_sample_clock>ci_detector>do_laser_gate>ao_scan:safe_output_state=pending_hardware_validation:evidence=pending_hardware_validation
- Cleanup: policy=stop_started_tasks_then_clear_all_created_tasks:failure_modes=partial_setup_failure+post_start_failure+buffered_write_failure+finite_read_failure+counter_output_wait_timeout:started_task_cleanup=stop_started_tasks_before_clear:stop=co_sample_clock>do_laser_gate>ao_scan>ci_detector:clear=co_sample_clock>ci_detector>do_laser_gate>ao_scan:safe_output_state=pending_hardware_validation:evidence=pending_hardware_validation

### Signal

- Tasks: ci_signal:counter_input:Dev1/ctr0; ai_signal:analog_input:Dev1/ai0
- Live readiness: ready=false;blocker=feature_ni_daqmx_sdk;missing=runtime_package+runtime_version+runtime_platform+runtime_license+sdk_header_path+sdk_header_sha256+feature_ni_daqmx_sdk+api_audit_and_hardware_validation;hardware=pending
- Start order: ci_signal>ai_signal
- Read order: ci_signal>ai_signal
- Clear order: ai_signal>ci_signal
- Routes: sample_clock source=<empty> producer=none consumers=ci_signal+ai_signal; start_trigger source=<empty> consumers=ci_signal+ai_signal
- Timing: ci_signal:counter_input:sample_rate=100000.000000Hz:samples=1024; ai_signal:analog_input:sample_rate=100000.000000Hz:samples=1024
- Waveforms: none
- Transfers: ci_signal:counter_input:read:u32_counts:1chx1024; ai_signal:analog_input:read:f64_volts:1chx1024
- Runtime sequence: step=1:setup:ci_signal>ai_signal:create_channels_and_timing; step=3:start:ci_signal>ai_signal:inputs_outputs_then_clock; step=4:read:ci_signal>ai_signal:finite_samples; step=6:stop:ai_signal>ci_signal:reverse_started_order; step=7:clear:ai_signal>ci_signal:reverse_setup_order
- Completion: mode=finite:samples=1024:timeout=10.000000s:evidence=pending_hardware_validation
- Execution contract: mode=signal_finite:write=none:read=ci_signal>ai_signal:wait=none:write_auto_start=false:write_layout=GroupByScanNumber:read_layout=GroupByScanNumber_for_analog_input:timeout=10.000000s:publication_policy=publish_only_after_validated_read_and_reconstruction:evidence=pending_hardware_validation
- Live executor: mode=signal_finite:status=not_enabled_pending_hardware_validation:backend=ni_daqmx_sdk_task_wrapper:phases=phase=1:validate_readiness:none:check_feature_target_package_header_runtime_live_request_and_external_gates,phase=2:setup:ci_signal>ai_signal:DAQmxCreateTask+channel_creation+timing_and_trigger_configuration,phase=3:write:none:DAQmxWriteAnalogF64+DAQmxWriteDigitalLines_buffered_auto_start_false,phase=4:start:ci_signal>ai_signal:DAQmxStartTask,phase=5:read:ci_signal>ai_signal:DAQmxReadCounterU32+DAQmxReadAnalogF64_finite_expected_samples,phase=6:wait:none:DAQmxWaitUntilTaskDone,phase=7:publish:none:publish_public_FrameReady_or_ScanSignalChunk_after_validated_read,phase=8:cleanup:ai_signal>ci_signal:DAQmxStopTask_then_DAQmxClearTask_for_created_tasks,phase=9:clear:ai_signal>ci_signal:DAQmxClearTask_reverse_setup_order:required_validation=legal_review+installed_header_audit+ni_pal_device_inventory+bench_safety_preconditions+task_ordering_routing_completion_cleanup_bench_validation+runtime_publication_hardware_validation+hardware_validation_note:evidence=pending_hardware_validation
- Reconstruction: none
- Publication: ScanSignalChunk:raw_signal_chunks:channels=counter0+ai0:samples_per_line=1024:lines=1:chunk_size=256:required_metadata=stream+channel_names+timing_origin+line_index+chunk_index+first_sample_index+sample_count+sample_values+sample_rate+sample_period+dropped_samples+dropped_chunks+overflowed:evidence=pending_hardware_validation
- Cancel: strategy=request_stop_then_clear_created_tasks:stop=ai_signal>ci_signal:clear=ai_signal>ci_signal:safe_output_state=pending_hardware_validation:evidence=pending_hardware_validation
- Cleanup: policy=stop_started_tasks_then_clear_all_created_tasks:failure_modes=partial_setup_failure+post_start_failure+buffered_write_failure+finite_read_failure+counter_output_wait_timeout:started_task_cleanup=stop_started_tasks_before_clear:stop=ai_signal>ci_signal:clear=ai_signal>ci_signal:safe_output_state=pending_hardware_validation:evidence=pending_hardware_validation

## Physical Channel Mapping

| Role | Configured channel | Inventory channel | Bench note |
| --- | --- | --- | --- |
| X galvo / piezo AO | `Dev1/ao0` |  |  |
| Y galvo / piezo AO | `Dev1/ao1` |  |  |
| Laser gate DO | `Dev1/port0/line0` |  |  |
| Frame or line trigger DO |  |  |  |
| Analog detector AI | `Dev1/ai0` |  |  |
| APD counter CI | `Dev1/ctr0` |  |  |
| Sample clock CO | `Dev1/ctr2` |  |  |

## Output And Input Validation

Output-writing and input-reading validation requires completed channel setup evidence and recorded hardware safety constraints before any channel is driven.

| Capability | Request or setpoint | Planned channel | Runtime output | Hardware readback | Result | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| AO voltage | Low safe voltage | `Dev1/ao0` |  | Meter or loopback voltage | Unknown |  |
| DO TTL | Low/high transition | `Dev1/port0/line0` |  | Scope, meter, or loopback | Unknown |  |
| AI voltage | Known source or AO loopback | `Dev1/ai0` |  | Reported voltage vs source | Unknown |  |
| CI count | Known pulse source or CO loopback | `Dev1/ctr0` |  | Count rate/count total | Unknown |  |
| CO pulse | Safe frequency and count | `Dev1/ctr2` |  | Scope or CI loopback | Unknown |  |

## LSM Task Execution Gate

Do not expose live `ConfocalImageCapture`, `ConfocalImageStream`, or `ScanSignalStream` until these rows have evidence.

| Behavior | Evidence required | Result |
| --- | --- | --- |
| Finite task creation order | Bench log for AO/DO/AI/CI/CO tasks | Unknown |
| Routing plan topology | `routing_plan` clock producer/consumers and trigger consumers match the bench wiring | Unknown |
| Sample-clock routing | Confirmed source and dependent-task route names | Unknown |
| Derived sample-clock source | If no explicit sample-clock source is configured, the derived `/Device/CtrNInternalOutput` route for the counter-output sample clock is accepted by DAQmx for all AO/DO/AI/CI consumers | Unknown |
| Start-trigger routing | Confirmed digital edge route and start order | Unknown |
| Planned buffer dimensions | `scan_buffer_plan`, `signal_buffer_plan`, and task `buffer_plan` dimensions match the bench request | Unknown |
| Task timing intent | Preflight `planned_timing` rows match configured sample-clock and implicit finite counter-output timing before setup or reads/writes are enabled | Unknown |
| Finite runtime sequence | Preflight `planned_runtime_sequence` and `planned_completion` rows match expected buffered-write, start, read, wait, stop, and clear ordering before live execution is enabled | Unknown |
| Execution contract intent | Public `daqmx_task_plan.execution_contract` and Preflight `planned_execution_contract` rows for raster and signal plans match the intended buffered-before-start write policy, `auto_start=false`, finite read order, wait order, timeout, layout, and publish-after-validated-read policy | Unknown |
| Live executor intent | Public `daqmx_task_plan.live_executor_plan` and preflight `planned_live_executor` rows match the intended SDK task-wrapper backend, readiness gate, phase order, DAQmx API surface, and required validation gates while `executor_status=not_enabled_pending_hardware_validation` | Unknown |
| Reconstruction intent | Public raster `daqmx_task_plan.reconstruction_plan` and preflight `planned_reconstruction` rows match the intended sample-to-pixel mapping, dimensions, accumulation, saturation, and publish-after-reconstruction gate before hardware-derived frames are enabled | Unknown |
| Runtime publication intent | Preflight `planned_publication` rows match the configured raster `FrameReady` or signal `ScanSignalChunk` output contract before hardware-derived runtime events are enabled, using public metadata names such as `frame_handle`, `stream`, `line_index`, `chunk_index`, `first_sample_index`, `sample_count`, and `sample_values` | Unknown |
| Raster timing intent | Preflight `raster_timing_preview` rows match configured sample rate, pixel period, line period, frame period, and total duration before any live writes are enabled | Unknown |
| Signal timing intent | Preflight `signal_timing_preview` rows match configured sample rate, samples_per_line, lines, chunk size, chunk period, line period, and total duration before reads are enabled | Unknown |
| Waveform intent | Raster AO/DO `waveform_plan` and preflight `waveform_preview` rows match expected scan and laser-gate timing before any live writes are enabled | Unknown |
| Cleanup plan | `cleanup_plan` and Preflight `planned_cleanup` rows for failure modes, stop/clear order, configured `daqmx_timeout`, and safe-output-state evidence match the bench run | Unknown |
| Buffered AO/DO writes | Written sample counts and idle/safe final state | Unknown |
| AI/CI reads | Expected sample count, timeout behavior, data layout | Unknown |
| Runtime capture frame publication | `ConfocalImageCapture` `FrameReady` output from numanager with frame handle, final-frame width/height, pixel format, scan/reconstruction dimensions, reconstructed pixel size, sample rate, line dwell, detector metadata, and saturated-pixel status | Unknown |
| Runtime live frame stream publication | `ConfocalImageStream` `FrameReady` output from numanager with stream id, repeated frame handles, dirty-region/update metadata, frame dimensions, pixel format, scan/reconstruction dimensions, reconstructed pixel size, timing metadata, detector metadata, and progress/status events | Unknown |
| Runtime signal chunk publication | `ScanSignalStream` `ScanSignalChunk` output with stream id, channel names, timing origin, line/chunk/first-sample indices, sample count, sample rate, sample period, sample values, dropped sample/chunk counters, overflow status, and progress/status events | Unknown |
| User stop/cancel | Observed stop, clear, and safe output state | Unknown |
| Failure cleanup | Partial setup/start/wait/read failure clears all created tasks; lifecycle-helper failures after task start should capture `cleanup_after_lifecycle_error` and `stopped_task_after_error` rows, setup-helper failures should capture `cleared_partial_task` and `cleanup_after_setup_error` rows when applicable, and I/O-smoke failures after task start should capture `cleanup_after_io_error` and `stopped_task_after_error` rows | Unknown |

## Required Commands

Set `NUMANAGER_DAQMX_DEVICE_NAME`, `NUMANAGER_DAQMX_LSM_X_GALVO`, `NUMANAGER_DAQMX_LSM_Y_GALVO`, `NUMANAGER_DAQMX_LSM_LASER_GATE`, `NUMANAGER_DAQMX_LSM_DETECTOR`, `NUMANAGER_DAQMX_LSM_SAMPLE_CLOCK`, `NUMANAGER_DAQMX_LSM_SAMPLE_CLOCK_SOURCE`, `NUMANAGER_DAQMX_LSM_START_TRIGGER_SOURCE`, `NUMANAGER_DAQMX_SIGNAL_AI`, `NUMANAGER_DAQMX_SIGNAL_CHANNELS`, `NUMANAGER_DAQMX_TIMEOUT_SECONDS`, and `NUMANAGER_DAQMX_HELPER_TIMEOUT_SECONDS` before generating this note when the bench mapping, DAQmx timeout, or helper supervision timeout differs from the defaults.

```sh
scripts/audit-ni-daqmx-external-gates.sh
scripts/audit-ni-daqmx-package-inputs.sh <installer-file-or-directory>
scripts/audit-ni-daqmx-sdk-headers.sh <header-file-or-directory>
scripts/audit-ni-daqmx-sys-source.sh <ni-daqmx-sys-repo>
scripts/audit-ni-daqmx-target-scope.sh
scripts/audit-ni-daqmx-no-hardware-helpers.sh
scripts/audit-ni-daqmx-plan-validation.sh
scripts/audit-ni-daqmx-live-gate.sh
scripts/audit-ni-daqmx-runtime-probe.sh
scripts/audit-ni-daqmx-example-output-sync.sh
NUMANAGER_DAQMX_CONFIG_ONLY=1 cargo run -p numanager-examples --features ni-daqmx-sdk -- daqmx_runtime_probe
cargo run -p numanager-examples --features ni-daqmx-sdk -- daqmx_runtime_probe
cargo run -p numanager-examples -- lsm_daqmx_bringup_plan
cargo build -p numanager-imswitch-daqmx --features ni-daqmx-sdk --bins
target/debug/numanager-daqmx-inventory-helper --device Dev1 --version-only
NUMANAGER_DAQMX_RUNTIME_HELPER=target/debug/numanager-daqmx-inventory-helper cargo run -p numanager-examples --features ni-daqmx-sdk -- daqmx_runtime_probe
target/debug/numanager-daqmx-inventory-helper --device Dev1
NUMANAGER_DAQMX_INVENTORY=1 NUMANAGER_DAQMX_INVENTORY_HELPER=target/debug/numanager-daqmx-inventory-helper cargo run -p numanager-examples --features ni-daqmx-sdk -- daqmx_runtime_probe
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 262144 --width 512 --height 512 --frames 1 --ao-task ao_scan --do-task do_laser_gate --ci-task ci_detector --co-task co_sample_clock --ao Dev1/ao0 --ao Dev1/ao1 --do Dev1/port0/line0 --ci Dev1/ctr0 --co Dev1/ctr2 --min-volts -10.000000 --max-volts 10.000000 --sample-clock-source /Dev1/Ctr2InternalOutput --timeout 10.000000 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1024 --signal-lines 1 --chunk-size 256 --ci-task ci_signal --ai-task ai_signal --ci Dev1/ctr0 --ai Dev1/ai0 --timeout 10.000000 --preflight-only
target/debug/numanager-daqmx-task-lifecycle-helper --dry-run
target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --start --wait-seconds 0.250000
target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --start --wait-seconds 0.250000 --simulate-error-after-start
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 262144 --width 512 --height 512 --frames 1 --ao-task ao_scan --do-task do_laser_gate --ci-task ci_detector --co-task co_sample_clock --ao Dev1/ao0 --ao Dev1/ao1 --do Dev1/port0/line0 --ci Dev1/ctr0 --co Dev1/ctr2 --min-volts -10.000000 --max-volts 10.000000 --sample-clock-source /Dev1/Ctr2InternalOutput --timeout 10.000000 --preflight-only --simulate-setup-error-after 1
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1024 --signal-lines 1 --chunk-size 256 --ci-task ci_signal --ai-task ai_signal --ci Dev1/ctr0 --ai Dev1/ai0 --timeout 10.000000 --preflight-only --simulate-setup-error-after 1
target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --wait-seconds NaN
target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --name ''
target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --name ' lifecycle '
target/debug/numanager-daqmx-task-lifecycle-helper --simulate-error-after-start
target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --simulate-error-after-start
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate NaN --samples 1 --ci Dev1/ctr0 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 0 --samples 1 --ci Dev1/ctr0 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --timeout NaN --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --timeout 0 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --sample-clock-source '' --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --sample-clock-source ' /Dev1/Ctr0InternalOutput ' --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --start-trigger '' --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --start-trigger ' /Dev1/PFI0 ' --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci '' --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci ' Dev1/ctr0 ' --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --ci-task '' --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --ci-task ' signal ' --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --signal-lines 0 --ci Dev1/ctr0 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 5 --signal-lines 2 --ci Dev1/ctr0 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --chunk-size 1 --ci Dev1/ctr0 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --signal-lines 1 --chunk-size 2 --ci Dev1/ctr0 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --co Dev1/ctr0 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --ai Dev1/ai0 --ci-task signal --ai-task signal --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 0 --ci Dev1/ctr0 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 2147483648 --ci Dev1/ctr0 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 3 --width 2 --height 2 --frames 1 --ci Dev1/ctr0 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --width 1 --ci Dev1/ctr0 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --width 18446744073709551615 --height 2 --frames 1 --ci Dev1/ctr0 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --width 2 --height 2 --frames 4611686018427387904 --ci Dev1/ctr0 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --width 0 --height 1 --frames 1 --ci Dev1/ctr0 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --width 1 --height 0 --frames 1 --ci Dev1/ctr0 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --width 1 --height 1 --frames 0 --ci Dev1/ctr0 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1073741824 --ao Dev1/ao0 --ao Dev1/ao1 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ao Dev1/ao0 --min-volts NaN --max-volts 1 --preflight-only
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ao Dev1/ao0 --min-volts 1 --max-volts -1 --preflight-only
target/debug/numanager-daqmx-channel-setup-helper --kind co --channel Dev1/ctr2 --name '' --dry-run
target/debug/numanager-daqmx-channel-setup-helper --kind co --channel Dev1/ctr2 --name ' channel-setup ' --dry-run
target/debug/numanager-daqmx-channel-setup-helper --kind co --channel '' --dry-run
target/debug/numanager-daqmx-channel-setup-helper --kind co --channel ' Dev1/ctr2 ' --dry-run
target/debug/numanager-daqmx-channel-setup-helper --kind co --channel Dev1/ctr2 --frequency inf --dry-run
target/debug/numanager-daqmx-channel-setup-helper --kind co --channel Dev1/ctr2 --frequency 0 --dry-run
target/debug/numanager-daqmx-channel-setup-helper --kind co --channel Dev1/ctr2 --duty-cycle NaN --dry-run
target/debug/numanager-daqmx-channel-setup-helper --kind co --channel Dev1/ctr2 --duty-cycle 1.5 --dry-run
target/debug/numanager-daqmx-io-smoke-helper --kind co --channel '' --samples 1
target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --name '' --samples 1
target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --name ' io-smoke ' --samples 1
target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --frequency inf --samples 1
target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --frequency 0 --samples 1
target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --duty-cycle NaN --samples 1
target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --duty-cycle 1.5 --samples 1
target/debug/numanager-daqmx-channel-setup-helper --kind ai --channel Dev1/ai0 --min-volts NaN --max-volts 1 --dry-run
target/debug/numanager-daqmx-channel-setup-helper --kind ai --channel Dev1/ai0 --min-volts 1 --max-volts -1 --dry-run
target/debug/numanager-daqmx-io-smoke-helper --kind ai --channel Dev1/ai0 --samples 1 --timeout NaN
target/debug/numanager-daqmx-io-smoke-helper --kind ai --channel Dev1/ai0 --samples 1 --timeout 0
target/debug/numanager-daqmx-io-smoke-helper --kind ai --channel Dev1/ai0 --samples 0
target/debug/numanager-daqmx-io-smoke-helper --kind ai --channel Dev1/ai0 --samples 2147483648
target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --min-volts NaN --max-volts 1 --volts 0
target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --min-volts 1 --max-volts -1 --volts 0
target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --volts NaN
target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --min-volts -1 --max-volts 1 --volts 2
target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --min-volts 1 --max-volts 5 --volts 2
target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --simulate-error-after-start
target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --volts 0 --execute
target/debug/numanager-daqmx-task-lifecycle-helper
target/debug/numanager-daqmx-channel-setup-helper --kind ai --channel Dev1/ai0 --dry-run
target/debug/numanager-daqmx-channel-setup-helper --kind ao --channel Dev1/ao0 --dry-run
target/debug/numanager-daqmx-channel-setup-helper --kind ao --channel Dev1/ao1 --dry-run
target/debug/numanager-daqmx-channel-setup-helper --kind ci --channel Dev1/ctr0 --dry-run
target/debug/numanager-daqmx-channel-setup-helper --kind co --channel Dev1/ctr2 --dry-run
target/debug/numanager-daqmx-channel-setup-helper --kind do --channel Dev1/port0/line0 --dry-run
target/debug/numanager-daqmx-channel-setup-helper --kind ai --channel Dev1/ai0
target/debug/numanager-daqmx-channel-setup-helper --kind ao --channel Dev1/ao0
target/debug/numanager-daqmx-channel-setup-helper --kind ao --channel Dev1/ao1
target/debug/numanager-daqmx-channel-setup-helper --kind ci --channel Dev1/ctr0
target/debug/numanager-daqmx-channel-setup-helper --kind co --channel Dev1/ctr2
target/debug/numanager-daqmx-channel-setup-helper --kind do --channel Dev1/port0/line0
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 262144 --width 512 --height 512 --frames 1 --ao-task ao_scan --do-task do_laser_gate --ci-task ci_detector --co-task co_sample_clock --ao Dev1/ao0 --ao Dev1/ao1 --do Dev1/port0/line0 --ci Dev1/ctr0 --co Dev1/ctr2 --min-volts -10.000000 --max-volts 10.000000 --sample-clock-source /Dev1/Ctr2InternalOutput --timeout 10.000000
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1024 --signal-lines 1 --chunk-size 256 --ci-task ci_signal --ai-task ai_signal --ci Dev1/ctr0 --ai Dev1/ai0 --timeout 10.000000
target/debug/numanager-daqmx-io-smoke-helper --kind ai --channel Dev1/ai0 --samples 1
target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --volts 0
target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao1 --volts 0
target/debug/numanager-daqmx-io-smoke-helper --kind ci --channel Dev1/ctr0 --samples 1
target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --frequency 10 --samples 1
target/debug/numanager-daqmx-io-smoke-helper --kind do --channel Dev1/port0/line0 --line-state false
target/debug/numanager-daqmx-io-smoke-helper --kind ai --channel Dev1/ai0 --samples 1 --simulate-error-after-start
target/debug/numanager-daqmx-io-smoke-helper --kind ci --channel Dev1/ctr0 --samples 1 --simulate-error-after-start
target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --frequency 10 --samples 1 --simulate-error-after-start
target/debug/numanager-daqmx-io-smoke-helper --kind ai --channel Dev1/ai0 --samples 1 --execute --bench-safety-reviewed
target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --volts 0 --execute --bench-safety-reviewed
target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao1 --volts 0 --execute --bench-safety-reviewed
target/debug/numanager-daqmx-io-smoke-helper --kind ci --channel Dev1/ctr0 --samples 1 --execute --bench-safety-reviewed
target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --frequency 10 --samples 1 --execute --bench-safety-reviewed
target/debug/numanager-daqmx-io-smoke-helper --kind do --channel Dev1/port0/line0 --line-state false --execute --bench-safety-reviewed
```

Commands containing `--execute` are bench-only I/O smoke checks; review wiring, load, safe output state, and cleanup before running them.
Commands containing `--simulate-error-after-start` without `--execute` are no-DAQmx cleanup-log simulations.
`NUMANAGER_DAQMX_CONFIG_ONLY=1` should print effective `probe_config`, `connected: Bool(false)`, and a no-runtime `backend_status` with `connect_requested=false`; it must not load the NI-DAQmx vendor runtime.

## Command Output Log

| Command | Exit status | Stdout/stderr artifact | Result | Notes |
| --- | --- | --- | --- | --- |
| `scripts/audit-ni-daqmx-external-gates.sh` |  |  | Unknown |  |
| `scripts/audit-ni-daqmx-package-inputs.sh <installer-file-or-directory>` |  |  | Unknown |  |
| `scripts/audit-ni-daqmx-sdk-headers.sh <header-file-or-directory>` |  |  | Unknown |  |
| `scripts/audit-ni-daqmx-sys-source.sh <ni-daqmx-sys-repo>` |  |  | Unknown |  |
| `scripts/audit-ni-daqmx-target-scope.sh` |  |  | Unknown |  |
| `scripts/audit-ni-daqmx-no-hardware-helpers.sh` |  |  | Unknown |  |
| `scripts/audit-ni-daqmx-plan-validation.sh` |  |  | Unknown |  |
| `scripts/audit-ni-daqmx-live-gate.sh` |  |  | Unknown |  |
| `scripts/audit-ni-daqmx-runtime-probe.sh` |  |  | Unknown |  |
| `scripts/audit-ni-daqmx-example-output-sync.sh` |  |  | Unknown |  |
| `NUMANAGER_DAQMX_CONFIG_ONLY=1 cargo run -p numanager-examples --features ni-daqmx-sdk -- daqmx_runtime_probe` |  |  | Unknown |  |
| `cargo run -p numanager-examples --features ni-daqmx-sdk -- daqmx_runtime_probe` |  |  | Unknown |  |
| `cargo run -p numanager-examples -- lsm_daqmx_bringup_plan` |  |  | Unknown |  |
| `cargo build -p numanager-imswitch-daqmx --features ni-daqmx-sdk --bins` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-inventory-helper --device Dev1 --version-only` |  |  | Unknown |  |
| `NUMANAGER_DAQMX_RUNTIME_HELPER=target/debug/numanager-daqmx-inventory-helper cargo run -p numanager-examples --features ni-daqmx-sdk -- daqmx_runtime_probe` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-inventory-helper --device Dev1` |  |  | Unknown |  |
| `NUMANAGER_DAQMX_INVENTORY=1 NUMANAGER_DAQMX_INVENTORY_HELPER=target/debug/numanager-daqmx-inventory-helper cargo run -p numanager-examples --features ni-daqmx-sdk -- daqmx_runtime_probe` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 262144 --width 512 --height 512 --frames 1 --ao-task ao_scan --do-task do_laser_gate --ci-task ci_detector --co-task co_sample_clock --ao Dev1/ao0 --ao Dev1/ao1 --do Dev1/port0/line0 --ci Dev1/ctr0 --co Dev1/ctr2 --min-volts -10.000000 --max-volts 10.000000 --sample-clock-source /Dev1/Ctr2InternalOutput --timeout 10.000000 --preflight-only` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1024 --signal-lines 1 --chunk-size 256 --ci-task ci_signal --ai-task ai_signal --ci Dev1/ctr0 --ai Dev1/ai0 --timeout 10.000000 --preflight-only` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-task-lifecycle-helper --dry-run` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --start --wait-seconds 0.250000` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --start --wait-seconds 0.250000 --simulate-error-after-start` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 262144 --width 512 --height 512 --frames 1 --ao-task ao_scan --do-task do_laser_gate --ci-task ci_detector --co-task co_sample_clock --ao Dev1/ao0 --ao Dev1/ao1 --do Dev1/port0/line0 --ci Dev1/ctr0 --co Dev1/ctr2 --min-volts -10.000000 --max-volts 10.000000 --sample-clock-source /Dev1/Ctr2InternalOutput --timeout 10.000000 --preflight-only --simulate-setup-error-after 1` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1024 --signal-lines 1 --chunk-size 256 --ci-task ci_signal --ai-task ai_signal --ci Dev1/ctr0 --ai Dev1/ai0 --timeout 10.000000 --preflight-only --simulate-setup-error-after 1` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --wait-seconds NaN` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --name ''` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --name ' lifecycle '` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-task-lifecycle-helper --simulate-error-after-start` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --simulate-error-after-start` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --preflight-only` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-plan-setup-helper --sample-rate NaN --samples 1 --ci Dev1/ctr0 --preflight-only` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-plan-setup-helper --sample-rate 0 --samples 1 --ci Dev1/ctr0 --preflight-only` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --timeout NaN --preflight-only` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --timeout 0 --preflight-only` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --sample-clock-source '' --preflight-only` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --sample-clock-source ' /Dev1/Ctr0InternalOutput ' --preflight-only` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --start-trigger '' --preflight-only` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --start-trigger ' /Dev1/PFI0 ' --preflight-only` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci '' --preflight-only` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci ' Dev1/ctr0 ' --preflight-only` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --ci-task '' --preflight-only` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --ci-task ' signal ' --preflight-only` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --signal-lines 0 --ci Dev1/ctr0 --preflight-only` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 5 --signal-lines 2 --ci Dev1/ctr0 --preflight-only` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --chunk-size 1 --ci Dev1/ctr0 --preflight-only` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --signal-lines 1 --chunk-size 2 --ci Dev1/ctr0 --preflight-only` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --co Dev1/ctr0 --preflight-only` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --ai Dev1/ai0 --ci-task signal --ai-task signal --preflight-only` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 0 --ci Dev1/ctr0 --preflight-only` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 2147483648 --ci Dev1/ctr0 --preflight-only` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 3 --width 2 --height 2 --frames 1 --ci Dev1/ctr0 --preflight-only` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --width 1 --ci Dev1/ctr0 --preflight-only` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --width 18446744073709551615 --height 2 --frames 1 --ci Dev1/ctr0 --preflight-only` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --width 2 --height 2 --frames 4611686018427387904 --ci Dev1/ctr0 --preflight-only` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --width 0 --height 1 --frames 1 --ci Dev1/ctr0 --preflight-only` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --width 1 --height 0 --frames 1 --ci Dev1/ctr0 --preflight-only` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --width 1 --height 1 --frames 0 --ci Dev1/ctr0 --preflight-only` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1073741824 --ao Dev1/ao0 --ao Dev1/ao1 --preflight-only` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ao Dev1/ao0 --min-volts NaN --max-volts 1 --preflight-only` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ao Dev1/ao0 --min-volts 1 --max-volts -1 --preflight-only` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-channel-setup-helper --kind co --channel Dev1/ctr2 --name '' --dry-run` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-channel-setup-helper --kind co --channel Dev1/ctr2 --name ' channel-setup ' --dry-run` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-channel-setup-helper --kind co --channel '' --dry-run` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-channel-setup-helper --kind co --channel ' Dev1/ctr2 ' --dry-run` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-channel-setup-helper --kind co --channel Dev1/ctr2 --frequency inf --dry-run` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-channel-setup-helper --kind co --channel Dev1/ctr2 --frequency 0 --dry-run` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-channel-setup-helper --kind co --channel Dev1/ctr2 --duty-cycle NaN --dry-run` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-channel-setup-helper --kind co --channel Dev1/ctr2 --duty-cycle 1.5 --dry-run` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-io-smoke-helper --kind co --channel '' --samples 1` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --name '' --samples 1` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --name ' io-smoke ' --samples 1` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --frequency inf --samples 1` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --frequency 0 --samples 1` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --duty-cycle NaN --samples 1` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --duty-cycle 1.5 --samples 1` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-channel-setup-helper --kind ai --channel Dev1/ai0 --min-volts NaN --max-volts 1 --dry-run` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-channel-setup-helper --kind ai --channel Dev1/ai0 --min-volts 1 --max-volts -1 --dry-run` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-io-smoke-helper --kind ai --channel Dev1/ai0 --samples 1 --timeout NaN` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-io-smoke-helper --kind ai --channel Dev1/ai0 --samples 1 --timeout 0` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-io-smoke-helper --kind ai --channel Dev1/ai0 --samples 0` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-io-smoke-helper --kind ai --channel Dev1/ai0 --samples 2147483648` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --min-volts NaN --max-volts 1 --volts 0` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --min-volts 1 --max-volts -1 --volts 0` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --volts NaN` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --min-volts -1 --max-volts 1 --volts 2` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --min-volts 1 --max-volts 5 --volts 2` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --simulate-error-after-start` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --volts 0 --execute` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-task-lifecycle-helper` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-channel-setup-helper --kind ai --channel Dev1/ai0 --dry-run` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-channel-setup-helper --kind ao --channel Dev1/ao0 --dry-run` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-channel-setup-helper --kind ao --channel Dev1/ao1 --dry-run` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-channel-setup-helper --kind ci --channel Dev1/ctr0 --dry-run` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-channel-setup-helper --kind co --channel Dev1/ctr2 --dry-run` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-channel-setup-helper --kind do --channel Dev1/port0/line0 --dry-run` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-channel-setup-helper --kind ai --channel Dev1/ai0` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-channel-setup-helper --kind ao --channel Dev1/ao0` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-channel-setup-helper --kind ao --channel Dev1/ao1` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-channel-setup-helper --kind ci --channel Dev1/ctr0` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-channel-setup-helper --kind co --channel Dev1/ctr2` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-channel-setup-helper --kind do --channel Dev1/port0/line0` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 262144 --width 512 --height 512 --frames 1 --ao-task ao_scan --do-task do_laser_gate --ci-task ci_detector --co-task co_sample_clock --ao Dev1/ao0 --ao Dev1/ao1 --do Dev1/port0/line0 --ci Dev1/ctr0 --co Dev1/ctr2 --min-volts -10.000000 --max-volts 10.000000 --sample-clock-source /Dev1/Ctr2InternalOutput --timeout 10.000000` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1024 --signal-lines 1 --chunk-size 256 --ci-task ci_signal --ai-task ai_signal --ci Dev1/ctr0 --ai Dev1/ai0 --timeout 10.000000` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-io-smoke-helper --kind ai --channel Dev1/ai0 --samples 1` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --volts 0` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao1 --volts 0` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-io-smoke-helper --kind ci --channel Dev1/ctr0 --samples 1` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --frequency 10 --samples 1` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-io-smoke-helper --kind do --channel Dev1/port0/line0 --line-state false` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-io-smoke-helper --kind ai --channel Dev1/ai0 --samples 1 --simulate-error-after-start` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-io-smoke-helper --kind ci --channel Dev1/ctr0 --samples 1 --simulate-error-after-start` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --frequency 10 --samples 1 --simulate-error-after-start` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-io-smoke-helper --kind ai --channel Dev1/ai0 --samples 1 --execute --bench-safety-reviewed` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --volts 0 --execute --bench-safety-reviewed` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao1 --volts 0 --execute --bench-safety-reviewed` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-io-smoke-helper --kind ci --channel Dev1/ctr0 --samples 1 --execute --bench-safety-reviewed` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --frequency 10 --samples 1 --execute --bench-safety-reviewed` |  |  | Unknown |  |
| `target/debug/numanager-daqmx-io-smoke-helper --kind do --channel Dev1/port0/line0 --line-state false --execute --bench-safety-reviewed` |  |  | Unknown |  |

## Evidence Checklist

| Evidence | Result | Notes |
| --- | --- | --- |
| Evidence-input audit covering local package, installed header, and FFI source inventory markers | Unknown |  |
| External-gates audit showing legal, installed-header, NI-PAL, bench-safety, runtime-publication, and live-task gates remain explicit | Unknown |  |
| Package input inventory | Unknown |  |
| Passing header inventory with NIDAQmx.h count/path, title/copyright, required symbols, runtime-version accessors, and literal package-version macro status | Unknown |  |
| Bindgen regeneration command and FFI-source inventory from the same installed target-platform NIDAQmx.h | Unknown |  |
| FFI source inventory with fork revision, bindgen inputs, platform link cfgs, and runtime-version bindings | Unknown |  |
| numanager NI-DAQmx target-scope audit with Linux/Windows dependency and helper-wrapper boundaries | Unknown |  |
| No-hardware helper audit covering dry-run, preflight-only, simulated-cleanup, and invalid-input guard paths | Unknown |  |
| Plan-validation audit showing valid helper commands stay runnable and invalid plans suppress setup/preflight helpers | Unknown |  |
| Live-gate audit showing live-task intent remains not-live until bench evidence exists | Unknown |  |
| Task-plan live readiness showing per-plan blocker, missing evidence, runtime-version comparison, backend-status agreement, and pending hardware validation | Unknown |  |
| Runtime-probe audit covering config-only metadata and process-isolated runtime probing | Unknown |  |
| Example-output sync audit covering generated DAQmx scaffold documentation markers | Unknown |  |
| Runtime probe config-only | Unknown |  |
| Runtime probe | Unknown |  |
| Backend inventory readiness showing helper isolation, requested inventory state, detected-device count, configured-device identity, and contained helper/configured-device errors | Unknown |  |
| LSM bring-up plan with backend_readiness and promotion_gate_statuses captured before helper commands | Unknown |  |
| Bench safety preconditions recorded before --execute helper commands | Unknown |  |
| Helper build | Unknown |  |
| Isolated Linux runtime probe | Unknown |  |
| Device inventory | Unknown |  |
| Raster plan preflight | Unknown |  |
| Signal plan preflight | Unknown |  |
| Task lifecycle dry run | Unknown |  |
| Task lifecycle cleanup-log simulation | Unknown |  |
| Plan setup cleanup-log simulation | Unknown |  |
| Helper invalid numeric/range/transfer/raster/signal input guard for non-finite/non-positive timing and frequency, non-finite/out-of-range duty cycle, empty route sources, whitespace-padded route sources, empty channels/task labels, leading/trailing whitespace in helper identifiers, duplicate physical channels, duplicate active task labels, invalid signal line/chunk metadata, single-channel empty channel inputs, empty explicit task names, incomplete raster dimensions, raster dimension overflow, raster frame-product overflow, reversed ranges, AO smoke ranges that exclude the 0 V final write, oversized transfers, raster mismatches, and I/O smoke --execute without --bench-safety-reviewed | Unknown |  |
| Empty task lifecycle | Unknown |  |
| Channel setup dry run | Unknown |  |
| Channel setup | Unknown |  |
| Raster plan setup | Unknown |  |
| Signal plan setup | Unknown |  |
| Output/input readback | Unknown |  |
| Runtime ConfocalImageCapture FrameReady output with frame handle, final-frame dimensions, pixel format, scan/reconstruction metadata, timing metadata, detector metadata, reconstruction pixel size, and saturated-pixel status | Unknown |  |
| Runtime ConfocalImageStream FrameReady output with stream id, repeated frame handles, dirty-region/update metadata, dimensions, pixel format, scan/reconstruction metadata, timing metadata, detector metadata, reconstruction pixel size, and progress/status events | Unknown |  |
| Runtime ScanSignalStream ScanSignalChunk output with stream id, channel names, timing origin, line/chunk/first-sample indices, sample count, sample rate, sample period, sample values, dropped sample/chunk counters, overflow status, and progress/status events | Unknown |  |
| User stop/cancel and cleanup | Unknown |  |

## Remaining Uncertainty

| Behavior | Uncertainty | Evidence needed before support claim |
| --- | --- | --- |
| Package/license boundary | Local installer identities, Linux package license-file identities, and Windows online-installer PE/payload metadata are recorded, but legal review has not established redistribution permission and the installed Windows package/license boundary has not been audited | Completed package-intake note with legal review for exact Linux and Windows inputs |
| Installed 26.5 headers | The 26.5 Linux package input and Windows online installer are identified, but no installed 26.5 NIDAQmx.h tree has been audited for either target platform | Passing header inventory, recorded bindgen regeneration command, and bindgen-source audit from the same installed Linux or Windows 26.5 target-platform NIDAQmx.h before publishing regenerated 26.5 bindings |
| Linux NI-PAL readiness | On the current Linux host, NI-PAL can abort the process during inventory or empty-task creation | Bench host log showing runtime probe, process-isolated version probe, process-isolated inventory, and empty task create/clear without process abort |
| Physical channel mapping | Configured Dev1 role channels are plan inputs, not proof that those channels exist or are safely wired | Inventory output plus bench mapping for AO/DO/AI/CI/CO role channels |
| Routing semantics | routing_plan records candidate clock/trigger topology, but route source strings and start order are not validated on hardware | Plan-setup and bench logs showing accepted timing/trigger configuration and the observed task order |
| Output safety | AO/DO/CO helper commands are gated, but safe voltage, TTL state, load, final idle state, and pulse count are not proven | Meter/scope/loopback evidence for reviewed safe setpoints and cleanup behavior |
| Input semantics | AI/CI reads are planned, but sample layout, counts, timeout behavior, and APD/count scaling are not proven | Known-source or loopback readback logs for AI/CI, including sample count and timeout observations |
| Runtime publication | Simulator publishes ConfocalImageCapture FrameReady, ConfocalImageStream FrameReady updates, and ScanSignalStream ScanSignalChunk output with the public metadata contract; the DAQmx backend does not yet publish hardware-derived frames/chunks | Hardware-backed runtime output logs showing capture FrameReady final-frame metadata, live-stream FrameReady update/dirty-region/progress metadata, and ScanSignalChunk channel/timing/sample/drop/overflow/progress metadata after task execution behavior is validated |
| Failure cleanup | Helper cleanup paths are implemented for lifecycle errors after start, partial setup, and post-start I/O failure, but real DAQmx failure modes are not characterized | Bench logs capturing cleanup rows after controlled start/wait/setup/read/write failures |

## Promotion Gate

Keep live NI-DAQmx task execution disabled until every checklist row has bench evidence.

```

## NI-DAQmx Runtime Probe

Command:

```sh
NUMANAGER_DAQMX_CONFIG_ONLY=1 cargo run -p numanager-examples --features ni-daqmx-sdk -- daqmx_runtime_probe
```

Recorded output excerpt:

```text
probe_config: device_name=Dev1, runtime_package=NI-DAQmx, runtime_version=<runtime_probe>, runtime_platform=linux x86_64, sdk_header_path=/usr/include/NIDAQmx.h, helper_timeout=<driver_default>, live_task_execution=false
config_only: true
connected: Bool(false)
backend_status: Map({"bringup_helpers_compiled": Map({"channel_setup": Bool(true), "inventory": Bool(true), "io_smoke": Bool(true), "plan_setup": Bool(true), "task_lifecycle": Bool(true)}), "configured": Bool(false), "configured_device_detected": Bool(false), "configured_device_error": Null, "configured_device_identity": Null, "configured_runtime_version": Null, "configured_runtime_version_major": Null, "configured_runtime_version_minor": Null, "configured_runtime_version_update": Null, "connect_requested": Bool(false), "detected_devices": List([]), "detected_runtime_version": Null, "detected_runtime_version_major": Null, "detected_runtime_version_minor": Null, "detected_runtime_version_update": Null, "device_inventory_error": Null, "device_inventory_requested": Bool(false), "evidence_status": String("pending_ni_daqmx_runtime_evidence"), "execution_status": String("not_live_backend"), "external_promotion_gate_statuses": Map({"bench_safety_preconditions": Map({"evidence_required": String("Completed Setup And Safety table plus reviewed wiring, load, safe output state, interlocks, emergency stop, cleanup, and fault-recovery constraints"), "status": String("pending"), "support_claim": String("not_validated")}), "hardware_validation_note": Map({"evidence_required": String("Completed hardware validation note following docs/devices/hardware-validation-template.md"), "status": String("pending"), "support_claim": String("not_validated")}), "installed_linux_26_5_header_audit": Map({"evidence_required": String("Installed Linux 26.5 NIDAQmx.h inventory, digest, and bindgen command recorded"), "status": String("pending"), "support_claim": String("not_validated")}), "installed_windows_26_5_header_audit": Map({"evidence_required": String("Installed Windows 26.5 NIDAQmx.h inventory, digest, and bindgen command recorded"), "status": String("pending"), "support_claim": String("not_validated")}), "installed_windows_package_license_review": Map({"evidence_required": String("Installed Windows package/license boundary audit recorded"), "status": String("pending"), "support_claim": String("not_validated")}), "legal_review": Map({"evidence_required": String("Completed package-intake legal review for exact Linux and Windows inputs"), "status": String("pending"), "support_claim": String("not_validated")}), "ni_pal_device_inventory": Map({"evidence_required": String("Process-isolated NI-PAL/device inventory and configured-device identity recorded"), "status": String("pending"), "support_claim": String("not_validated")}), "runtime_publication_hardware_validation": Map({"evidence_required": String("Hardware-backed FrameReady and ScanSignalChunk runtime output logs"), "status": String("pending"), "support_claim": String("not_validated")}), "task_ordering_routing_completion_cleanup_bench_validation": Map({"evidence_required": String("Bench logs for task order, routing, completion, stop/clear, cleanup, and safe output state"), "status": String("pending"), "support_claim": String("not_validated")})}), "external_promotion_gates": List([String("legal_review"), String("installed_windows_package_license_review"), String("installed_linux_26_5_header_audit"), String("installed_windows_26_5_header_audit"), String("ni_pal_device_inventory"), String("bench_safety_preconditions"), String("task_ordering_routing_completion_cleanup_bench_validation"), String("runtime_publication_hardware_validation"), String("hardware_validation_note")]), "feature_enabled": Bool(true), "feature_requested": Bool(true), "hardware_validation_status": String("pending"), "inventory_helper_configured": Bool(false), "inventory_helper_timeout": TimeInterval(TimeInterval { value: 8.0, unit: Seconds }), "live_task_execution_blocker": String("package_or_header_evidence_missing"), "live_task_execution_ready": Bool(false), "live_task_execution_requested": Bool(false), "metadata_configured": Bool(false), "missing": List([String("runtime_version"), String("api_audit_and_hardware_validation")]), "package_identity_recorded": Bool(false), "runtime_detected": Bool(false), "runtime_version_comparison": String("not_configured"), "runtime_version_comparison_basis": String("configured_runtime_version_missing"), "runtime_version_matches": Null, "sdk_header_recorded": Bool(true), "target_supported": Bool(true), "task_wrapper_compiled": Bool(true)})
runtime_version_comparison: not_configured (matches=unknown, basis=configured_runtime_version_missing)
readiness: feature_requested=true, target_supported=true, feature_enabled=true, metadata_configured=false, live_task_execution_requested=false, live_task_execution_ready=false, blocker=package_or_header_evidence_missing
bringup_helpers: inventory=true, task_lifecycle=true, channel_setup=true, plan_setup=true, io_smoke=true
inventory: requested=false, helper=false, detected_devices=0, configured_device_detected=false, configured_device=none, error=none
missing: runtime_version, api_audit_and_hardware_validation
promotion_gates: legal_review, installed_windows_package_license_review, installed_linux_26_5_header_audit, installed_windows_26_5_header_audit, ni_pal_device_inventory, bench_safety_preconditions, task_ordering_routing_completion_cleanup_bench_validation, runtime_publication_hardware_validation, hardware_validation_note
promotion_gate_statuses: pending=9

```

## NI-DAQmx Raster Plan Preflight

Command:

```sh
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 262144 --width 512 --height 512 --frames 1 --ao-task ao_scan --do-task do_laser_gate --ci-task ci_detector --co-task co_sample_clock --ao Dev1/ao0 --ao Dev1/ao1 --do Dev1/port0/line0 --ci Dev1/ctr0 --co Dev1/ctr2 --min-volts -10.000000 --max-volts 10.000000 --sample-clock-source /Dev1/Ctr2InternalOutput --timeout 10.000000 --preflight-only
```

Recorded output excerpt:

```text
preflight_plan	true
sample_rate_hz	100000.000000
samples_per_channel	262144
sample_clock_source	/Dev1/Ctr2InternalOutput
sample_clock_source_origin	explicit
start_trigger	<empty>
analog_range_volts	-10.000000	10.000000
cleanup_timeout_s	10.000000
planned_task	ao_scan	analog_output	Dev1/ao0,Dev1/ao1
planned_task	do_laser_gate	digital_output	Dev1/port0/line0
planned_task	ci_detector	counter_input	Dev1/ctr0
planned_task	co_sample_clock	counter_output	Dev1/ctr2
planned_setup_order	ao_scan,do_laser_gate,ci_detector,co_sample_clock
planned_start_order	ci_detector,ao_scan,do_laser_gate,co_sample_clock
planned_read_order	ci_detector
planned_stop_order	co_sample_clock,do_laser_gate,ao_scan,ci_detector
planned_clear_order	co_sample_clock,ci_detector,do_laser_gate,ao_scan
cleanup_policy	stop_started_tasks_then_clear_all_created_tasks
planned_sample_clock_route	source=/Dev1/Ctr2InternalOutput	producer=co_sample_clock	consumers=ci_detector,ao_scan,do_laser_gate	edge=Rising
planned_start_trigger_route	source=<empty>	consumers=ci_detector,ao_scan,do_laser_gate	edge=Rising
planned_timing	ci_detector	sample_clock	source=/Dev1/Ctr2InternalOutput	rate_hz=100000.000000	edge=Rising	mode=FiniteSamps	samples_per_channel=262144
planned_timing	ao_scan	sample_clock	source=/Dev1/Ctr2InternalOutput	rate_hz=100000.000000	edge=Rising	mode=FiniteSamps	samples_per_channel=262144
planned_timing	do_laser_gate	sample_clock	source=/Dev1/Ctr2InternalOutput	rate_hz=100000.000000	edge=Rising	mode=FiniteSamps	samples_per_channel=262144
planned_timing	co_sample_clock	implicit	mode=FiniteSamps	samples_per_channel=262144	pulse_frequency_hz=100000.000000	idle_state=Low	duty_cycle=0.500000
raster_timing_preview	pixel_period_s=0.000010000	line_period_s=0.005120000	frame_period_s=2.621440000	total_period_s=2.621440000	evidence=pending_hardware_validation
planned_waveform	ao_scan	analog_output	pattern=x_fast_sawtooth_y_slow_step	sample_order=row_major_unidirectional	width=512	height=512	frames=1	channels=2	voltage_min=-10.000000	voltage_max=10.000000	evidence=pending_hardware_validation
planned_waveform	do_laser_gate	digital_output	pattern=high_during_active_pixels	sample_order=row_major_unidirectional	line_indexing=zero_based	width=512	height=512	frames=1	channels=1	evidence=pending_hardware_validation
waveform_preview	ao_scan	analog_output	pattern=x_fast_sawtooth_y_slow_step	samples=0:x=-10.000,y=-10.000|131328:x=0.020,y=0.020|262143:x=10.000,y=10.000	evidence=pending_hardware_validation
waveform_preview	do_laser_gate	digital_output	pattern=high_during_active_pixels	samples=0:gate=1|131328:gate=1|262143:gate=1	evidence=pending_hardware_validation
planned_transfer	ao_scan	analog_output	write	f64_volts	channels=2	samples_per_channel=262144	total_elements=524288	layout=GroupByScanNumber	auto_start=false	timeout_s=10.000000
planned_transfer	do_laser_gate	digital_output	write	u8_line_state	channels=1	samples_per_channel=262144	total_elements=262144	layout=GroupByScanNumber	auto_start=false	timeout_s=10.000000
planned_transfer	ci_detector	counter_input	read	u32_counts	channels=1	samples_per_channel=262144	total_elements=262144	timeout_s=10.000000
planned_transfer	co_sample_clock	counter_output	generate	counter_pulse_train	channels=1	samples_per_channel=262144	total_elements=262144	timing=implicit_finite
planned_runtime_sequence	step=1	phase=setup	tasks=ao_scan,do_laser_gate,ci_detector,co_sample_clock	basis=create_channels_and_timing	evidence=pending_hardware_validation
planned_runtime_sequence	step=2	phase=write	tasks=ao_scan,do_laser_gate	basis=buffered_output_before_start	evidence=pending_hardware_validation
planned_runtime_sequence	step=3	phase=start	tasks=ci_detector,ao_scan,do_laser_gate,co_sample_clock	basis=inputs_outputs_then_clock	evidence=pending_hardware_validation
planned_runtime_sequence	step=4	phase=read	tasks=ci_detector	basis=finite_samples	evidence=pending_hardware_validation
planned_runtime_sequence	step=5	phase=wait	tasks=co_sample_clock	basis=counter_output_done_or_timeout	evidence=pending_hardware_validation
planned_runtime_sequence	step=6	phase=stop	tasks=co_sample_clock,do_laser_gate,ao_scan,ci_detector	basis=reverse_started_order	evidence=pending_hardware_validation
planned_runtime_sequence	step=7	phase=clear	tasks=co_sample_clock,ci_detector,do_laser_gate,ao_scan	basis=reverse_setup_order	evidence=pending_hardware_validation
planned_completion	mode=finite	samples_per_channel=262144	timeout_s=10.000000	evidence=pending_hardware_validation
planned_execution_contract	mode=raster_finite	write=ao_scan,do_laser_gate	read=ci_detector	wait=co_sample_clock	write_policy=buffered_before_start	write_auto_start=false	write_layout=GroupByScanNumber	read_policy=finite_expected_samples	read_layout=GroupByScanNumber_for_analog_input	timeout_s=10.000000	publication_policy=publish_only_after_validated_read_and_reconstruction	evidence=pending_hardware_validation
planned_live_executor	mode=raster_finite	status=not_enabled_pending_hardware_validation	backend=ni_daqmx_sdk_task_wrapper	target_scope=linux_windows_optional_sdk_backend	required_validation=legal_review,installed_header_audit,ni_pal_device_inventory,bench_safety_preconditions,task_ordering_routing_completion_cleanup_bench_validation,runtime_publication_hardware_validation,hardware_validation_note	evidence=pending_hardware_validation
planned_live_executor_phase	step=1	phase=validate_readiness	tasks=none	api_surface=check_feature_target_package_header_runtime_live_request_and_external_gates	evidence=pending_hardware_validation
planned_live_executor_phase	step=2	phase=setup	tasks=ao_scan,do_laser_gate,ci_detector,co_sample_clock	api_surface=DAQmxCreateTask+channel_creation+timing_and_trigger_configuration	evidence=pending_hardware_validation
planned_live_executor_phase	step=3	phase=write	tasks=ao_scan,do_laser_gate	api_surface=DAQmxWriteAnalogF64+DAQmxWriteDigitalLines_buffered_auto_start_false	evidence=pending_hardware_validation
planned_live_executor_phase	step=4	phase=start	tasks=ci_detector,ao_scan,do_laser_gate,co_sample_clock	api_surface=DAQmxStartTask	evidence=pending_hardware_validation
planned_live_executor_phase	step=5	phase=read	tasks=ci_detector	api_surface=DAQmxReadCounterU32+DAQmxReadAnalogF64_finite_expected_samples	evidence=pending_hardware_validation
planned_live_executor_phase	step=6	phase=wait	tasks=co_sample_clock	api_surface=DAQmxWaitUntilTaskDone	evidence=pending_hardware_validation
planned_live_executor_phase	step=7	phase=publish	tasks=none	api_surface=publish_public_FrameReady_or_ScanSignalChunk_after_validated_read	evidence=pending_hardware_validation
planned_live_executor_phase	step=8	phase=cleanup	tasks=co_sample_clock,do_laser_gate,ao_scan,ci_detector	api_surface=DAQmxStopTask_then_DAQmxClearTask_for_created_tasks	evidence=pending_hardware_validation
planned_live_executor_phase	step=9	phase=clear	tasks=co_sample_clock,ci_detector,do_laser_gate,ao_scan	api_surface=DAQmxClearTask_reverse_setup_order	evidence=pending_hardware_validation
planned_reconstruction	mode=one_detector_sample_per_pixel	input=ci_detector	scan=512x512	frames=1	reconstruction=512x512	pixel_format=pending_runtime_reconstruction	sample_to_pixel_mapping=row_major_unidirectional_one_sample_per_pixel	accumulation=sum_samples_per_reconstructed_pixel	background_subtraction=disabled_until_hardware_validated	saturation_policy=clip_to_pixel_format_and_report_saturated_pixels	publication_gate=publish_after_validated_read_and_reconstruction	evidence=pending_hardware_validation
planned_publication	event=FrameReady	mode=raster_frame_payload	scan=512x512	frames=1	pixel_format=pending_runtime_reconstruction	required_metadata=frame_handle,stream,scan_width,scan_height,reconstruction_width,reconstruction_height,reconstruction_pixel_size,sample_rate,line_dwell,detectors,saturated_pixels,progress_status	evidence=pending_hardware_validation
planned_cleanup	failure_modes=partial_setup_failure,post_start_failure,buffered_write_failure,finite_read_failure,counter_output_wait_timeout	started_task_cleanup=stop_started_tasks_before_clear	safe_output_state=pending_hardware_validation	evidence=pending_hardware_validation
planned_cleanup_order	stop=co_sample_clock,do_laser_gate,ao_scan,ci_detector	clear=co_sample_clock,ci_detector,do_laser_gate,ao_scan	timeout_s=10.000000	evidence=pending_hardware_validation
preflight_only	true
created_tasks	0
configured_timing	false
configured_start_trigger	false
started_tasks	false
wrote_output	false
read_input	false

```

## NI-DAQmx Signal Plan Preflight

Command:

```sh
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1024 --signal-lines 1 --chunk-size 256 --ci-task ci_signal --ai-task ai_signal --ci Dev1/ctr0 --ai Dev1/ai0 --timeout 10.000000 --preflight-only
```

Recorded output excerpt:

```text
preflight_plan	true
sample_rate_hz	100000.000000
samples_per_channel	1024
sample_clock_source	<empty>
sample_clock_source_origin	default_task_timebase
start_trigger	<empty>
analog_range_volts	-10.000000	10.000000
cleanup_timeout_s	10.000000
planned_task	ci_signal	counter_input	Dev1/ctr0
planned_task	ai_signal	analog_input	Dev1/ai0
planned_setup_order	ci_signal,ai_signal
planned_start_order	ci_signal,ai_signal
planned_read_order	ci_signal,ai_signal
planned_stop_order	ai_signal,ci_signal
planned_clear_order	ai_signal,ci_signal
cleanup_policy	stop_started_tasks_then_clear_all_created_tasks
planned_sample_clock_route	source=<empty>	producer=none	consumers=ci_signal,ai_signal	edge=Rising
planned_start_trigger_route	source=<empty>	consumers=ci_signal,ai_signal	edge=Rising
planned_timing	ci_signal	sample_clock	source=<empty>	rate_hz=100000.000000	edge=Rising	mode=FiniteSamps	samples_per_channel=1024
planned_timing	ai_signal	sample_clock	source=<empty>	rate_hz=100000.000000	edge=Rising	mode=FiniteSamps	samples_per_channel=1024
signal_timing_preview	sample_period_s=0.000010000	samples_per_line=1024	lines=1	line_period_s=0.010240000	chunk_size=256	chunk_period_s=0.002560000	total_period_s=0.010240000	evidence=pending_hardware_validation
planned_transfer	ci_signal	counter_input	read	u32_counts	channels=1	samples_per_channel=1024	total_elements=1024	timeout_s=10.000000
planned_transfer	ai_signal	analog_input	read	f64_volts	channels=1	samples_per_channel=1024	total_elements=1024	layout=GroupByScanNumber	timeout_s=10.000000
planned_runtime_sequence	step=1	phase=setup	tasks=ci_signal,ai_signal	basis=create_channels_and_timing	evidence=pending_hardware_validation
planned_runtime_sequence	step=3	phase=start	tasks=ci_signal,ai_signal	basis=inputs_outputs_then_clock	evidence=pending_hardware_validation
planned_runtime_sequence	step=4	phase=read	tasks=ci_signal,ai_signal	basis=finite_samples	evidence=pending_hardware_validation
planned_runtime_sequence	step=6	phase=stop	tasks=ai_signal,ci_signal	basis=reverse_started_order	evidence=pending_hardware_validation
planned_runtime_sequence	step=7	phase=clear	tasks=ai_signal,ci_signal	basis=reverse_setup_order	evidence=pending_hardware_validation
planned_completion	mode=finite	samples_per_channel=1024	timeout_s=10.000000	evidence=pending_hardware_validation
planned_execution_contract	mode=signal_finite	write=none	read=ci_signal,ai_signal	wait=none	write_policy=buffered_before_start	write_auto_start=false	write_layout=GroupByScanNumber	read_policy=finite_expected_samples	read_layout=GroupByScanNumber_for_analog_input	timeout_s=10.000000	publication_policy=publish_only_after_validated_read_and_reconstruction	evidence=pending_hardware_validation
planned_live_executor	mode=signal_finite	status=not_enabled_pending_hardware_validation	backend=ni_daqmx_sdk_task_wrapper	target_scope=linux_windows_optional_sdk_backend	required_validation=legal_review,installed_header_audit,ni_pal_device_inventory,bench_safety_preconditions,task_ordering_routing_completion_cleanup_bench_validation,runtime_publication_hardware_validation,hardware_validation_note	evidence=pending_hardware_validation
planned_live_executor_phase	step=1	phase=validate_readiness	tasks=none	api_surface=check_feature_target_package_header_runtime_live_request_and_external_gates	evidence=pending_hardware_validation
planned_live_executor_phase	step=2	phase=setup	tasks=ci_signal,ai_signal	api_surface=DAQmxCreateTask+channel_creation+timing_and_trigger_configuration	evidence=pending_hardware_validation
planned_live_executor_phase	step=3	phase=write	tasks=none	api_surface=DAQmxWriteAnalogF64+DAQmxWriteDigitalLines_buffered_auto_start_false	evidence=pending_hardware_validation
planned_live_executor_phase	step=4	phase=start	tasks=ci_signal,ai_signal	api_surface=DAQmxStartTask	evidence=pending_hardware_validation
planned_live_executor_phase	step=5	phase=read	tasks=ci_signal,ai_signal	api_surface=DAQmxReadCounterU32+DAQmxReadAnalogF64_finite_expected_samples	evidence=pending_hardware_validation
planned_live_executor_phase	step=6	phase=wait	tasks=none	api_surface=DAQmxWaitUntilTaskDone	evidence=pending_hardware_validation
planned_live_executor_phase	step=7	phase=publish	tasks=none	api_surface=publish_public_FrameReady_or_ScanSignalChunk_after_validated_read	evidence=pending_hardware_validation
planned_live_executor_phase	step=8	phase=cleanup	tasks=ai_signal,ci_signal	api_surface=DAQmxStopTask_then_DAQmxClearTask_for_created_tasks	evidence=pending_hardware_validation
planned_live_executor_phase	step=9	phase=clear	tasks=ai_signal,ci_signal	api_surface=DAQmxClearTask_reverse_setup_order	evidence=pending_hardware_validation
planned_publication	event=ScanSignalChunk	mode=raw_signal_chunks	channels=ci_signal,ai_signal	samples_per_line=1024	lines=1	chunk_size=256	required_metadata=stream,channel_names,timing_origin,line_index,chunk_index,first_sample_index,sample_count,sample_values,sample_rate,sample_period,dropped_samples,dropped_chunks,overflowed	evidence=pending_hardware_validation
planned_cleanup	failure_modes=partial_setup_failure,post_start_failure,buffered_write_failure,finite_read_failure,counter_output_wait_timeout	started_task_cleanup=stop_started_tasks_before_clear	safe_output_state=pending_hardware_validation	evidence=pending_hardware_validation
planned_cleanup_order	stop=ai_signal,ci_signal	clear=ai_signal,ci_signal	timeout_s=10.000000	evidence=pending_hardware_validation
preflight_only	true
created_tasks	0
configured_timing	false
configured_start_trigger	false
started_tasks	false
wrote_output	false
read_input	false

```

## LSM Confocal Stream API ImSwitch

Command:

```sh
cargo run -p numanager-examples -- lsm_confocal_stream imswitch
```

Recorded output excerpt:

```text
source: imswitch
hub: Dev1-imswitch-daqmx-hub
api: ConfocalImageStream request=ConfocalImageStream
request: raster 512x512 scan reconstructed to 256x256 live stream, dirty-region updates
result: api_status=declared_not_live, completion_basis=configured_api_only, daqmx_task_plan=map(39 keys), evidence_status=pending_ni_daqmx_runtime_evidence, live_task_execution_blocker=feature_ni_daqmx_sdk, live_task_execution_readiness=map(17 keys), live_task_execution_ready=false, live_task_execution_requested=false, overwrite_previous_pixels=true, reconstruction_fields=5, result=live_image_stream_pending, scan_fields=12, update_policy=dirty_region
daqmx_plan: execution=not_live_task_execution, blocker=feature_ni_daqmx_sdk, readiness=[ready=false;blocker=feature_ni_daqmx_sdk;missing=runtime_package+runtime_version+runtime_platform+runtime_license+sdk_header_path+sdk_header_sha256+feature_ni_daqmx_sdk+api_audit_and_hardware_validation;hardware=pending], routing=pending_hardware_validation, roles=[x_galvo=Dev1/ao0,y_galvo=Dev1/ao1,laser_gate=Dev1/port0/line0,detector=Dev1/ctr0,sample_clock=Dev1/ctr2], clock=/Dev1/Ctr2InternalOutput, buffers=[scan=512x512x1:262144 samples;tasks=ao_scan:write:f64_volts:2chx262144|do_laser_gate:write:u8_line_state:1chx262144|ci_detector:read:u32_counts:1chx262144|co_sample_clock:generate:counter_pulse_train:1chx262144], cleanup_timeout_s=10.000, waveforms=[ao_scan:x_fast_sawtooth_y_slow_step:pending_hardware_validation|do_laser_gate:high_during_active_pixels:pending_hardware_validation], routes=[clock:/Dev1/Ctr2InternalOutput:co_sample_clock->ci_detector+ao_scan+do_laser_gate;trigger:none->ci_detector+ao_scan+do_laser_gate], sequence=[setup:ao_scan>do_laser_gate>ci_detector>co_sample_clock;write:ao_scan>do_laser_gate;start:ci_detector>ao_scan>do_laser_gate>co_sample_clock;read:ci_detector;wait:co_sample_clock;stop:co_sample_clock>do_laser_gate>ao_scan>ci_detector;clear:co_sample_clock>ci_detector>do_laser_gate>ao_scan], completion=[mode=finite;samples=262144;timeout_s=10.000;evidence=pending_hardware_validation], contract=[mode=raster_finite;write=ao_scan>do_laser_gate;read=ci_detector;wait=co_sample_clock;auto_start=false;timeout_s=10.000;evidence=pending_hardware_validation], executor=[mode=raster_finite;status=not_enabled_pending_hardware_validation;backend=ni_daqmx_sdk_task_wrapper;phases=validate_readiness>setup>write>start>read>wait>publish>cleanup>clear;evidence=pending_hardware_validation], reconstruction=[mode=one_detector_sample_per_pixel;input=ci_detector;scan=512x512;recon=256x256;pixel_format=Mono16;evidence=pending_hardware_validation], publication=[FrameReady:live_dirty_region_updates:scan=512x512:recon=256x256:Mono16:pending_hardware_validation], cancel=[strategy=request_stop_then_clear_created_tasks;stop=co_sample_clock>do_laser_gate>ao_scan>ci_detector;clear=co_sample_clock>ci_detector>do_laser_gate>ao_scan;evidence=pending_hardware_validation], start=[ci_detector>ao_scan>do_laser_gate>co_sample_clock], read=[ci_detector], clear=[co_sample_clock>ci_detector>do_laser_gate>ao_scan], cleanup=stop_started_tasks_then_clear_all_created_tasks

```

## NI-DAQmx Example-Output Sync Audit

Command:

```sh
scripts/audit-ni-daqmx-example-output-sync.sh
```

Recorded output excerpt:

```text
# NI-DAQmx Example Output Sync Audit

| Workflow | Status |
| --- | --- |
| Bring-up plan emitted DAQmx audit commands | ok |
| Validation note emitted DAQmx audit commands | ok |
| Recorded example output includes DAQmx audit commands and scaffold sections | ok |

This audit compares public DAQmx scaffold example output against recorded documentation markers. It does not create NI-DAQmx tasks, write outputs, read inputs, execute scans, or provide hardware evidence.

```

## LSM Simulator Workflow Audit

Command:

```sh
scripts/audit-lsm-simulator-workflows.sh
```

Recorded output excerpt:

```text
# LSM Simulator Workflow Audit

| Workflow | Status |
| --- | --- |
| Confocal capture | ok |
| Mono8 reconstructed capture | ok |
| Confocal stream | ok |
| Scan-signal stream timing/drop metadata | ok |
| Line-dwell timing | ok |
| Live image cancellation | ok |
| Signal cancellation | ok |
| Composed brightfield/LSM simulator | ok |
| LSM GUI smoke frame/chunk metadata consumption | ok |
| LSM GUI composed shared scene/objective controls | ok |

This audit runs simulator examples through public runtime APIs only. It does not create hardware tasks or provide NI-DAQmx evidence.

```

## NI-DAQmx Inventory Probe

Command:

```sh
NUMANAGER_DAQMX_INVENTORY=1 NUMANAGER_DAQMX_INVENTORY_HELPER=target/debug/numanager-daqmx-inventory-helper cargo run -p numanager-examples --features ni-daqmx-sdk -- daqmx_runtime_probe
```

Recorded output excerpt:

```text
probe_config: device_name=Dev1, runtime_package=NI-DAQmx, runtime_version=<runtime_probe>, runtime_platform=linux x86_64, sdk_header_path=/usr/include/NIDAQmx.h, helper_timeout=<driver_default>, live_task_execution=false
connected: Bool(true)
backend_status: Map({"bringup_helpers_compiled": Map({"channel_setup": Bool(true), "inventory": Bool(true), "io_smoke": Bool(true), "plan_setup": Bool(true), "task_lifecycle": Bool(true)}), "configured": Bool(false), "configured_device_detected": Bool(false), "configured_device_error": Null, "configured_device_identity": Null, "configured_runtime_version": Null, "configured_runtime_version_major": Null, "configured_runtime_version_minor": Null, "configured_runtime_version_update": Null, "connect_requested": Bool(true), "detected_devices": List([]), "detected_runtime_version": String("unknown"), "detected_runtime_version_major": Null, "detected_runtime_version_minor": Null, "detected_runtime_version_update": Null, "device_inventory_error": String("DAQmx inventory helper exited with signal: 6 (SIGABRT) (core dumped); stderr=libnipalu.so failed to initialize\nVerify that nipalk.ko is built and loaded."), "device_inventory_requested": Bool(true), "evidence_status": String("pending_ni_daqmx_runtime_evidence"), "execution_status": String("runtime_probe_only"), "external_promotion_gate_statuses": Map({"bench_safety_preconditions": Map({"evidence_required": String("Completed Setup And Safety table plus reviewed wiring, load, safe output state, interlocks, emergency stop, cleanup, and fault-recovery constraints"), "status": String("pending"), "support_claim": String("not_validated")}), "hardware_validation_note": Map({"evidence_required": String("Completed hardware validation note following docs/devices/hardware-validation-template.md"), "status": String("pending"), "support_claim": String("not_validated")}), "installed_linux_26_5_header_audit": Map({"evidence_required": String("Installed Linux 26.5 NIDAQmx.h inventory, digest, and bindgen command recorded"), "status": String("pending"), "support_claim": String("not_validated")}), "installed_windows_26_5_header_audit": Map({"evidence_required": String("Installed Windows 26.5 NIDAQmx.h inventory, digest, and bindgen command recorded"), "status": String("pending"), "support_claim": String("not_validated")}), "installed_windows_package_license_review": Map({"evidence_required": String("Installed Windows package/license boundary audit recorded"), "status": String("pending"), "support_claim": String("not_validated")}), "legal_review": Map({"evidence_required": String("Completed package-intake legal review for exact Linux and Windows inputs"), "status": String("pending"), "support_claim": String("not_validated")}), "ni_pal_device_inventory": Map({"evidence_required": String("Process-isolated NI-PAL/device inventory and configured-device identity recorded"), "status": String("pending"), "support_claim": String("not_validated")}), "runtime_publication_hardware_validation": Map({"evidence_required": String("Hardware-backed FrameReady and ScanSignalChunk runtime output logs"), "status": String("pending"), "support_claim": String("not_validated")}), "task_ordering_routing_completion_cleanup_bench_validation": Map({"evidence_required": String("Bench logs for task order, routing, completion, stop/clear, cleanup, and safe output state"), "status": String("pending"), "support_claim": String("not_validated")})}), "external_promotion_gates": List([String("legal_review"), String("installed_windows_package_license_review"), String("installed_linux_26_5_header_audit"), String("installed_windows_26_5_header_audit"), String("ni_pal_device_inventory"), String("bench_safety_preconditions"), String("task_ordering_routing_completion_cleanup_bench_validation"), String("runtime_publication_hardware_validation"), String("hardware_validation_note")]), "feature_enabled": Bool(true), "feature_requested": Bool(true), "hardware_validation_status": String("pending"), "inventory_helper_configured": Bool(true), "inventory_helper_timeout": TimeInterval(TimeInterval { value: 8.0, unit: Seconds }), "live_task_execution_blocker": String("live_task_execution_not_requested"), "live_task_execution_ready": Bool(false), "live_task_execution_requested": Bool(false), "metadata_configured": Bool(true), "missing": List([String("api_audit_and_hardware_validation")]), "package_identity_recorded": Bool(true), "runtime_detected": Bool(true), "runtime_version_comparison": String("not_configured"), "runtime_version_comparison_basis": String("configured_runtime_version_missing"), "runtime_version_matches": Null, "sdk_header_recorded": Bool(true), "target_supported": Bool(true), "task_wrapper_compiled": Bool(true)})
runtime_version: unknown
runtime_version_comparison: not_configured (matches=unknown, basis=configured_runtime_version_missing)
readiness: feature_requested=true, target_supported=true, feature_enabled=true, metadata_configured=true, live_task_execution_requested=false, live_task_execution_ready=false, blocker=live_task_execution_not_requested
bringup_helpers: inventory=true, task_lifecycle=true, channel_setup=true, plan_setup=true, io_smoke=true
inventory: requested=true, helper=true, detected_devices=0, configured_device_detected=false, configured_device=none, error=DAQmx inventory helper exited with signal: 6 (SIGABRT) (core dumped); stderr=libnipalu.so failed to initialize Verify that
missing: api_audit_and_hardware_validation
promotion_gates: legal_review, installed_windows_package_license_review, installed_linux_26_5_header_audit, installed_windows_26_5_header_audit, ni_pal_device_inventory, bench_safety_preconditions, task_ordering_routing_completion_cleanup_bench_validation, runtime_publication_hardware_validation, hardware_validation_note
promotion_gate_statuses: pending=9

```

## Motion Stage

Command:

```sh
cargo run -p numanager-examples -- motion_stage [asi|chuo|corvus|esp32|marzhauser|openstage|openuc2|pi-gcs|prior|standa|sutter-mp285|sutter-stage|thorlabs-apt|trinamic-tmcl|triggerscope|wosm|zaber]
```

Recorded output, default ASI source:

```text
detected 1 motion candidate(s)
candidate: Simulated ASI MS-2000 controller
  asi-ms2000-hub [hub, motion.controller, serial.ascii]
  asi-ms2000-xy [axis.xy, stage.xy]
  asi-ms2000-z [axis.z, stage.z]
source: asi
selected 2 stage device(s)
selected stage: asi-ms2000-xy [axis.xy, stage.xy] axes=x,y move=StageMove request=StageMove home=StageHome request=None stop=StageStop request=None
selected stage: asi-ms2000-z [axis.z, stage.z] axes=z move=StageMove request=StageMove home=StageHome request=None stop=StageStop request=None
state set completed: map keys=[x, y, z]
move completed for asi-ms2000-xy: map keys=[mode, x, y]
move completed for asi-ms2000-z: map keys=[mode, z]
timing arm completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
timing start completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
timing stop completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
asi-ms2000-xy x: Position(Position { value: 70.0, unit: Micrometers })
asi-ms2000-xy y: Position(Position { value: 35.0, unit: Micrometers })
asi-ms2000-z z: Position(Position { value: 60.0, unit: Micrometers })
stop completed for asi-ms2000-xy: String("halted")
stop completed for asi-ms2000-z: String("halted")
home completed for asi-ms2000-xy: String("xy homed")
home completed for asi-ms2000-z: String("z homed")
event: operation on [asi-ms2000-xy, asi-ms2000-z] running
event: asi-ms2000-xy.x changed to Position(Position { value: 10.0, unit: Micrometers })
event: asi-ms2000-xy.y changed to Position(Position { value: 15.0, unit: Micrometers })
event: asi-ms2000-z.z changed to Position(Position { value: 20.0, unit: Micrometers })
event: operation on [asi-ms2000-xy, asi-ms2000-z] completed map keys=[x, y, z]
event: operation on [asi-ms2000-xy] running
event: asi-ms2000-xy.x changed to Position(Position { value: 50.0, unit: Micrometers })
event: asi-ms2000-xy.y changed to Position(Position { value: 25.0, unit: Micrometers })
event: operation on [asi-ms2000-xy] completed map keys=[mode, x, y]
event: operation on [asi-ms2000-z] running
event: asi-ms2000-z.z changed to Position(Position { value: 40.0, unit: Micrometers })
event: operation on [asi-ms2000-z] completed map keys=[mode, z]
```

Recorded output excerpts for other stage topologies:

```text
source: thorlabs-apt
selected 1 stage device(s)
selected stage: thorlabs-apt-axis-1 [axis.x, stage.x, motion.apt] axes=position move=StageMove request=StageMove home=StageHome request=None stop=StageStop request=None
state set completed: map keys=[position]
move completed for thorlabs-apt-axis-1: map keys=[acceleration, max_velocity, mode, position]
thorlabs-apt-axis-1 position: Position(Position { value: 350.0, unit: Micrometers })
stop completed for thorlabs-apt-axis-1: String("stopped")
home completed for thorlabs-apt-axis-1: String("homed")

source: esp32
selected 2 stage device(s)
selected stage: esp32-xy [axis.xy] axes=x,y move=StageMove request=StageMove home=none stop=none
selected stage: esp32-z [axis.z] axes=z move=StageMove request=StageMove home=none stop=none
state set completed: map keys=[x, y, z]
move completed for esp32-xy: map keys=[x, y, z]
move completed for esp32-z: map keys=[x, y, z]
esp32-xy x: Position(Position { value: 70.0, unit: Micrometers })
esp32-xy y: Position(Position { value: 35.0, unit: Micrometers })
esp32-z z: Position(Position { value: 60.0, unit: Micrometers })

source: chuo
selected 2 stage device(s)
selected stage: chuo-qt-xy-stage [axis.xy, stage.xy, motion.stage] axes=x,y move=StageMove request=StageMove home=StageHome request=None stop=StageStop request=None
selected stage: chuo-qt-z-stage [axis.z, stage.z, motion.stage] axes=z move=StageMove request=StageMove home=StageHome request=None stop=StageStop request=None
state set completed: map keys=[x, y, z]
move completed for chuo-qt-xy-stage: map keys=[x, y, z]
move completed for chuo-qt-z-stage: Position(Position { value: 40.0, unit: Micrometers })
chuo-qt-xy-stage x: Position(Position { value: 50.0, unit: Micrometers })
chuo-qt-xy-stage y: Position(Position { value: 25.0, unit: Micrometers })
chuo-qt-z-stage z: Position(Position { value: 40.0, unit: Micrometers })
stop completed for chuo-qt-xy-stage: map keys=[moving]
home completed for chuo-qt-z-stage: Position(Position { value: 0.0, unit: Micrometers })

source: marzhauser
selected 2 stage device(s)
selected stage: marzhauser-xy-stage [axis.xy, stage.xy] axes=x,y move=StageMove request=StageMove home=StageHome request=None stop=StageStop request=None
selected stage: marzhauser-z-stage [axis.z, stage.z] axes=z move=StageMove request=StageMove home=StageHome request=None stop=StageStop request=None
state set completed: map keys=[x, y, z]
move completed for marzhauser-xy-stage: map keys=[accel_x, accel_y, mode, speed_x, speed_y, x, y]
move completed for marzhauser-z-stage: map keys=[accel, mode, speed, z]
marzhauser-xy-stage x: Position(Position { value: 70.0, unit: Micrometers })
marzhauser-xy-stage y: Position(Position { value: 35.0, unit: Micrometers })
marzhauser-z-stage z: Position(Position { value: 60.0, unit: Micrometers })
stop completed for marzhauser-xy-stage: String("aborted")
home completed for marzhauser-z-stage: String("z calibrated")

source: openuc2
selected 2 stage device(s)
selected stage: openuc2-xy [axis.xy] axes=x,y move=StageMove request=StageMove home=none stop=none
selected stage: openuc2-z [axis.z] axes=z move=StageMove request=StageMove home=none stop=none
state set completed: map keys=[x, y, z]
move completed for openuc2-xy: map keys=[x, y, z]
move completed for openuc2-z: map keys=[x, y, z]
openuc2-xy x: Position(Position { value: 70.0, unit: Micrometers })
openuc2-xy y: Position(Position { value: 35.0, unit: Micrometers })
openuc2-z z: Position(Position { value: 60.0, unit: Micrometers })

source: pi-gcs
selected 2 stage device(s)
selected stage: pi-gcs-xy-stage [axis.xy, stage.xy] axes=x,y move=StageMove request=StageMove home=StageHome request=None stop=StageStop request=None
selected stage: pi-gcs-z-stage [axis.z, stage.z] axes=z move=StageMove request=StageMove home=StageHome request=None stop=StageStop request=None
state set completed: map keys=[x, y, z]
move completed for pi-gcs-xy-stage: map keys=[mode, x, y]
move completed for pi-gcs-z-stage: map keys=[mode, z]
pi-gcs-xy-stage x: Position(Position { value: 70.0, unit: Micrometers })
pi-gcs-xy-stage y: Position(Position { value: 35.0, unit: Micrometers })
pi-gcs-z-stage z: Position(Position { value: 60.0, unit: Micrometers })
stop completed for pi-gcs-xy-stage: String("xy halted")
home completed for pi-gcs-z-stage: String("z referenced")

source: corvus
selected 2 stage device(s)
selected stage: corvus-xy-stage [axis.xy, stage.xy, motion.stage] axes=x,y move=StageMove request=StageMove home=StageHome request=None stop=StageStop request=None
selected stage: corvus-z-stage [axis.z, stage.z, motion.stage] axes=z move=StageMove request=StageMove home=StageHome request=None stop=StageStop request=None
state set completed: map keys=[x, y, z]
move completed for corvus-xy-stage: map keys=[x, y, z]
move completed for corvus-z-stage: Position(Position { value: 40.0, unit: Micrometers })
corvus-xy-stage x: Position(Position { value: 50.0, unit: Micrometers })
corvus-xy-stage y: Position(Position { value: 25.0, unit: Micrometers })
corvus-z-stage z: Position(Position { value: 40.0, unit: Micrometers })
stop completed for corvus-xy-stage: map keys=[moving]
home completed for corvus-z-stage: Position(Position { value: 0.0, unit: Micrometers })

source: openstage
selected 2 stage device(s)
selected stage: openstage-xy [axis.xy, stage.xy, motion.stage] axes=x,y move=StageMove request=StageMove home=none stop=none
selected stage: openstage-z [axis.z, stage.z, motion.stage] axes=z move=StageMove request=StageMove home=none stop=none
state set completed: map keys=[x, y, z]
move completed for openstage-xy: map keys=[x, y, z]
move completed for openstage-z: map keys=[x, y, z]
openstage-xy x: Position(Position { value: 50.0, unit: Micrometers })
openstage-xy y: Position(Position { value: 25.0, unit: Micrometers })
openstage-z z: Position(Position { value: 40.0, unit: Micrometers })

source: standa
selected 1 stage device(s)
selected stage: standa-8smc4-x [axis.x, stage.1d, standa.8smc4.axis] axes=position move=StageMove request=StageMove home=StageHome request=None stop=StageStop request=None
state set completed: map keys=[position]
move completed for standa-8smc4-x: map keys=[acceleration, busy, mode, position, target, velocity]
standa-8smc4-x position: Position(Position { value: 250.0, unit: Micrometers })
stop completed for standa-8smc4-x: map keys=[busy]
home completed for standa-8smc4-x: map keys=[busy, homed, position]

source: prior
selected 3 stage device(s)
selected stage: prior-xy-stage [axis.xy, stage.xy] axes=x,y move=StageMove request=StageMove home=StageHome request=None stop=StageStop request=None
selected stage: prior-z-stage [axis.z, stage.z] axes=z move=StageMove request=StageMove home=none stop=StageStop request=None
selected stage: prior-nanoscan-z [axis.z, stage.z, piezo.z] axes=z move=StageMove request=StageMove home=none stop=StageStop request=None
state set completed: map keys=[x, y, z x2]
move completed for prior-xy-stage: map keys=[mode, x, y]
move completed for prior-z-stage: map keys=[mode, z]
move completed for prior-nanoscan-z: map keys=[mode, position_steps, z]
prior-xy-stage x: Position(Position { value: 70.0, unit: Micrometers })
prior-z-stage z: Position(Position { value: 60.0, unit: Micrometers })
prior-nanoscan-z z: Position(Position { value: 60.0, unit: Micrometers })
stop completed for prior-nanoscan-z: String("halted")
home completed for prior-xy-stage: String("xy homed")

source: sutter-mp285
selected 2 stage device(s)
selected stage: sutter-mp285-xy [stage.xy, axis.x, axis.y] axes=x,y move=StageMove request=StageMove home=StageHome request=None stop=StageStop request=None
selected stage: sutter-mp285-z [stage.z, axis.z] axes=z move=StageMove request=StageMove home=StageHome request=None stop=StageStop request=None
state set completed: map keys=[xy:x, xy:y, z:z]
move completed for sutter-mp285-xy: map keys=[mode, velocity, x, y, z]
move completed for sutter-mp285-z: map keys=[mode, velocity, x, y, z]
sutter-mp285-xy x: Position(Position { value: 70.0, unit: Micrometers })
sutter-mp285-z z: Position(Position { value: 60.0, unit: Micrometers })
stop completed for sutter-mp285-z: String("stopped")
home completed for sutter-mp285-z: String("origin set")

source: sutter-stage
selected 2 stage device(s)
selected stage: sutter-xy-stage [axis.xy, stage.xy] axes=x,y move=StageMove request=StageMove home=StageHome request=None stop=StageStop request=None
selected stage: sutter-z-stage [axis.z, stage.z] axes=z move=StageMove request=StageMove home=none stop=StageStop request=None
state set completed: map keys=[x, y, z]
move completed for sutter-xy-stage: map keys=[mode, speed, x, y]
move completed for sutter-z-stage: map keys=[mode, speed, z]
sutter-xy-stage x: Position(Position { value: 70.0, unit: Micrometers })
sutter-z-stage z: Position(Position { value: 60.0, unit: Micrometers })
stop completed for sutter-z-stage: String("halted")
home completed for sutter-xy-stage: String("xy homed")

source: triggerscope
selected 1 stage device(s)
selected stage: triggerscope-focus [axis.z, stage.z, motion.stage] axes=z move=StageMove request=StageMove home=none stop=none
state set completed: map keys=[z]
move completed for triggerscope-focus: Position(Position { value: 40.0, unit: Micrometers })
triggerscope-focus z: Position(Position { value: 40.0, unit: Micrometers })

source: trinamic-tmcl
selected 1 stage device(s)
selected stage: trinamic-tmcl-x-stage [stage.1d, motion.stage, state.device, trinamic.tmcl.axis] axes=position move=StageMove request=StageMove home=none stop=StageStop request=None
state set completed: map keys=[position]
move completed for trinamic-tmcl-x-stage: map keys=[actual_speed, actual_steps, axis, axis_index, busy, home_switch, left_limit_switch, position, position_reached, right_limit_switch, target, target_steps]
trinamic-tmcl-x-stage position: Position(Position { value: 250.0, unit: Micrometers })
stop completed for trinamic-tmcl-x-stage: map keys=[actual_speed, actual_steps, axis, axis_index, busy, home_switch, left_limit_switch, position, position_reached, right_limit_switch, target, target_steps]

source: wosm
selected 2 stage device(s)
selected stage: wosm-xy-stage [axis.xy, stage.xy, motion.stage] axes=x,y move=StageMove request=StageMove home=none stop=none
selected stage: wosm-z-stage [axis.z, stage.z, motion.stage] axes=z move=StageMove request=StageMove home=none stop=none
state set completed: map keys=[stage]
move completed for wosm-xy-stage: map keys=[x, y, z]
move completed for wosm-z-stage: map keys=[x, y, z]

source: zaber
selected 1 stage device(s)
selected stage: zaber-ascii-axis-1 [axis.1, stage.axis, stage.x] axes=position move=StageMove request=StageMove home=StageHome request=None stop=StageStop request=None
state set completed: map keys=[position]
move completed for zaber-ascii-axis-1: map keys=[acceleration, mode, position, velocity]
zaber-ascii-axis-1 position: Position(Position { value: 350.0, unit: Micrometers })
stop completed for zaber-ascii-axis-1: String("stopped")
home completed for zaber-ascii-axis-1: String("homed")
```

## Laser

Command:

```sh
cargo run -p numanager-examples -- laser
```

Recorded output:

```text
selected laser family: cobolt
selected laser: cobolt-laser [laser, light.source, shutter, trigger.sink, serial.ascii]
capabilities: dac=Dac request=Dac trigger=TriggerSink request=Trigger
laser property: enabled type=Bool writable=true sequenceable=true
laser property: power type=OpticalPower writable=true sequenceable=true
laser property: actual_power type=OpticalPower writable=false sequenceable=false
laser property: current type=ElectricCurrent writable=true sequenceable=false
laser property: actual_current type=ElectricCurrent writable=false sequenceable=false
laser property: interlock_closed type=Bool writable=false sequenceable=false
laser property: fault type=String writable=false sequenceable=false
laser property: wavelength type=Wavelength writable=false sequenceable=false
laser property: hours type=TimeInterval writable=false sequenceable=false
laser safety: safe map keys=[device, enabled, fault, interlock_closed, state]
laser output request completed: OpticalPower(OpticalPower { value: 5.0, unit: Milliwatts })
laser enable completed: map keys=[enabled]
laser disable completed: map keys=[enabled]
enabled: Bool(false)
power: OpticalPower(OpticalPower { value: 5.0, unit: Milliwatts })
actual_power: OpticalPower(OpticalPower { value: 0.0, unit: Milliwatts })
current: ElectricCurrent(ElectricCurrent { value: 0.0, unit: Milliamps })
actual_current: ElectricCurrent(ElectricCurrent { value: 0.0, unit: Milliamps })
wavelength: Wavelength(Wavelength { value: 488.0, unit: Nanometers })
interlock_closed: Bool(true)
fault: String("No Fault")
event: operation on [cobolt-laser] running
event: cobolt-laser.power changed to OpticalPower(OpticalPower { value: 5.0, unit: Milliwatts })
event: operation on [cobolt-laser] completed OpticalPower(OpticalPower { value: 5.0, unit: Milliwatts })
event: operation on [cobolt-laser] running
event: cobolt-laser.enabled changed to Bool(true)
event: operation on [cobolt-laser] completed map keys=[enabled]
event: operation on [cobolt-laser] running
event: cobolt-laser.enabled changed to Bool(false)
event: operation on [cobolt-laser] completed map keys=[enabled]
```

Command for the Coherent OBIS selector:

```sh
cargo run -p numanager-examples -- laser obis
```

Recorded output excerpt:

```text
selected laser family: obis
selected laser: coherent-obis-laser [laser, light.source, shutter, trigger.sink, serial.scpi]
capabilities: dac=Dac request=Dac trigger=TriggerSink request=Trigger
laser property: power type=OpticalPower writable=true sequenceable=true
laser safety: safe map keys=[device, enabled, fault, state]
laser output request completed: map keys=[power]
laser enable completed: map keys=[enabled, triggered]
laser disable completed: map keys=[enabled, triggered]
enabled: Bool(false)
power: OpticalPower(OpticalPower { value: 5.0, unit: Milliwatts })
actual_power: OpticalPower(OpticalPower { value: 0.0, unit: Milliwatts })
wavelength: Wavelength(Wavelength { value: 488.0, unit: Nanometers })
fault: String("No Fault")
```

Command for the Omicron selector:

```sh
cargo run -p numanager-examples -- laser omicron
```

Recorded output excerpt:

```text
selected laser family: omicron
selected laser: omicron-serial-laser [laser, light.source, shutter, trigger.sink, serial.ascii]
capabilities: dac=Dac request=Dac trigger=TriggerSink request=Trigger
laser property: relative_power type=Ratio writable=true sequenceable=true
laser property: analog_modulation_enabled type=Bool writable=true sequenceable=true
laser property: digital_modulation_enabled type=Bool writable=true sequenceable=true
laser safety: safe map keys=[device, enabled, fault, fault_bits, fault_flags, interlock_closed, state]
laser output request completed: map keys=[level, power, relative_power]
laser enable completed: map keys=[enabled, triggered]
laser disable completed: map keys=[enabled, triggered]
enabled: Bool(false)
power: OpticalPower(OpticalPower { value: 5.010989010989011, unit: Milliwatts })
relative_power: Ratio(Ratio { value: 4.175824175824176, unit: Percent })
actual_power: OpticalPower(OpticalPower { value: 0.0, unit: Milliwatts })
wavelength: Wavelength(Wavelength { value: 488.0, unit: Nanometers })
interlock_closed: Bool(true)
fault: String("No Error")
analog_modulation_enabled: Bool(false)
digital_modulation_enabled: Bool(false)
```

## Light Source

Command:

```sh
cargo run -p numanager-examples -- light_source
```

Recorded output:

```text
selected light source family: coolled
selected light hub: coolled-pe300-hub [hub, light.engine, shutter]
selected light channel: coolled-pe300-channel-1 [light.source, led.channel, trigger.sink]
selected laser: cobolt-laser [laser, light.source, shutter, trigger.sink, serial.ascii]
selected trigger controller: lumencor-cia [trigger.controller, pulse.program, light.engine.adapter]
capabilities: hub trigger=TriggerSink request=Trigger; channel trigger=TriggerSink request=Trigger dac=Dac request=Dac; laser dac=Dac request=Dac; cia program=PulseProgram request=PulseProgram trigger=TriggerSink request=Trigger
channel property: enabled type=Bool writable=true sequenceable=true
channel property: selected type=Bool writable=true sequenceable=true
channel property: intensity type=Ratio writable=true sequenceable=true
laser property: power type=OpticalPower writable=true sequenceable=true
laser property: actual_power type=OpticalPower writable=false sequenceable=false
laser property: wavelength type=Wavelength writable=false sequenceable=false
cia property: event_count type=I64 writable=false sequenceable=false
cia property: run_state type=String writable=false sequenceable=false
channel safety: safe map keys=[device, enabled, state]
laser safety: safe map keys=[device, enabled, fault, interlock_closed, state]
state set completed: map keys=[enabled x2, intensity, selected]
dac set completed: map keys=[intensity]
laser optical power completed: OpticalPower(OpticalPower { value: 15.0, unit: Milliwatts })
cia program completed: map keys=[event_count, run_state]
cia trigger pulse completed: map keys=[run_state]
channel pulse completed: map keys=[enabled, triggered]
hub disable completed: map keys=[enabled, triggered]
timing arm completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
timing start completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
timing stop completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
enabled: Bool(false)
timing_state: map keys=[armed, route_count, routed_channels, running, sequence_count, starts, stops]
enabled: Bool(false)
selected: Bool(false)
intensity: Ratio(Ratio { value: 0.0, unit: Percent })
power: OpticalPower(OpticalPower { value: 15.0, unit: Milliwatts })
run_state: String("Stopped")
event: operation on [coolled-pe300-channel-1, coolled-pe300-hub] running
event: coolled-pe300-channel-1.intensity changed to Ratio(Ratio { value: 25.0, unit: Percent })
event: coolled-pe300-channel-1.selected changed to Bool(true)
event: coolled-pe300-channel-1.enabled changed to Bool(true)
event: coolled-pe300-hub.enabled changed to Bool(true)
event: operation on [coolled-pe300-channel-1, coolled-pe300-hub] completed map keys=[enabled x2, intensity, selected]
event: operation on [coolled-pe300-channel-1] running
event: coolled-pe300-channel-1.intensity changed to Ratio(Ratio { value: 42.0, unit: Percent })
event: operation on [coolled-pe300-channel-1] completed map keys=[intensity]
event: operation on [cobolt-laser] running
event: cobolt-laser.power changed to OpticalPower(OpticalPower { value: 15.0, unit: Milliwatts })
event: operation on [cobolt-laser] completed OpticalPower(OpticalPower { value: 15.0, unit: Milliwatts })
event: operation on [lumencor-cia] running
event: lumencor-cia.event_count changed to I64(0)
event: lumencor-cia.run_state changed to String("Ready")
event: operation on [lumencor-cia] completed map keys=[event_count, run_state]
event: operation on [lumencor-cia] running
event: lumencor-cia.run_state changed to String("Stopped")
event: operation on [lumencor-cia] completed map keys=[run_state]
event: operation on [coolled-pe300-channel-1] running
event: coolled-pe300-channel-1.enabled changed to Bool(true)
event: coolled-pe300-channel-1.selected changed to Bool(true)
event: coolled-pe300-channel-1.enabled changed to Bool(false)
event: coolled-pe300-channel-1.selected changed to Bool(false)
event: operation on [coolled-pe300-channel-1] completed map keys=[enabled, triggered]
event: operation on [coolled-pe300-hub] running
event: coolled-pe300-hub.enabled changed to Bool(false)
event: operation on [coolled-pe300-hub] completed map keys=[enabled, triggered]
event: operation on [coolled-pe300-channel-1, coolled-pe300-hub] running
event: operation on [coolled-pe300-channel-1, coolled-pe300-hub] completed map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
event: coolled-pe300-channel-1.intensity changed to Ratio(Ratio { value: 10.0, unit: Percent })
event: coolled-pe300-channel-1.enabled changed to Bool(true)
event: operation on [coolled-pe300-channel-1, coolled-pe300-hub] running
event: coolled-pe300-hub.enabled changed to Bool(true)
event: operation on [coolled-pe300-channel-1, coolled-pe300-hub] completed map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
event: coolled-pe300-channel-1.intensity changed to Ratio(Ratio { value: 0.0, unit: Percent })
event: coolled-pe300-channel-1.enabled changed to Bool(false)
event: coolled-pe300-hub.enabled changed to Bool(false)
event: operation on [coolled-pe300-channel-1, coolled-pe300-hub] running
event: operation on [coolled-pe300-channel-1, coolled-pe300-hub] completed map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
event: operation on [coolled-pe300-hub] running
event: operation on [coolled-pe300-hub] completed Bool(false)
event: operation on [coolled-pe300-hub] running
event: operation on [coolled-pe300-hub] completed map keys=[armed, route_count, routed_channels, running, sequence_count, starts, stops]
event: operation on [coolled-pe300-channel-1] running
event: operation on [coolled-pe300-channel-1] completed Bool(false)
event: operation on [coolled-pe300-channel-1] running
event: operation on [coolled-pe300-channel-1] completed Bool(false)
event: operation on [coolled-pe300-channel-1] running
event: operation on [coolled-pe300-channel-1] completed Ratio(Ratio { value: 0.0, unit: Percent })
event: operation on [cobolt-laser] running
event: operation on [cobolt-laser] completed OpticalPower(OpticalPower { value: 15.0, unit: Milliwatts })
event: operation on [lumencor-cia] running
event: operation on [lumencor-cia] completed String("Stopped")
```

Command for the Agilent/Keysight Laser Combiner configured selector:

```sh
cargo run -p numanager-examples -- light_source agilent
```

Recorded output excerpt:

```text
selected light source family: agilent
selected light hub: agilent-combiner-hub [hub, light.engine]
selected light channel: agilent-laser-line-1 [light.source, laser, trigger.sink]
capabilities: hub trigger=TriggerSink request=Trigger; channel trigger=TriggerSink request=Trigger dac=Dac request=Dac; laser dac=Dac request=Dac; cia program=PulseProgram request=PulseProgram trigger=TriggerSink request=Trigger
channel property: wavelength type=Wavelength writable=true sequenceable=false
channel property: enabled type=Bool writable=true sequenceable=true
channel property: intensity type=Ratio writable=true sequenceable=true
channel property: power type=OpticalPower writable=true sequenceable=true
state set completed: map keys=[enabled, intensity]
dac set completed: Ratio(Ratio { value: 0.420004577706569, unit: Fraction })
channel pulse completed: map keys=[triggered]
hub disable completed: map keys=[triggered]
timing arm completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
timing start completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
timing stop completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
enabled: Bool(false)
intensity: Ratio(Ratio { value: 0.0, unit: Fraction })
power: OpticalPower(OpticalPower { value: 0.0, unit: Milliwatts })
event: agilent-laser-line-1.intensity changed to Ratio(Ratio { value: 0.2500038147554742, unit: Fraction })
event: agilent-combiner-hub.state_mask changed to I64(1)
event: agilent-laser-line-1.enabled changed to Bool(true)
event: agilent-combiner-hub.shutter_open changed to Bool(false)
```

Command for the Coherent OBIS light-source selector:

```sh
cargo run -p numanager-examples -- light_source obis
```

Recorded output excerpt:

```text
selected light source family: obis
selected light hub: coherent-obis-laser [laser, light.source, shutter, trigger.sink, serial.scpi]
selected light channel: coherent-obis-laser [laser, light.source, shutter, trigger.sink, serial.scpi]
capabilities: hub trigger=TriggerSink request=Trigger; channel trigger=TriggerSink request=Trigger dac=Dac request=Dac; laser dac=Dac request=Dac; cia program=PulseProgram request=PulseProgram trigger=TriggerSink request=Trigger
channel property: enabled type=Bool writable=true sequenceable=true
channel property: power type=OpticalPower writable=true sequenceable=true
state set completed: map keys=[enabled, power]
dac set completed: map keys=[power]
channel pulse completed: map keys=[enabled, triggered]
hub disable completed: map keys=[enabled, triggered]
timing arm completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
enabled: Bool(false)
fault: String("No Fault")
power: OpticalPower(OpticalPower { value: 0.0, unit: Milliwatts })
event: coherent-obis-laser.power changed to OpticalPower(OpticalPower { value: 37.5, unit: Milliwatts })
event: coherent-obis-laser.enabled changed to Bool(true)
```

Command for the Omicron light-source selector:

```sh
cargo run -p numanager-examples -- light_source omicron
```

Recorded output excerpt:

```text
selected light source family: omicron
selected light hub: omicron-serial-laser [laser, light.source, shutter, trigger.sink, serial.ascii]
selected light channel: omicron-serial-laser [laser, light.source, shutter, trigger.sink, serial.ascii]
capabilities: hub trigger=TriggerSink request=Trigger; channel trigger=TriggerSink request=Trigger dac=Dac request=Dac; laser dac=Dac request=Dac; cia program=PulseProgram request=PulseProgram trigger=TriggerSink request=Trigger
channel property: enabled type=Bool writable=true sequenceable=true
channel property: power type=OpticalPower writable=true sequenceable=true
channel property: relative_power type=Ratio writable=true sequenceable=true
state set completed: map keys=[enabled, power]
dac set completed: map keys=[level, power, relative_power]
channel pulse completed: map keys=[enabled, triggered]
hub disable completed: map keys=[enabled, triggered]
timing arm completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
enabled: Bool(false)
fault: String("No Error")
interlock_closed: Bool(true)
power: OpticalPower(OpticalPower { value: 0.0, unit: Milliwatts })
event: omicron-serial-laser.power changed to OpticalPower(OpticalPower { value: 30.007326007326007, unit: Milliwatts })
event: omicron-serial-laser.relative_power changed to Ratio(Ratio { value: 42.002442002442, unit: Percent })
```

Command for the CoolLED pE-340 configured selector:

```sh
cargo run -p numanager-examples -- light_source pe340
```

Recorded output excerpt:

```text
selected light source family: pe340
selected light hub: coolled-pe340-hub [hub, light.engine, shutter]
selected light channel: coolled-pe340-channel-1 [light.source, led.channel, trigger.sink]
capabilities: hub trigger=TriggerSink request=Trigger; channel trigger=TriggerSink request=Trigger dac=Dac request=Dac; laser dac=Dac request=Dac; cia program=PulseProgram request=PulseProgram trigger=TriggerSink request=Trigger
channel property: enabled type=Bool writable=true sequenceable=true
channel property: selected type=Bool writable=true sequenceable=true
channel property: intensity type=Ratio writable=true sequenceable=true
channel property: wavelength type=Wavelength writable=true sequenceable=false
state set completed: map keys=[enabled x2, intensity, selected]
dac set completed: map keys=[intensity]
channel pulse completed: map keys=[enabled, triggered]
hub disable completed: map keys=[enabled, triggered]
timing arm completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
timing start completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
timing stop completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
enabled: Bool(false)
timing_state: map keys=[armed, route_count, routed_channels, running, sequence_count, starts, stops]
intensity: Ratio(Ratio { value: 0.0, unit: Percent })
event: coolled-pe340-channel-1.intensity changed to Ratio(Ratio { value: 25.0, unit: Percent })
event: coolled-pe340-channel-1.enabled changed to Bool(true)
event: coolled-pe340-hub.enabled changed to Bool(false)
```

Command for the CoolLED pE-4000 selector:

```sh
cargo run -p numanager-examples -- light_source pe4000
```

Recorded output excerpt:

```text
selected light source family: pe4000
selected light hub: coolled-pe4000-hub [hub, light.engine, shutter]
selected light channel: coolled-pe4000-channel-1 [light.source, led.channel, trigger.sink]
capabilities: hub trigger=TriggerSink request=Trigger; channel trigger=TriggerSink request=Trigger dac=Dac request=Dac; laser dac=Dac request=Dac; cia program=PulseProgram request=PulseProgram trigger=TriggerSink request=Trigger
channel property: enabled type=Bool writable=true sequenceable=true
channel property: selected type=Bool writable=true sequenceable=true
channel property: intensity type=Ratio writable=true sequenceable=true
channel property: wavelength type=Wavelength writable=true sequenceable=false
state set completed: map keys=[enabled x2, intensity, selected]
dac set completed: map keys=[intensity]
channel pulse completed: map keys=[enabled, triggered]
hub disable completed: map keys=[enabled, triggered]
timing arm completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
timing start completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
timing stop completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
enabled: Bool(false)
timing_state: map keys=[armed, route_count, routed_channels, running, sequence_count, starts, stops]
intensity: Ratio(Ratio { value: 0.0, unit: Percent })
event: coolled-pe4000-channel-1.intensity changed to Ratio(Ratio { value: 25.0, unit: Percent })
event: coolled-pe4000-hub.enabled changed to Bool(false)
```

Command for the Lumencor Spectra configured selector:

```sh
cargo run -p numanager-examples -- light_source lumencor
```

Recorded output excerpt:

```text
selected light source family: lumencor
selected light hub: lumencor-spectra-hub [hub, light.engine, shutter]
selected light channel: lumencor-red [light.source, led.channel, trigger.sink]
capabilities: hub trigger=TriggerSink request=Trigger; channel trigger=TriggerSink request=Trigger dac=Dac request=Dac; laser dac=Dac request=Dac; cia program=PulseProgram request=PulseProgram trigger=TriggerSink request=Trigger
channel property: intensity type=Ratio writable=true sequenceable=true
channel property: wavelength type=Wavelength writable=false sequenceable=false
state set completed: map keys=[enabled, intensity, open]
dac set completed: map keys=[intensity]
channel pulse completed: map keys=[enabled, triggered]
hub disable completed: map keys=[enabled, triggered]
timing arm completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
timing start completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
timing stop completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
enabled: Bool(false)
intensity: Ratio(Ratio { value: 0.0, unit: Percent })
event: lumencor-red.intensity changed to Ratio(Ratio { value: 25.0, unit: Percent })
event: lumencor-spectra-hub.open changed to Bool(false)
```

Command for the Spectral LMM5 configured selector:

```sh
cargo run -p numanager-examples -- light_source lmm5
```

Recorded output excerpt:

```text
selected light source family: lmm5
selected light hub: spectral-lmm5-hub [hub, light.engine, serial.ascii.hex]
selected light channel: spectral-lmm5-line-1 [light.source, laser.line, shutter, trigger.sink]
capabilities: hub trigger=none; channel trigger=TriggerSink request=Trigger dac=Dac request=Dac; laser dac=Dac request=Dac; cia program=PulseProgram request=PulseProgram trigger=TriggerSink request=Trigger
channel property: transmission type=Ratio writable=true sequenceable=true
state set completed: map keys=[enabled, transmission]
dac set completed: Ratio(Ratio { value: 42.0, unit: Percent })
channel pulse completed: map keys=[enabled, triggered]
timing arm completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
timing start completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
timing stop completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
enabled: Bool(false)
transmission: Ratio(Ratio { value: 42.0, unit: Percent })
event: spectral-lmm5-line-1.transmission changed to Ratio(Ratio { value: 25.0, unit: Percent })
event: spectral-lmm5-line-1.enabled changed to Bool(false)
```

Command for the Thorlabs DC2010/DC2100 configured selector:

```sh
cargo run -p numanager-examples -- light_source thorlabs-dc
```

Recorded output excerpt:

```text
selected light source family: thorlabs-dc
selected light hub: thorlabs-dc-led [led.controller, light.source, shutter, trigger.sink]
selected light channel: thorlabs-dc-led [led.controller, light.source, shutter, trigger.sink]
capabilities: hub trigger=TriggerSink request=Trigger; channel trigger=TriggerSink request=Trigger dac=Dac request=Dac; laser dac=Dac request=Dac; cia program=PulseProgram request=PulseProgram trigger=TriggerSink request=Trigger
channel property: constant_current type=ElectricCurrent writable=true sequenceable=true
channel property: pwm_frequency type=Frequency writable=true sequenceable=false
channel safety: safe map keys=[device, enabled, fault, state, status]
state set completed: map keys=[constant_current, enabled]
dac set completed: map keys=[constant_current]
channel pulse completed: map keys=[enabled, triggered]
hub disable completed: map keys=[enabled, triggered]
timing arm completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
timing start completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
timing stop completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
constant_current: ElectricCurrent(ElectricCurrent { value: 0.0, unit: Milliamps })
event: thorlabs-dc-led.constant_current changed to ElectricCurrent(ElectricCurrent { value: 2.5, unit: Milliamps })
event: thorlabs-dc-led.enabled changed to Bool(false)
```

Command for the Thorlabs DC2200 configured SCPI selector:

```sh
cargo run -p numanager-examples -- light_source dc2200
```

Recorded output excerpt:

```text
selected light source family: dc2200
selected light hub: thorlabs-dc2200-led [led.controller, light.source, shutter, trigger.sink]
selected light channel: thorlabs-dc2200-led [led.controller, light.source, shutter, trigger.sink]
capabilities: hub trigger=TriggerSink request=Trigger; channel trigger=TriggerSink request=Trigger dac=Dac request=Dac; laser dac=Dac request=Dac; cia program=PulseProgram request=PulseProgram trigger=TriggerSink request=Trigger
channel property: constant_current type=ElectricCurrent writable=true sequenceable=true
channel property: pwm_frequency type=Frequency writable=true sequenceable=false
channel property: pwm_duty_cycle type=Ratio writable=true sequenceable=false
channel property: maximum_frequency type=Frequency writable=false sequenceable=false
channel property: wavelength type=Wavelength writable=false sequenceable=false
channel property: forward_bias type=Voltage writable=false sequenceable=false
channel safety: safe map keys=[device, enabled, fault, state, status]
state set completed: map keys=[constant_current, enabled]
dac set completed: map keys=[constant_current]
channel pulse completed: map keys=[enabled, triggered]
hub disable completed: map keys=[enabled, triggered]
constant_current: ElectricCurrent(ElectricCurrent { value: 0.0, unit: Milliamps })
event: thorlabs-dc2200-led.constant_current changed to ElectricCurrent(ElectricCurrent { value: 2.5, unit: Milliamps })
event: thorlabs-dc2200-led.enabled changed to Bool(false)
```

Command for the Thorlabs DC3100 configured selector:

```sh
cargo run -p numanager-examples -- light_source dc3100
```

Recorded output excerpt:

```text
selected light source family: dc3100
selected light hub: thorlabs-dc3100-led [led.controller, light.source, shutter, trigger.sink]
selected light channel: thorlabs-dc3100-led [led.controller, light.source, shutter, trigger.sink]
capabilities: hub trigger=TriggerSink request=Trigger; channel trigger=TriggerSink request=Trigger dac=Dac request=Dac; laser dac=Dac request=Dac; cia program=PulseProgram request=PulseProgram trigger=TriggerSink request=Trigger
channel property: constant_current type=ElectricCurrent writable=true sequenceable=true
channel property: pwm_current type=ElectricCurrent writable=false sequenceable=true
channel property: pwm_frequency type=Frequency writable=true sequenceable=false
channel property: pwm_duty_cycle type=Ratio writable=true sequenceable=false
channel property: modulation_current type=ElectricCurrent writable=true sequenceable=false
channel property: modulation_frequency type=Frequency writable=true sequenceable=false
channel property: modulation_depth type=Ratio writable=true sequenceable=false
channel safety: safe map keys=[device, enabled, fault, state, status]
state set completed: map keys=[constant_current, enabled]
dac set completed: map keys=[constant_current]
channel pulse completed: map keys=[enabled, triggered]
hub disable completed: map keys=[enabled, triggered]
constant_current: ElectricCurrent(ElectricCurrent { value: 0.0, unit: Milliamps })
event: thorlabs-dc3100-led.constant_current changed to ElectricCurrent(ElectricCurrent { value: 2.5, unit: Milliamps })
event: thorlabs-dc3100-led.enabled changed to Bool(false)
```

Command for the Thorlabs DC4100/DC4104 configured selector:

```sh
cargo run -p numanager-examples -- light_source dc4100
```

Recorded output excerpt:

```text
selected light source family: dc4100
selected light hub: thorlabs-dc4100-led-1 [light.source, led.channel, trigger.sink]
selected light channel: thorlabs-dc4100-led-1 [light.source, led.channel, trigger.sink]
capabilities: hub trigger=TriggerSink request=Trigger; channel trigger=TriggerSink request=Trigger dac=Dac request=Dac; laser dac=Dac request=Dac; cia program=PulseProgram request=PulseProgram trigger=TriggerSink request=Trigger
channel property: brightness type=Ratio writable=true sequenceable=true
channel property: constant_current type=ElectricCurrent writable=true sequenceable=true
state set completed: map keys=[brightness, enabled]
dac set completed: map keys=[brightness]
channel pulse completed: map keys=[enabled, triggered]
hub disable completed: map keys=[enabled, triggered]
timing arm completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
timing start completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
timing stop completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
brightness: Ratio(Ratio { value: 0.0, unit: Percent })
event: thorlabs-dc4100-led-1.brightness changed to Ratio(Ratio { value: 25.0, unit: Percent })
event: thorlabs-dc4100-led-1.enabled changed to Bool(false)
```

Command for the Bluebox Optics niji configured support:

```sh
cargo run -p numanager-examples -- light_source niji
```

Recorded output:

```text
selected light source family: niji
selected light hub: niji-hub [hub, light.engine, shutter, serial.ascii]
selected light channel: niji-channel-1 [light.source, led.channel, trigger.sink]
selected laser: cobolt-laser [laser, light.source, shutter, trigger.sink, serial.ascii]
selected trigger controller: lumencor-cia [trigger.controller, pulse.program, light.engine.adapter]
capabilities: hub trigger=TriggerSink request=Trigger; channel trigger=TriggerSink request=Trigger dac=Dac request=Dac; laser dac=Dac request=Dac; cia program=PulseProgram request=PulseProgram trigger=TriggerSink request=Trigger
channel property: enabled type=Bool writable=true sequenceable=true
channel property: selected type=Bool writable=true sequenceable=true
channel property: intensity type=Ratio writable=true sequenceable=true
channel property: wavelength type=Wavelength writable=false sequenceable=false
channel property: label type=String writable=false sequenceable=false
laser property: power type=OpticalPower writable=true sequenceable=true
laser property: actual_power type=OpticalPower writable=false sequenceable=false
laser property: wavelength type=Wavelength writable=false sequenceable=false
cia property: event_count type=I64 writable=false sequenceable=false
cia property: run_state type=String writable=false sequenceable=false
channel safety: safe map keys=[device, enabled, state]
laser safety: safe map keys=[device, enabled, fault, interlock_closed, state]
state set completed: map keys=[enabled, intensity, selected]
dac set completed: Ratio(Ratio { value: 42.0, unit: Percent })
laser optical power completed: OpticalPower(OpticalPower { value: 15.0, unit: Milliwatts })
cia program completed: map keys=[event_count, run_state]
cia trigger pulse completed: map keys=[run_state]
channel pulse completed: Bool(true)
hub disable completed: Bool(false)
timing arm completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
timing start completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
timing stop completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
enabled: Bool(false)
fault: Bool(false)
interlock_closed: Bool(true)
output_temperature: Temperature(Temperature { value: 22.5, unit: Celsius })
ambient_temperature: Temperature(Temperature { value: 22.0, unit: Celsius })
enabled: Bool(true)
selected: Bool(true)
intensity: Ratio(Ratio { value: 42.0, unit: Percent })
power: OpticalPower(OpticalPower { value: 15.0, unit: Milliwatts })
run_state: String("Stopped")
event: operation on [niji-channel-1, niji-hub] running
event: niji-channel-1.intensity changed to Ratio(Ratio { value: 25.0, unit: Percent })
event: niji-channel-1.enabled changed to Bool(true)
event: niji-channel-1.selected changed to Bool(true)
event: niji-channel-1.enabled changed to Bool(true)
event: niji-channel-1.selected changed to Bool(true)
event: niji-hub.enabled changed to Bool(true)
event: operation on [niji-channel-1, niji-hub] completed map keys=[enabled, intensity, selected]
event: operation on [niji-channel-1] running
event: niji-channel-1.intensity changed to Ratio(Ratio { value: 42.0, unit: Percent })
event: operation on [niji-channel-1] completed Ratio(Ratio { value: 42.0, unit: Percent })
event: operation on [cobolt-laser] running
event: cobolt-laser.power changed to OpticalPower(OpticalPower { value: 15.0, unit: Milliwatts })
event: operation on [cobolt-laser] completed OpticalPower(OpticalPower { value: 15.0, unit: Milliwatts })
event: operation on [lumencor-cia] running
event: lumencor-cia.event_count changed to I64(0)
event: lumencor-cia.run_state changed to String("Ready")
event: operation on [lumencor-cia] completed map keys=[event_count, run_state]
event: operation on [lumencor-cia] running
event: lumencor-cia.run_state changed to String("Stopped")
event: operation on [lumencor-cia] completed map keys=[run_state]
event: operation on [niji-channel-1] running
event: niji-channel-1.enabled changed to Bool(true)
event: niji-channel-1.selected changed to Bool(true)
event: operation on [niji-channel-1] completed Bool(true)
event: operation on [niji-hub] running
event: niji-hub.enabled changed to Bool(false)
event: operation on [niji-hub] completed Bool(false)
event: operation on [niji-channel-1, niji-hub] running
event: operation on [niji-channel-1, niji-hub] completed map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
event: operation on [niji-channel-1, niji-hub] running
event: operation on [niji-channel-1, niji-hub] completed map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
event: operation on [niji-channel-1, niji-hub] running
event: operation on [niji-channel-1, niji-hub] completed map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
event: operation on [niji-hub] running
event: operation on [niji-hub] completed Bool(false)
event: operation on [niji-hub] running
event: operation on [niji-hub] completed Bool(false)
event: operation on [niji-hub] running
event: operation on [niji-hub] completed Bool(true)
event: operation on [niji-hub] running
event: operation on [niji-hub] completed Temperature(Temperature { value: 22.5, unit: Celsius })
event: operation on [niji-hub] running
event: operation on [niji-hub] completed Temperature(Temperature { value: 22.0, unit: Celsius })
event: operation on [niji-channel-1] running
event: operation on [niji-channel-1] completed Bool(true)
event: operation on [niji-channel-1] running
event: operation on [niji-channel-1] completed Bool(true)
event: operation on [niji-channel-1] running
event: operation on [niji-channel-1] completed Ratio(Ratio { value: 42.0, unit: Percent })
event: operation on [cobolt-laser] running
event: operation on [cobolt-laser] completed OpticalPower(OpticalPower { value: 15.0, unit: Milliwatts })
event: operation on [lumencor-cia] running
event: operation on [lumencor-cia] completed String("Stopped")
```

Command for the OpenUC2 configured light output:

```sh
cargo run -p numanager-examples -- light_source openuc2
```

Recorded output excerpt:

```text
selected light source family: openuc2
selected light hub: openuc2-laser [light.source, shutter, trigger.sink]
selected light channel: openuc2-laser [light.source, shutter, trigger.sink]
capabilities: hub trigger=TriggerSink request=Trigger; channel trigger=TriggerSink request=Trigger dac=Dac request=Dac; laser dac=Dac request=Dac; cia program=PulseProgram request=PulseProgram trigger=TriggerSink request=Trigger
channel property: enabled type=Bool writable=true sequenceable=true
channel property: power type=Ratio writable=true sequenceable=true
state set completed: map keys=[enabled, power]
dac set completed: map keys=[power, wire_value]
channel pulse completed: map keys=[enabled, triggered, wire_value]
hub disable completed: map keys=[enabled, triggered, wire_value]
timing arm completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
timing start completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
timing stop completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
enabled: Bool(false)
power: Ratio(Ratio { value: 0.0, unit: Percent })
event: openuc2-laser.power changed to Ratio(Ratio { value: 25.0, unit: Percent })
event: openuc2-laser.enabled changed to Bool(false)
```

Command for the WOSM configured light output:

```sh
cargo run -p numanager-examples -- light_source wosm
```

Recorded output excerpt:

```text
selected light source family: wosm
selected light hub: wosm-light-1 [light.source, dac.output, trigger.sink]
selected light channel: wosm-light-1 [light.source, dac.output, trigger.sink]
capabilities: hub trigger=TriggerSink request=Trigger; channel trigger=TriggerSink request=Trigger dac=Dac request=Dac; laser dac=Dac request=Dac; cia program=PulseProgram request=PulseProgram trigger=TriggerSink request=Trigger
channel property: output type=Ratio writable=true sequenceable=true
channel property: enabled type=Bool writable=true sequenceable=true
state set completed: map keys=[enabled, output]
dac set completed: Ratio(Ratio { value: 42.0, unit: Percent })
channel pulse completed: Null
hub disable completed: I64(0)
timing arm completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
timing start completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
timing stop completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
enabled: Bool(false)
output: Ratio(Ratio { value: 42.0, unit: Percent })
event: wosm-light-1.output changed to Ratio(Ratio { value: 25.0, unit: Percent })
event: wosm-light-1.enabled changed to Bool(false)
```

### Mightex HID Output Bring-Up

Command without a detected Sirius HID controller:

```sh
cargo run -p numanager-examples --features os-hid -- light_source
```

Recorded output prefix:

```text
mightex hardware: no Sirius HID light controller detected
selected light hub: coolled-pe300-hub [hub, light.engine, shutter]
selected light channel: coolled-pe300-channel-1 [light.source, led.channel, trigger.sink]
...
```

Command for an intentional low-output hardware check:

```sh
NUMANAGER_MIGHTEX_OUTPUT=1 cargo run -p numanager-examples --features os-hid -- light_source
```

Expected hardware-output lines when a Sirius BLS/SLC controller is present:

```text
mightex hardware: detected 1 Sirius HID controller candidate(s)
mightex hardware candidate: Mightex BLS HID controller Sirius BLS (5 device(s))
mightex hardware: added Mightex BLS HID controller Sirius BLS with 5 device(s)
mightex hardware output: selected mightex-bls-channel-1 [light.source, led.channel, trigger.sink]
mightex hardware hub: mightex-bls-hub [hub, light.engine, hid.device]
mightex hardware initial safety: safe map keys=[device, enabled, state]
mightex hardware output setup completed: map keys=[enabled, intensity, mode]
mightex hardware dac completed: map keys=[intensity]
mightex hardware active mode: String("normal")
mightex hardware active enabled: Bool(true)
mightex hardware active intensity: Ratio(Ratio { value: 1.0, unit: Percent })
mightex hardware active safety: active map keys=[device, enabled, state]
mightex hardware output: holding 1% output for 1000 ms
mightex hardware output observation required: record visible light or meter/readback before validation
mightex hardware disable completed: map keys=[enabled, triggered]
mightex hardware disable-all completed: map keys=[command, last_transaction, reply, reply_expected, reply_report_count, support_level]
mightex hardware final safety: safe map keys=[device, enabled, state]
mightex hardware mode: String("normal")
mightex hardware enabled: Bool(false)
mightex hardware intensity: Ratio(Ratio { value: 1.0, unit: Percent })
mightex hardware soft_start: Bool(false)
mightex hardware trigger_program: String("pulse")
mightex hardware trigger_repeat_count: I64(1)
mightex hardware trigger_pulse_current_1: I64(0)
mightex hardware trigger_pulse_current_2: I64(50)
mightex hardware trigger_pulse_current_3: I64(0)
mightex hardware trigger_pulse_time_1: I64(500000)
mightex hardware trigger_pulse_time_2: I64(500000)
mightex hardware trigger_pulse_time_3: I64(500000)
mightex hardware trigger_follow_on_current: I64(50)
mightex hardware trigger_follow_off_current: I64(0)
mightex hardware overdrive_current_limit: Ratio(Ratio { value: ..., unit: Percent })
mightex hardware overdrive_duty_cycle_limit: Ratio(Ratio { value: ..., unit: Percent })
mightex hardware overdrive_pulse_width_limit: TimeInterval(TimeInterval { value: ..., unit: Milliseconds })
optional SLC readback:
mightex hardware normal_current_max_raw: I64(...)
mightex hardware normal_current_set_raw: I64(...)
mightex hardware strobe_current_max_raw: I64(20)
mightex hardware strobe_repeat_count_raw: I64(1)
mightex hardware trigger_current_max_raw: I64(20)
mightex hardware trigger_polarity_raw: I64(1)
mightex hardware profile_frequency: Frequency(Frequency { value: 1.0, unit: Hertz })
mightex hardware profile_duty_cycle: Ratio(Ratio { value: 50.0, unit: Percent })
mightex hardware profile_current_1_raw: I64(0)
mightex hardware profile_current_2_raw: I64(10)
mightex hardware current_max_raw_readback: I64(...)
mightex hardware current_raw_readback: I64(...)
mightex hardware strobe_current_max_raw_readback: I64(...)
mightex hardware strobe_repeat_count_raw_readback: I64(...)
mightex hardware strobe_profile_raw_readback: List(...)
mightex hardware trigger_current_max_raw_readback: I64(...)
mightex hardware trigger_polarity_raw_readback: I64(...)
mightex hardware trigger_profile_raw_readback: List(...)
mightex hardware load_voltage_raw: I64(...)
mightex hardware hub command_count: I64(...)
mightex hardware hub last_command: String("...")
mightex hardware hub last_reply: String("...")
mightex hardware hub last_reply_kind: String("...")
mightex hardware hub last_outcome: String("accepted_unvalidated_reply")
mightex hardware hub last_error: Null
mightex hardware hub last_reply_report_count: I64(...)
mightex hardware hub last_transaction: map keys=[command, command_count, outcome, reply, reply_error, reply_expected, reply_kind, reply_report_count, support_level, wire_terminator]
```

The exact device label, channel count, optional `module_type` transaction key,
overdrive readbacks, command count, and last command/reply depend on the
attached controller. A successful bench run must still include the requested
output, hold duration, software completion, typed readback, and disable result.

## Environment Control

Command:

```sh
cargo run -p numanager-examples -- environment_control
```

Recorded output:

```text
selected environment family: spark_cyto
selected temperature controller: spark-temperature [environment.temperature]
selected gas controller: spark-gas [environment.gas]
capabilities: temperature=TemperatureControl request=TemperatureControl; gas=GasControl request=GasControl
environment property: target type=Temperature writable=true sequenceable=true
environment property: enabled type=Bool writable=true sequenceable=true
environment property: co2_target type=GasConcentration writable=true sequenceable=true
environment property: co2_actual type=GasConcentration writable=false sequenceable=false
environment property: enabled type=Bool writable=true sequenceable=true
environment property: fault type=Bool writable=false sequenceable=false
environment state set completed: map keys=[co2_target, enabled x2, target]
temperature control completed: map keys=[enabled, target]
gas control completed: map keys=[co2_actual, co2_target, enabled]
temperature safety: safe map keys=[device, enabled, state]
gas safety: safe map keys=[device, enabled, fault, state]
target: Temperature(Temperature { value: 36.5, unit: Celsius })
enabled: Bool(true)
co2_target: GasConcentration(GasConcentration { value: 4.5, unit: Percent })
co2_actual: GasConcentration(GasConcentration { value: 4.5, unit: Percent })
enabled: Bool(true)
fault: Bool(false)
event: operation on [spark-temperature, spark-gas] running
event: spark-temperature.target changed to Temperature(Temperature { value: 37.0, unit: Celsius })
event: spark-temperature.enabled changed to Bool(true)
event: spark-gas.co2_target changed to GasConcentration(GasConcentration { value: 5.0, unit: Percent })
event: spark-gas.enabled changed to Bool(true)
event: operation on [spark-temperature, spark-gas] completed map keys=[co2_target, enabled x2, target]
event: operation on [spark-temperature] running
event: spark-temperature.target changed to Temperature(Temperature { value: 36.5, unit: Celsius })
event: spark-temperature.enabled changed to Bool(true)
event: operation on [spark-temperature] completed map keys=[enabled, target]
event: operation on [spark-gas] running
event: spark-gas.co2_target changed to GasConcentration(GasConcentration { value: 4.5, unit: Percent })
event: spark-gas.enabled changed to Bool(true)
event: operation on [spark-gas] completed map keys=[co2_actual, co2_target, enabled]
event: operation on [spark-temperature] running
event: operation on [spark-temperature] completed Bool(true)
event: operation on [spark-gas] running
event: operation on [spark-gas] completed Bool(true)
event: operation on [spark-gas] running
event: operation on [spark-gas] completed Bool(false)
event: operation on [spark-temperature] running
event: operation on [spark-temperature] completed Temperature(Temperature { value: 36.5, unit: Celsius })
event: operation on [spark-temperature] running
event: operation on [spark-temperature] completed Bool(true)
event: operation on [spark-gas] running
event: operation on [spark-gas] completed GasConcentration(GasConcentration { value: 4.5, unit: Percent })
event: operation on [spark-gas] running
event: operation on [spark-gas] completed GasConcentration(GasConcentration { value: 4.5, unit: Percent })
event: operation on [spark-gas] running
event: operation on [spark-gas] completed Bool(true)
event: operation on [spark-gas] running
event: operation on [spark-gas] completed Bool(false)
```

Command for an Andor SDK2 cooler:

```sh
cargo run -p numanager-examples -- environment_control andor_sdk2
```

Recorded output excerpt:

```text
selected environment family: andor_sdk2
selected temperature controller: Configured ANDOR SDK2 cooler [temperature.controller, cooler, state.device]
selected gas controller: none
capabilities: temperature=TemperatureControl request=TemperatureControl; gas=none
environment state set completed: String("-20")
temperature control completed: map keys=[sensor_cooling, temperature_control]
gas control skipped: none
temperature_control: String("-20")
sensor_cooling: Bool(true)
sensor_temperature: Null
temperature_status: String("configured")
```

Command for an Andor SDK3 cooler:

```sh
cargo run -p numanager-examples -- environment_control andor_sdk3
```

Recorded output excerpt:

```text
selected environment family: andor_sdk3
selected temperature controller: Configured ANDOR SDK3 cooler [temperature.controller, cooler, state.device]
selected gas controller: none
capabilities: temperature=TemperatureControl request=TemperatureControl; gas=none
environment state set completed: String("-20")
temperature control completed: map keys=[sensor_cooling, temperature_control]
gas control skipped: none
temperature_control: String("-20")
sensor_cooling: Bool(true)
sensor_temperature: Null
temperature_status: String("configured")
```

Command:

```sh
cargo run -p numanager-examples -- environment_control okolab
```

Recorded output:

```text
selected environment family: okolab
selected temperature controller: Configured Okolab environmental controller temperature [environment.temperature, measure]
selected gas controller: Configured Okolab environmental controller gas [environment.gas, measure]
capabilities: temperature=TemperatureControl request=TemperatureControl; gas=GasControl request=GasControl
environment property: actual type=Temperature writable=false sequenceable=false
environment property: target type=Temperature writable=true sequenceable=false
environment property: enabled type=Bool writable=true sequenceable=false
environment property: status type=String writable=false sequenceable=false
environment property: status_read_code type=I64 writable=false sequenceable=false
environment property: read_code type=I64 writable=false sequenceable=false
environment property: write_code type=I64 writable=false sequenceable=false
environment property: co2_actual type=GasConcentration writable=false sequenceable=false
environment property: co2_target type=GasConcentration writable=true sequenceable=false
environment property: enabled type=Bool writable=true sequenceable=false
environment property: status type=String writable=false sequenceable=false
environment property: co2_status_read_code type=I64 writable=false sequenceable=false
environment property: co2_read_code type=I64 writable=false sequenceable=false
environment property: co2_write_code type=I64 writable=false sequenceable=false
environment state set completed: map keys=[co2_target, enabled x2, target]
temperature control completed: map keys=[actual, completion_basis, enabled, target]
gas control completed: map keys=[co2_actual, co2_target, completion_basis, enabled]
temperature safety: fault map keys=[device, enabled, fault, state, status]
gas safety: fault map keys=[device, enabled, fault, state, status]
target: Temperature(Temperature { value: 36.5, unit: Celsius })
enabled: Bool(true)
co2_target: GasConcentration(GasConcentration { value: 4.5, unit: Percent })
co2_actual: GasConcentration(GasConcentration { value: 5.0, unit: Percent })
enabled: Bool(true)
status: String("unvalidated")
event: operation on [Configured Okolab environmental controller temperature, Configured Okolab environmental controller gas] running
event: operation on [Configured Okolab environmental controller temperature, Configured Okolab environmental controller gas] completed map keys=[co2_target, enabled x2, target]
event: operation on [Configured Okolab environmental controller temperature] running
event: operation on [Configured Okolab environmental controller temperature] completed map keys=[actual, completion_basis, enabled, target]
event: operation on [Configured Okolab environmental controller gas] running
event: operation on [Configured Okolab environmental controller gas] completed map keys=[co2_actual, co2_target, completion_basis, enabled]
event: operation on [Configured Okolab environmental controller temperature] running
event: operation on [Configured Okolab environmental controller temperature] completed Bool(true)
event: operation on [Configured Okolab environmental controller temperature] running
event: operation on [Configured Okolab environmental controller temperature] completed String("unvalidated")
event: operation on [Configured Okolab environmental controller gas] running
event: operation on [Configured Okolab environmental controller gas] completed Bool(true)
event: operation on [Configured Okolab environmental controller gas] running
event: operation on [Configured Okolab environmental controller gas] completed String("unvalidated")
event: operation on [Configured Okolab environmental controller temperature] running
event: operation on [Configured Okolab environmental controller temperature] completed Temperature(Temperature { value: 36.5, unit: Celsius })
event: operation on [Configured Okolab environmental controller temperature] running
event: operation on [Configured Okolab environmental controller temperature] completed Bool(true)
event: operation on [Configured Okolab environmental controller gas] running
event: operation on [Configured Okolab environmental controller gas] completed GasConcentration(GasConcentration { value: 4.5, unit: Percent })
event: operation on [Configured Okolab environmental controller gas] running
event: operation on [Configured Okolab environmental controller gas] completed GasConcentration(GasConcentration { value: 5.0, unit: Percent })
event: operation on [Configured Okolab environmental controller gas] running
event: operation on [Configured Okolab environmental controller gas] completed Bool(true)
event: operation on [Configured Okolab environmental controller gas] running
event: operation on [Configured Okolab environmental controller gas] completed String("unvalidated")
```

## Plate Reader

Command:

```sh
cargo run -p numanager-examples -- plate_reader
```

Recorded output:

```text
selected plate-reader family: spark_cyto
selected plate transport: spark-mainboard [hub, plate.transport]
selected detector: spark-absorbance [detector.absorbance]
selected imaging head: spark-fim [imaging.head, objective.turret]
selected camera binding: spark-camera-binding [camera.binding]
capabilities: plate=PlateMove request=PlateMove; detector=Measure request=Measure; imaging=ImagingHead request=ImagingHead; camera=CameraBinding request=CameraBinding
plate-reader property: spark-mainboard.well type=String writable=true sequenceable=true
plate-reader property: spark-absorbance.wavelength type=Wavelength writable=true sequenceable=true
plate-reader property: spark-fim.objective type=I64 writable=true sequenceable=true
plate-reader property: spark-fim.mode type=String writable=true sequenceable=true
plate-reader property: spark-fim.interlock_closed type=Bool writable=false sequenceable=false
plate-reader property: spark-fim.fault type=Bool writable=false sequenceable=false
plate-reader property: spark-camera-binding.bound type=Bool writable=true sequenceable=true
plate-reader property: spark-camera-binding.imaging_mode type=String writable=true sequenceable=true
plate-reader setup completed: map keys=[bound, imaging_mode, mode, objective, wavelength, well]
plate move completed: map keys=[moved, well]
detector measure completed: map keys=[device, integration_time, signal, wavelength]
imaging head completed: map keys=[mode, objective]
camera binding completed: map keys=[bound, imaging_mode]
plate.well: String("B03")
detector.wavelength: Wavelength(Wavelength { value: 600.0, unit: Nanometers })
imaging.objective: I64(3)
imaging.mode: String("brightfield")
imaging.interlock_closed: Bool(true)
imaging.fault: Bool(false)
camera.bound: Bool(true)
camera.imaging_mode: String("brightfield")
event: operation on [spark-mainboard, spark-absorbance, spark-fim, spark-camera-binding] running
event: spark-mainboard.well changed to String("A01")
event: spark-absorbance.wavelength changed to Wavelength(Wavelength { value: 600.0, unit: Nanometers })
event: spark-fim.objective changed to I64(2)
event: spark-fim.mode changed to String("brightfield")
event: spark-camera-binding.bound changed to Bool(true)
event: spark-camera-binding.imaging_mode changed to String("brightfield")
event: operation on [spark-mainboard, spark-absorbance, spark-fim, spark-camera-binding] completed map keys=[bound, imaging_mode, mode, objective, wavelength, well]
event: operation on [spark-mainboard] running
event: spark-mainboard.well changed to String("B03")
event: operation on [spark-mainboard] completed map keys=[moved, well]
event: operation on [spark-absorbance] running
event: operation on [spark-absorbance] completed map keys=[device, integration_time, signal, wavelength]
event: operation on [spark-fim] running
event: spark-fim.objective changed to I64(3)
event: spark-fim.mode changed to String("brightfield")
event: operation on [spark-fim] completed map keys=[mode, objective]
event: operation on [spark-camera-binding] running
event: spark-camera-binding.bound changed to Bool(true)
event: spark-camera-binding.imaging_mode changed to String("brightfield")
event: operation on [spark-camera-binding] completed map keys=[bound, imaging_mode]
event: operation on [spark-mainboard] running
event: operation on [spark-mainboard] completed String("B03")
event: operation on [spark-absorbance] running
event: operation on [spark-absorbance] completed Wavelength(Wavelength { value: 600.0, unit: Nanometers })
event: operation on [spark-fim] running
event: operation on [spark-fim] completed I64(3)
event: operation on [spark-fim] running
event: operation on [spark-fim] completed String("brightfield")
event: operation on [spark-fim] running
event: operation on [spark-fim] completed Bool(true)
event: operation on [spark-fim] running
event: operation on [spark-fim] completed Bool(false)
event: operation on [spark-camera-binding] running
event: operation on [spark-camera-binding] completed Bool(true)
event: operation on [spark-camera-binding] running
event: operation on [spark-camera-binding] completed String("brightfield")
```

Additional detector variants use the same public workflow:

```sh
cargo run -p numanager-examples -- plate_reader fluorescence
cargo run -p numanager-examples -- plate_reader luminescence
```

Recorded output excerpts:

```text
selected detector: spark-fluorescence [detector.fluorescence, light.source]
plate-reader property: spark-fluorescence.wavelength type=Wavelength writable=true sequenceable=true
plate-reader property: spark-fluorescence.enabled type=Bool writable=true sequenceable=true
plate-reader setup completed: map keys=[bound, enabled, imaging_mode, mode, objective, wavelength, well]
detector measure completed: map keys=[device, enabled, integration_time, signal, wavelength]
detector.wavelength: Wavelength(Wavelength { value: 520.0, unit: Nanometers })
detector.enabled: Bool(true)
imaging.mode: String("fluorescence")
camera.imaging_mode: String("fluorescence")
```

```text
selected detector: spark-luminescence [detector.luminescence]
plate-reader property: spark-luminescence.enabled type=Bool writable=true sequenceable=true
plate-reader setup completed: map keys=[bound, enabled, imaging_mode, mode, objective, well]
detector measure completed: map keys=[device, enabled, integration_time, signal]
detector.enabled: Bool(true)
imaging.mode: String("fluorescence")
camera.imaging_mode: String("fluorescence")
```

## Fluidics

Command:

```sh
cargo run -p numanager-examples -- fluidics
```

Recorded output:

```text
selected fluidics controller: hamilton-mvp-hub [hub, fluidics.controller, hamilton.mvp]
selected valve: hamilton-mvp-valve-a [fluidics.valve, state.device, hamilton.mvp.valve]
capabilities: valve=ValveSelect request=ValveSelect
valve property: position type=I64 writable=true sequenceable=false
valve property: port_count type=I64 writable=false sequenceable=false
valve property: valve_type type=I64 writable=false sequenceable=false
valve property: address type=String writable=false sequenceable=false
valve property: initialized type=Bool writable=false sequenceable=false
valve property: busy type=Bool writable=false sequenceable=false
valve property: valve_error type=Bool writable=false sequenceable=false
valve property: state_summary type=Map writable=false sequenceable=false
valve select completed: map keys=[address, busy, initialized, port_count, position, valve_error, valve_type]
valve state set completed: map keys=[a]
position: I64(5)
port_count: I64(8)
initialized: Bool(true)
busy: Bool(false)
valve_error: Bool(false)
state_summary: map keys=[address, busy, initialized, port_count, position, valve_error, valve_type]
controller last_transaction: map keys=[command, completion_basis, reply_len, response]
event: operation on [hamilton-mvp-valve-a] running
event: hamilton-mvp-valve-a.position changed to I64(3)
event: hamilton-mvp-valve-a.busy changed to Bool(false)
event: operation on [hamilton-mvp-valve-a] completed map keys=[address, busy, initialized, port_count, position, valve_error, valve_type]
event: operation on [hamilton-mvp-valve-a] running
event: hamilton-mvp-valve-a.position changed to I64(5)
event: hamilton-mvp-valve-a.busy changed to Bool(false)
event: operation on [hamilton-mvp-valve-a] completed map keys=[a]
event: operation on [hamilton-mvp-valve-a] running
event: operation on [hamilton-mvp-valve-a] completed I64(5)
event: operation on [hamilton-mvp-valve-a] running
event: operation on [hamilton-mvp-valve-a] completed I64(8)
event: operation on [hamilton-mvp-valve-a] running
event: operation on [hamilton-mvp-valve-a] completed Bool(true)
event: operation on [hamilton-mvp-valve-a] running
event: operation on [hamilton-mvp-valve-a] completed Bool(false)
event: operation on [hamilton-mvp-valve-a] running
event: operation on [hamilton-mvp-valve-a] completed Bool(false)
event: operation on [hamilton-mvp-valve-a] running
event: operation on [hamilton-mvp-valve-a] completed map keys=[address, busy, initialized, port_count, position, valve_error, valve_type]
event: operation on [hamilton-mvp-hub] running
event: operation on [hamilton-mvp-hub] completed map keys=[command, completion_basis, reply_len, response]
```

## Robot Inventory

Command:

```sh
cargo run -p numanager-examples -- robot_inventory opentrons
```

Recorded output:

```text
selected robot inventory source: opentrons (Configured Opentrons OT-2 robot, 6 device(s), 1 resource(s))
resource: opentrons-ot2-http kind=network.http metadata=map keys=[api_version, host, support_level]
added robot inventory driver with 6 device(s)
device: opentrons-ot2-gantry [stage.xyz, motion.robot]
  homed: Bool(false)
  status: String("idle")
device: opentrons-ot2-deck [deck, labware.host]
  loaded_labware: I64(0)
  loaded_modules: I64(1)
device: opentrons-ot2-left-pipette [liquid_handler.pipette, mount.left]
  mount: String("left")
  model: String("p300_single_gen2")
  serial: String("PIP-L-CONFIG-0001")
  has_tip: Bool(false)
device: opentrons-ot2-camera [camera.snapshot, inspection.camera]
  available: Bool(true)
  snapshot_supported: Bool(false)
device: opentrons-ot2-module-1 [module.temperature, module.opentrons]
  model: String("temperatureModuleV2")
  serial: String("TEMP-MOD-CONFIG-0001")
  temperature: Temperature(Temperature { value: 22.0, unit: Celsius })
  target_temperature: Temperature(Temperature { value: 4.0, unit: Celsius })
  status: String("idle")
device: opentrons-ot2 [hub, liquid_handler.robot, network.http]
  host: String("opentrons-ot2.local")
  port: I64(31950)
  api_version: String("2")
  server_version: String("configured")
  robot_serial: String("OT2-CONFIG-0001")
  robot_type: String("OT-2")
  status: String("idle")
  door_open: Bool(false)
  current_run: String("none")
  ready: Bool(true)
```

## Filters

Command:

```sh
cargo run -p numanager-examples -- filters
```

Recorded output:

```text
selected filter family: starlight
selected filter wheel: starlight-xpress-filter-wheel [filter.wheel, state.device]
capabilities: wheel=FilterSelect request=FilterSelect
filter property: product type=String writable=false sequenceable=false
filter property: serial_number type=String writable=false sequenceable=false
filter property: positions type=I64 writable=false sequenceable=false
filter property: position type=I64 writable=true sequenceable=false
filter property: moving type=Bool writable=false sequenceable=false
filter select completed: I64(3)
filter state set completed: map keys=[position]
position: I64(2)
positions: I64(7)
moving: Bool(false)
last_transaction: map keys=[command, completion_basis, moving, position, positions]
event: operation on [starlight-xpress-filter-wheel] running
event: starlight-xpress-filter-wheel.position changed to I64(3)
event: starlight-xpress-filter-wheel.moving changed to Bool(false)
event: operation on [starlight-xpress-filter-wheel] completed I64(3)
event: operation on [starlight-xpress-filter-wheel] running
event: starlight-xpress-filter-wheel.position changed to I64(2)
event: starlight-xpress-filter-wheel.moving changed to Bool(false)
event: operation on [starlight-xpress-filter-wheel] completed map keys=[position]
event: operation on [starlight-xpress-filter-wheel] running
event: operation on [starlight-xpress-filter-wheel] completed I64(2)
event: operation on [starlight-xpress-filter-wheel] running
event: operation on [starlight-xpress-filter-wheel] completed I64(7)
event: operation on [starlight-xpress-filter-wheel] running
event: operation on [starlight-xpress-filter-wheel] completed Bool(false)
event: operation on [starlight-xpress-filter-wheel] running
event: operation on [starlight-xpress-filter-wheel] completed map keys=[command, completion_basis, moving, position, positions]
```

Command for a Prior filter-wheel source:

```sh
cargo run -p numanager-examples -- filters prior
```

Recorded output:

```text
selected filter family: prior
selected filter wheel: prior-filter-wheel-1 [filter.wheel, state.device]
capabilities: wheel=FilterSelect request=FilterSelect
filter property: position type=I64 writable=true sequenceable=false
filter property: busy type=Bool writable=false sequenceable=false
filter select completed: I64(3)
filter state set completed: map keys=[position]
position: I64(2)
event: operation on [prior-filter-wheel-1] running
event: prior-filter-wheel-1.position changed to I64(3)
event: operation on [prior-filter-wheel-1] completed I64(3)
event: operation on [prior-filter-wheel-1] running
event: prior-filter-wheel-1.position changed to I64(2)
event: operation on [prior-filter-wheel-1] completed map keys=[position]
event: operation on [prior-filter-wheel-1] running
event: operation on [prior-filter-wheel-1] completed I64(2)
```

Command for an IX85 microscope-body filter selector:

```sh
cargo run -p numanager-examples -- filters ix85
```

Recorded output excerpt:

```text
selected filter family: ix85
selected filter selector: ix85-nosepiece [objective.turret, state.device]
capabilities: wheel=FilterSelect request=FilterSelect
filter property: nosepiece_position type=I64 writable=true sequenceable=true
filter property: minimum_position type=I64 writable=false sequenceable=false
filter property: maximum_position type=I64 writable=false sequenceable=false
filter select completed: I64(3)
filter state set completed: map keys=[nosepiece_position]
nosepiece_position: I64(2)
event: operation on [ix85-nosepiece] running
event: operation on [ix85-nosepiece] completed I64(3)
event: operation on [ix85-nosepiece] running
event: operation on [ix85-nosepiece] completed map keys=[nosepiece_position]
event: operation on [ix85-nosepiece] running
event: operation on [ix85-nosepiece] completed I64(2)
```

Command for the Thorlabs KURIOS selector:

```sh
cargo run -p numanager-examples -- filters kurios
```

Recorded output excerpt:

```text
selected filter family: kurios
selected tunable filter: thorlabs-kurios-lctf [filter.tunable, lctf, light.filter, serial.ascii]
capabilities: tunable=TriggerSink request=Trigger
filter property: wavelength type=Wavelength writable=true sequenceable=true
filter property: bandwidth type=Wavelength writable=true sequenceable=true
filter property: output_enabled type=Bool writable=true sequenceable=true
tunable filter state set completed: map keys=[bandwidth, output_enabled, wavelength]
tunable filter disable completed: map keys=[output_enabled, steps]
tunable filter timing arm completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
tunable filter timing start completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
tunable filter timing stop completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
wavelength: Wavelength(Wavelength { value: 520.0, unit: Nanometers })
bandwidth: Wavelength(Wavelength { value: 20.0, unit: Nanometers })
output_enabled: Bool(false)
event: thorlabs-kurios-lctf.wavelength changed to Wavelength(Wavelength { value: 520.0, unit: Nanometers })
event: thorlabs-kurios-lctf.output_enabled changed to Bool(false)
```

## Shutter

Command:

```sh
cargo run -p numanager-examples -- shutter
```

Recorded output:

```text
selected shutter family: sc10
selected shutter: thorlabs-sc10-shutter [shutter, light.gate, trigger.sink]
capabilities: shutter=TriggerSink request=Trigger
shutter property: enabled type=Bool writable=true sequenceable=true
shutter property: mode type=String writable=true sequenceable=true
shutter property: open_time type=TimeInterval writable=true sequenceable=true
shutter property: close_time type=TimeInterval writable=true sequenceable=true
shutter property: trigger_mode type=String writable=true sequenceable=true
shutter property: repeat_count type=I64 writable=true sequenceable=true
shutter property: interlock_closed type=Bool writable=false sequenceable=false
shutter property: fault type=Bool writable=false sequenceable=false
shutter property: state_summary type=Map writable=false sequenceable=false
shutter safety: safe map keys=[device, enabled, fault, fault_active, interlock_closed, state]
shutter setup completed: map keys=[close_time, mode, open_time, repeat_count, trigger_mode]
shutter open completed: Bool(true)
shutter pulse completed: Bool(false)
shutter close completed: Bool(false)
enabled: Bool(false)
mode: String("Manual")
open_time: TimeInterval(TimeInterval { value: 15.0, unit: Milliseconds })
close_time: TimeInterval(TimeInterval { value: 15.0, unit: Milliseconds })
trigger_mode: String("Internal")
repeat_count: I64(1)
interlock_closed: Bool(true)
fault: Bool(false)
state_summary: map keys=[enabled, fault, interlock_closed, mode, trigger_mode]
event: operation on [thorlabs-sc10-shutter] running
event: thorlabs-sc10-shutter.mode changed to String("Manual")
event: thorlabs-sc10-shutter.open_time changed to TimeInterval(TimeInterval { value: 15.0, unit: Milliseconds })
event: thorlabs-sc10-shutter.close_time changed to TimeInterval(TimeInterval { value: 15.0, unit: Milliseconds })
event: thorlabs-sc10-shutter.trigger_mode changed to String("Internal")
event: thorlabs-sc10-shutter.repeat_count changed to I64(1)
event: operation on [thorlabs-sc10-shutter] completed map keys=[close_time, mode, open_time, repeat_count, trigger_mode]
event: operation on [thorlabs-sc10-shutter] running
event: thorlabs-sc10-shutter.enabled changed to Bool(true)
event: operation on [thorlabs-sc10-shutter] completed Bool(true)
event: operation on [thorlabs-sc10-shutter] running
event: thorlabs-sc10-shutter.enabled changed to Bool(true)
event: thorlabs-sc10-shutter.enabled changed to Bool(false)
event: operation on [thorlabs-sc10-shutter] completed Bool(false)
event: operation on [thorlabs-sc10-shutter] running
event: thorlabs-sc10-shutter.enabled changed to Bool(false)
event: operation on [thorlabs-sc10-shutter] completed Bool(false)
event: operation on [thorlabs-sc10-shutter] running
event: operation on [thorlabs-sc10-shutter] completed Bool(false)
event: operation on [thorlabs-sc10-shutter] running
event: operation on [thorlabs-sc10-shutter] completed String("Manual")
event: operation on [thorlabs-sc10-shutter] running
event: operation on [thorlabs-sc10-shutter] completed TimeInterval(TimeInterval { value: 15.0, unit: Milliseconds })
event: operation on [thorlabs-sc10-shutter] running
event: operation on [thorlabs-sc10-shutter] completed TimeInterval(TimeInterval { value: 15.0, unit: Milliseconds })
event: operation on [thorlabs-sc10-shutter] running
event: operation on [thorlabs-sc10-shutter] completed String("Internal")
event: operation on [thorlabs-sc10-shutter] running
event: operation on [thorlabs-sc10-shutter] completed I64(1)
event: operation on [thorlabs-sc10-shutter] running
event: operation on [thorlabs-sc10-shutter] completed Bool(true)
event: operation on [thorlabs-sc10-shutter] running
event: operation on [thorlabs-sc10-shutter] completed Bool(false)
event: operation on [thorlabs-sc10-shutter] running
event: operation on [thorlabs-sc10-shutter] completed map keys=[enabled, fault, interlock_closed, mode, trigger_mode]
```

Command for the ESP32 shutter endpoint:

```sh
cargo run -p numanager-examples -- shutter esp32
```

Recorded output excerpt:

```text
selected shutter family: esp32
selected shutter: esp32-shutter [shutter, trigger.sink]
capabilities: shutter=TriggerSink request=Trigger
shutter property: open type=Bool writable=true sequenceable=true
shutter safety: unknown map keys=[device, state]
shutter setup skipped: no generic setup properties
shutter open completed: map keys=[open, triggered]
shutter pulse completed: map keys=[open, triggered]
shutter close completed: map keys=[open, triggered]
open: Bool(false)
event: esp32-shutter.open changed to Bool(true)
event: esp32-shutter.open changed to Bool(false)
```

Command for an IX85 microscope-body shutter:

```sh
cargo run -p numanager-examples -- shutter ix85
```

Recorded output excerpt:

```text
selected shutter family: ix85
selected shutter: ix85-dia-shutter [shutter, light.gate, state.device]
capabilities: shutter=TriggerSink request=Trigger
shutter property: dia_shutter_open type=Bool writable=true sequenceable=true
shutter safety: unknown map keys=[device, state]
shutter setup skipped: no generic setup properties
shutter open completed: Bool(true)
shutter pulse completed: Bool(false)
shutter close completed: Bool(false)
dia_shutter_open: Bool(false)
```

## Discovery Flow

Command:

```sh
cargo run -p numanager-examples -- discover_devices
```

Recorded output:

```text
detected 66 candidate driver(s)
0: Simulated Toupcam camera, 1 device(s), 2 resource(s)
    device: toupcam-0 ["camera", "trigger.sink"]
1: Configured Toupcam camera Configured Toupcam geometry, 1 device(s), 2 resource(s)
    device: Configured Toupcam geometry ["camera", "trigger.sink"]
2: Simulated Spark Cyto, 8 device(s), 3 resource(s)
    device: spark-mainboard ["hub", "plate.transport"]
    device: spark-absorbance ["detector.absorbance"]
    device: spark-fluorescence ["detector.fluorescence", "light.source"]
    device: spark-luminescence ["detector.luminescence"]
    device: spark-temperature ["environment.temperature"]
    device: spark-gas ["environment.gas"]
    device: spark-fim ["imaging.head", "objective.turret"]
    device: spark-camera-binding ["camera.binding"]
2: Simulated Cephla Squid controller, 25 device(s), 1 resource(s)
    device: squid-controller ["hub", "serial.controller"]
    device: squid-xy-stage ["stage.xy"]
    device: squid-z-stage ["stage.z"]
    device: squid-theta ["stage.theta"]
    device: squid-filter-wheel-w ["filter.wheel"]
    device: squid-filter-wheel-w2 ["filter.wheel"]
    device: squid-led-matrix ["light.source", "illumination.matrix"]
    device: squid-autofocus ["autofocus"]
    device: squid-illumination-d1 ["light.source", "illumination.port"]
    device: squid-illumination-d2 ["light.source", "illumination.port"]
    device: squid-illumination-d3 ["light.source", "illumination.port"]
    device: squid-illumination-d4 ["light.source", "illumination.port"]
    device: squid-illumination-d5 ["light.source", "illumination.port"]
    device: squid-trigger-1 ["trigger.source", "camera.trigger"]
    device: squid-trigger-2 ["trigger.source", "camera.trigger"]
    device: squid-trigger-3 ["trigger.source", "camera.trigger"]
    device: squid-trigger-4 ["trigger.source", "camera.trigger"]
    device: squid-onboard-dac-1 ["analog.output"]
    device: squid-onboard-dac-2 ["analog.output"]
    device: squid-onboard-dac-3 ["analog.output"]
    device: squid-onboard-dac-4 ["analog.output"]
    device: squid-onboard-dac-5 ["analog.output"]
    device: squid-onboard-dac-6 ["analog.output"]
    device: squid-onboard-dac-7 ["analog.output"]
    device: squid-onboard-dac-8 ["analog.output"]
3: Simulated ASI MS-2000 controller, 3 device(s), 1 resource(s)
    device: asi-ms2000-hub ["hub", "motion.controller", "serial.ascii"]
    device: asi-ms2000-xy ["axis.xy", "stage.xy"]
    device: asi-ms2000-z ["axis.z", "stage.z"]
4: Simulated ASI Tiger controller, 6 device(s), 1 resource(s)
    device: asi-tiger-hub ["hub", "motion.controller", "serial.ascii", "asi.tiger"]
    device: asi-tiger-xy ["axis.xy", "stage.xy", "asi.tiger.card"]
    device: asi-tiger-z ["axis.z", "stage.z", "asi.tiger.card"]
    device: asi-tiger-ttl ["digital.output", "trigger.source", "asi.tiger.card"]
    device: asi-tiger-ring-buffer ["motion.program", "ring.buffer", "asi.tiger.card"]
    device: asi-tiger-crisp-autofocus ["autofocus", "continuous.focus", "asi.crisp", "asi.tiger.card"]
5: Simulated Cobolt serial laser, 1 device(s), 1 resource(s)
    device: cobolt-laser ["laser", "light.source", "shutter", "trigger.sink", "serial.ascii"]
6: Simulated CoolLED pE-4000, 5 device(s), 1 resource(s)
    device: coolled-pe4000-hub ["hub", "light.engine", "shutter"]
    device: coolled-pe4000-channel-1 ["light.source", "led.channel", "trigger.sink"]
    device: coolled-pe4000-channel-2 ["light.source", "led.channel", "trigger.sink"]
    device: coolled-pe4000-channel-3 ["light.source", "led.channel", "trigger.sink"]
    device: coolled-pe4000-channel-4 ["light.source", "led.channel", "trigger.sink"]
7: Simulated CoolLED pE-300, 4 device(s), 1 resource(s)
    device: coolled-pe300-hub ["hub", "light.engine", "shutter"]
    device: coolled-pe300-channel-1 ["light.source", "led.channel", "trigger.sink"]
    device: coolled-pe300-channel-2 ["light.source", "led.channel", "trigger.sink"]
    device: coolled-pe300-channel-3 ["light.source", "led.channel", "trigger.sink"]
8: Simulated Zaber ASCII stage, 2 device(s), 1 resource(s)
    device: zaber-ascii-hub ["hub", "motion.controller", "serial.ascii"]
    device: zaber-ascii-axis-1 ["axis.1", "stage.axis", "stage.x"]
9: Simulated Coherent OBIS laser, 1 device(s), 1 resource(s)
    device: coherent-obis-laser ["laser", "light.source", "shutter", "trigger.sink", "serial.scpi"]
10: Simulated Omicron serial laser, 1 device(s), 1 resource(s)
    device: omicron-serial-laser ["laser", "light.source", "shutter", "trigger.sink", "serial.ascii"]
11: Simulated Prior ProScan controller, 8 device(s), 1 resource(s)
    device: prior-proscan-hub ["hub", "motion.controller", "serial.ascii"]
    device: prior-xy-stage ["axis.xy", "stage.xy"]
    device: prior-z-stage ["axis.z", "stage.z"]
    device: prior-filter-wheel-1 ["filter.wheel", "state.device"]
    device: prior-shutter-1 ["shutter", "light.gate", "trigger.sink"]
    device: prior-ttl-0 ["trigger.source", "digital.output"]
    device: prior-nanoscan-z ["axis.z", "stage.z", "piezo.z"]
    device: prior-lumen-200pro ["light.source", "shutter", "lamp", "trigger.sink"]
12: Simulated SutterStage controller, 4 device(s), 1 resource(s)
    device: sutter-stage-hub ["hub", "motion.controller", "serial.ascii"]
    device: sutter-xy-stage ["axis.xy", "stage.xy"]
    device: sutter-z-stage ["axis.z", "stage.z"]
    device: sutter-autofocus ["autofocus", "sutter.af"]
13: Simulated Sutter MP-285 manipulator, 3 device(s), 1 resource(s)
    device: sutter-mp285-hub ["hub", "motion.controller", "serial.binary"]
    device: sutter-mp285-xy ["stage.xy", "axis.x", "axis.y"]
    device: sutter-mp285-z ["stage.z", "axis.z"]
14: Simulated Marzhauser L-Step/TANGO controller, 3 device(s), 1 resource(s)
    device: marzhauser-hub ["hub", "motion.controller", "serial.ascii"]
    device: marzhauser-xy-stage ["axis.xy", "stage.xy"]
    device: marzhauser-z-stage ["axis.z", "stage.z"]
15: Configured PI GCS controller fixture, 3 device(s), 1 resource(s)
    device: pi-gcs-hub ["hub", "motion.controller", "serial.ascii"]
    device: pi-gcs-xy-stage ["axis.xy", "stage.xy"]
    device: pi-gcs-z-stage ["axis.z", "stage.z"]
16: Configured Thorlabs APT motor fixture, 2 device(s), 1 resource(s)
    device: thorlabs-apt-hub ["hub", "motion.controller", "binary.apt"]
    device: thorlabs-apt-axis-1 ["axis.x", "stage.x", "motion.apt"]
17: Configured Lumencor SpectraX fixture, 7 device(s), 1 resource(s)
    device: lumencor-spectra-hub ["hub", "light.engine", "shutter"]
    device: lumencor-red ["light.source", "led.channel", "trigger.sink"]
    device: lumencor-green ["light.source", "led.channel", "trigger.sink"]
    device: lumencor-cyan ["light.source", "led.channel", "trigger.sink"]
    device: lumencor-violet ["light.source", "led.channel", "trigger.sink"]
    device: lumencor-blue ["light.source", "led.channel", "trigger.sink"]
    device: lumencor-teal ["light.source", "led.channel", "trigger.sink"]
18: Configured Lumencor CIA fixture, 1 device(s), 1 resource(s)
    device: lumencor-cia ["trigger.controller", "pulse.program", "light.engine.adapter"]
19: Configured Thorlabs DC2010/DC2100 LED controller, 1 device(s), 1 resource(s)
    device: thorlabs-dc-led ["led.controller", "light.source", "shutter", "trigger.sink"]
20: Configured Thorlabs DC3100 LED controller, 1 device(s), 1 resource(s)
    device: thorlabs-dc3100-led ["led.controller", "light.source", "shutter", "trigger.sink"]
21: Configured Thorlabs DC2200 SCPI LED controller, 1 device(s), 1 resource(s)
    device: thorlabs-dc2200-led ["led.controller", "light.source", "shutter", "trigger.sink"]
22: Configured Thorlabs DC4100/DC4104 LED controller, 5 device(s), 1 resource(s)
    device: thorlabs-dc4100-hub ["led.controller", "light.source", "shutter", "trigger.sink"]
    device: thorlabs-dc4100-led-1 ["light.source", "led.channel", "trigger.sink"]
    device: thorlabs-dc4100-led-2 ["light.source", "led.channel", "trigger.sink"]
    device: thorlabs-dc4100-led-3 ["light.source", "led.channel", "trigger.sink"]
    device: thorlabs-dc4100-led-4 ["light.source", "led.channel", "trigger.sink"]
23: Configured Thorlabs DC4100 LED controller, 5 device(s), 1 resource(s)
    device: thorlabs-dc4100-hub ["led.controller", "light.source", "shutter", "trigger.sink"]
    device: thorlabs-dc4100-led-1 ["light.source", "led.channel", "trigger.sink"]
    device: thorlabs-dc4100-led-2 ["light.source", "led.channel", "trigger.sink"]
    device: thorlabs-dc4100-led-3 ["light.source", "led.channel", "trigger.sink"]
    device: thorlabs-dc4100-led-4 ["light.source", "led.channel", "trigger.sink"]
24: Configured Modbus IO fixture, 1 device(s), 1 resource(s)
    device: modbus-mapped-io ["mapped.io", "modbus"]
25: Configured Mightex Sirius BLS, 5 device(s), 1 resource(s)
    device: mightex-bls-hub ["hub", "light.engine", "hid.device"]
    device: mightex-bls-channel-1 ["light.source", "led.channel", "trigger.sink"]
    device: mightex-bls-channel-2 ["light.source", "led.channel", "trigger.sink"]
    device: mightex-bls-channel-3 ["light.source", "led.channel", "trigger.sink"]
    device: mightex-bls-channel-4 ["light.source", "led.channel", "trigger.sink"]
26: Configured Thorlabs APT stage, 2 device(s), 1 resource(s)
    device: thorlabs-apt-hub ["hub", "motion.controller", "binary.apt"]
    device: thorlabs-apt-axis-1 ["axis.x", "stage.x", "motion.apt"]
27: Configured GenICam node map genicam-local-camera, 1 device(s), 1 resource(s)
    device: genicam-local-camera ["camera", "genicam", "genicam.node_map", "fixture"]
28: Configured GenICam node map Configured GenICam local node-map camera, 1 device(s), 1 resource(s)
    device: Configured GenICam local node-map camera ["camera", "genicam", "genicam.node_map", "fixture"]
29: Configured GigE Vision camera Configured GigE Vision fixture camera, 1 device(s), 2 resource(s)
    device: Configured GigE Vision fixture camera ["camera", "gige.vision", "genicam.transport", "trigger.sink", "trigger.source"]
30: Configured USB3 Vision camera Configured USB3 Vision fixture camera, 1 device(s), 3 resource(s)
    device: Configured USB3 Vision fixture camera ["camera", "usb3.vision", "genicam.transport", "trigger.sink", "trigger.source"]
31: Configured Thorlabs KURIOS LCTF fixture, 1 device(s), 1 resource(s)
    device: thorlabs-kurios-lctf ["filter.tunable", "lctf", "light.filter", "serial.ascii"]
32: Configured Thorlabs KURIOS filter, 1 device(s), 1 resource(s)
    device: thorlabs-kurios-lctf ["filter.tunable", "lctf", "light.filter", "serial.ascii"]
33: Configured platform camera Configured platform fixture camera (fixture), 1 device(s), 1 resource(s)
    device: Configured platform fixture camera ["camera", "platform.camera", "trigger.sink", "trigger.source"]
34: Configured Mightex Sirius SLC, 3 device(s), 1 resource(s)
    device: mightex-slc-hub ["hub", "light.engine", "hid.device"]
    device: mightex-slc-channel-1 ["light.source", "led.channel", "trigger.sink"]
    device: mightex-slc-channel-2 ["light.source", "led.channel", "trigger.sink"]
35: Standa 8SMC4 8SMC4-USB STANDA-CONFIG-0002, 2 device(s), 1 resource(s)
    device: standa-8smc4-hub ["hub", "motion.controller", "standa.8smc4"]
    device: standa-8smc4-x ["axis.x", "stage.1d", "standa.8smc4.axis"]
36: Configured Hamilton Serial MVP valve, 2 device(s), 1 resource(s)
    device: hamilton-mvp-hub ["hub", "fluidics.controller", "hamilton.mvp"]
    device: hamilton-mvp-valve ["fluidics.valve", "state.device", "hamilton.mvp.valve"]
37: Configured Trinamic TMCL stage controller, 4 device(s), 1 resource(s)
    device: trinamic-tmcl-hub ["hub", "motion.controller", "trinamic.tmcl"]
    device: trinamic-tmcl-x-stage ["stage.1d", "motion.stage", "state.device", "trinamic.tmcl.axis"]
    device: trinamic-tmcl-y-stage ["stage.1d", "motion.stage", "state.device", "trinamic.tmcl.axis"]
    device: trinamic-tmcl-z-stage ["stage.1d", "motion.stage", "state.device", "trinamic.tmcl.axis"]
38: Configured Velleman K8055 IO board, 9 device(s), 1 resource(s)
    device: velleman-k8055-hub ["hub", "usb.io", "velleman.k8055"]
    device: velleman-k8055-digital-input ["digital.input", "state.device"]
    device: velleman-k8055-digital-output ["digital.output", "state.device"]
    device: velleman-k8055-analog-input-1 ["analog.input", "adc"]
    device: velleman-k8055-analog-input-2 ["analog.input", "adc"]
    device: velleman-k8055-counter-1 ["counter", "digital.input.counter"]
    device: velleman-k8055-counter-2 ["counter", "digital.input.counter"]
    device: velleman-k8055-analog-output-1 ["analog.output", "dac"]
    device: velleman-k8055-analog-output-2 ["analog.output", "dac"]
39: Configured Velleman K8061 IO board, 22 device(s), 1 resource(s)
    device: velleman-k8061-hub ["hub", "usb.io", "velleman.k8061"]
    device: velleman-k8061-digital-input ["digital.input", "state.device"]
    device: velleman-k8061-digital-output ["digital.output", "state.device"]
    device: velleman-k8061-analog-input-1 ["analog.input", "adc"]
    device: velleman-k8061-analog-input-2 ["analog.input", "adc"]
    device: velleman-k8061-analog-input-3 ["analog.input", "adc"]
    device: velleman-k8061-analog-input-4 ["analog.input", "adc"]
    device: velleman-k8061-analog-input-5 ["analog.input", "adc"]
    device: velleman-k8061-analog-input-6 ["analog.input", "adc"]
    device: velleman-k8061-analog-input-7 ["analog.input", "adc"]
    device: velleman-k8061-analog-input-8 ["analog.input", "adc"]
    device: velleman-k8061-counter-1 ["counter", "digital.input.counter"]
    device: velleman-k8061-counter-2 ["counter", "digital.input.counter"]
    device: velleman-k8061-analog-output-1 ["analog.output", "dac"]
    device: velleman-k8061-analog-output-2 ["analog.output", "dac"]
    device: velleman-k8061-analog-output-3 ["analog.output", "dac"]
    device: velleman-k8061-analog-output-4 ["analog.output", "dac"]
    device: velleman-k8061-analog-output-5 ["analog.output", "dac"]
    device: velleman-k8061-analog-output-6 ["analog.output", "dac"]
    device: velleman-k8061-analog-output-7 ["analog.output", "dac"]
    device: velleman-k8061-analog-output-8 ["analog.output", "dac"]
    device: velleman-k8061-pwm-output ["pwm.output", "dac"]
40: Configured Starlight Xpress filter wheel, 1 device(s), 1 resource(s)
    device: starlight-xpress-filter-wheel ["filter.wheel", "state.device"]
41: Configured Spectral LMM5, 6 device(s), 1 resource(s)
    device: spectral-lmm5-hub ["hub", "light.engine", "serial.ascii.hex"]
    device: spectral-lmm5-line-1 ["light.source", "laser.line", "shutter", "trigger.sink"]
    device: spectral-lmm5-line-2 ["light.source", "laser.line", "shutter", "trigger.sink"]
    device: spectral-lmm5-line-3 ["light.source", "laser.line", "shutter", "trigger.sink"]
    device: spectral-lmm5-line-4 ["light.source", "laser.line", "shutter", "trigger.sink"]
    device: spectral-lmm5-line-5 ["light.source", "laser.line", "shutter", "trigger.sink"]
42: Configured OpenStage controller, 3 device(s), 1 resource(s)
    device: openstage-hub ["hub", "motion.controller", "serial.ascii"]
    device: openstage-xy ["axis.xy", "stage.xy", "motion.stage"]
    device: openstage-z ["axis.z", "stage.z", "motion.stage"]
43: Configured WOSM controller, 10 device(s), 1 resource(s)
    device: wosm-hub ["hub", "microscope.controller", "tcp.text"]
    device: wosm-switch ["digital.output", "state.device", "trigger.source"]
    device: wosm-shutter ["shutter", "light.gate", "trigger.sink"]
    device: wosm-xy-stage ["axis.xy", "stage.xy", "motion.stage"]
    device: wosm-z-stage ["axis.z", "stage.z", "motion.stage"]
    device: wosm-input ["digital.input", "analog.input", "state.device"]
    device: wosm-light-1 ["light.source", "dac.output", "trigger.sink"]
    device: wosm-light-2 ["light.source", "dac.output", "trigger.sink"]
    device: wosm-light-3 ["light.source", "dac.output", "trigger.sink"]
    device: wosm-light-4 ["light.source", "dac.output", "trigger.sink"]
44: Configured TriggerScope controller, 12 device(s), 1 resource(s)
    device: triggerscope-hub ["hub", "trigger.controller", "serial.ascii"]
    device: triggerscope-focus ["axis.z", "stage.z", "motion.stage"]
    device: triggerscope-cam-1 ["camera.trigger", "trigger.source", "state.device"]
    device: triggerscope-cam-2 ["camera.trigger", "trigger.source", "state.device"]
    device: triggerscope-ttl-1 ["digital.output", "ttl.output", "trigger.source", "trigger.sink"]
    device: triggerscope-ttl-2 ["digital.output", "ttl.output", "trigger.source", "trigger.sink"]
    device: triggerscope-ttl-3 ["digital.output", "ttl.output", "trigger.source", "trigger.sink"]
    device: triggerscope-ttl-4 ["digital.output", "ttl.output", "trigger.source", "trigger.sink"]
    device: triggerscope-dac-1 ["analog.output", "dac.output", "trigger.sink"]
    device: triggerscope-dac-2 ["analog.output", "dac.output", "trigger.sink"]
    device: triggerscope-dac-3 ["analog.output", "dac.output", "trigger.sink"]
    device: triggerscope-dac-4 ["analog.output", "dac.output", "trigger.sink"]
45: Configured Chuo Seiki QT controller, 3 device(s), 1 resource(s)
    device: chuo-qt-hub ["hub", "motion.controller", "serial.ascii"]
    device: chuo-qt-xy-stage ["axis.xy", "stage.xy", "motion.stage"]
    device: chuo-qt-z-stage ["axis.z", "stage.z", "motion.stage"]
46: Configured ITK Corvus controller, 3 device(s), 1 resource(s)
    device: corvus-hub ["hub", "motion.controller", "serial.ascii"]
    device: corvus-xy-stage ["axis.xy", "stage.xy", "motion.stage"]
    device: corvus-z-stage ["axis.z", "stage.z", "motion.stage"]
47: Configured Bluebox Optics niji, 8 device(s), 1 resource(s)
    device: niji-hub ["hub", "light.engine", "shutter", "serial.ascii"]
    device: niji-channel-1 ["light.source", "led.channel", "trigger.sink"]
    device: niji-channel-2 ["light.source", "led.channel", "trigger.sink"]
    device: niji-channel-3 ["light.source", "led.channel", "trigger.sink"]
    device: niji-channel-4 ["light.source", "led.channel", "trigger.sink"]
    device: niji-channel-5 ["light.source", "led.channel", "trigger.sink"]
    device: niji-channel-6 ["light.source", "led.channel", "trigger.sink"]
    device: niji-channel-7 ["light.source", "led.channel", "trigger.sink"]
48: Configured Opentrons OT-2 robot, 6 device(s), 1 resource(s)
    device: opentrons-ot2 ["hub", "liquid_handler.robot", "network.http"]
    device: opentrons-ot2-gantry ["stage.xyz", "motion.robot"]
    device: opentrons-ot2-deck ["deck", "labware.host"]
    device: opentrons-ot2-left-pipette ["liquid_handler.pipette", "mount.left"]
    device: opentrons-ot2-camera ["camera.snapshot", "inspection.camera"]
    device: opentrons-ot2-module-1 ["module.temperature", "module.opentrons"]
49: Configured Thorlabs SC10 shutter controller, 2 device(s), 1 resource(s)
    device: thorlabs-sc10-controller ["hub", "shutter.controller", "serial.ascii"]
    device: thorlabs-sc10-shutter ["shutter", "light.gate", "trigger.sink"]
50: Configured CoolLED pE-340, 5 device(s), 1 resource(s)
    device: coolled-pe340-hub ["hub", "light.engine", "shutter"]
    device: coolled-pe340-channel-1 ["light.source", "led.channel", "trigger.sink"]
    device: coolled-pe340-channel-2 ["light.source", "led.channel", "trigger.sink"]
    device: coolled-pe340-channel-3 ["light.source", "led.channel", "trigger.sink"]
    device: coolled-pe340-channel-4 ["light.source", "led.channel", "trigger.sink"]
52: Configured Andor SDK2 camera (136e:0012), 3 device(s), 3 resource(s)
    device: Configured Andor SDK2 camera hub ["hub", "usb.camera", "camera.controller"]
    device: Configured Andor SDK2 camera ["camera", "camera.scientific", "detector.mono", "andor.sdk2"]
    device: Configured Andor SDK2 camera cooler ["temperature.controller", "cooler", "state.device"]
53: Configured Andor SDK3 camera (136e:0014), 3 device(s), 3 resource(s)
    device: Configured Andor SDK3 camera hub ["hub", "usb.camera", "camera.controller"]
    device: Configured Andor SDK3 camera ["camera", "camera.scientific", "detector.mono", "andor.sdk3"]
    device: Configured Andor SDK3 camera cooler ["temperature.controller", "cooler", "state.device"]
54: Configured Photometrics PVCAM camera (PVCAM-CONFIG-0002), 3 device(s), 2 resource(s)
    device: Configured Photometrics PVCAM camera hub ["hub", "camera.controller", "pvcam"]
    device: Configured Photometrics PVCAM camera ["camera", "camera.scientific", "detector.mono", "pvcam"]
    device: Configured Photometrics PVCAM camera cooler ["temperature.controller", "cooler", "state.device"]
55: Configured Evident IX85 microscope body (IX85-CONFIG-0002), 8 device(s), 1 resource(s)
    device: ix85-hub ["hub", "microscope.body", "serial.ascii"]
    device: ix85-focus ["axis.z", "stage.z", "microscope.focus"]
    device: ix85-nosepiece ["objective.turret", "state.device"]
    device: ix85-light-path ["light.path", "state.device"]
    device: ix85-mirror-unit-1 ["filter.cube", "mirror.unit", "state.device"]
    device: ix85-dia-shutter ["shutter", "light.gate", "state.device"]
    device: ix85-epi-shutter-1 ["shutter", "light.gate", "state.device"]
    device: ix85-zdc-autofocus ["autofocus", "zdc", "state.device"]
56: Configured Okolab environmental controller (H201 T Unit-BL), 3 device(s), 2 resource(s)
    device: Configured Okolab environmental controller hub ["hub", "environment.controller", "serial.device"]
    device: Configured Okolab environmental controller temperature ["environment.temperature", "measure"]
    device: Configured Okolab environmental controller gas ["environment.gas", "measure"]
57: Configured ABS camera reverse engineered support (ABS CamUSB camera), 1 device(s), 1 resource(s)
    device: Configured ABS camera reverse engineered support ["camera", "reverse.engineered"]
58: Configured Mightex camera reverse engineered support (Mightex buffered USB camera), 1 device(s), 2 resource(s)
    device: Configured Mightex camera reverse engineered support ["camera", "reverse.engineered"]
59: Configured MCL reverse engineered support (Mad City Labs MicroDrive/NanoDrive), 4 device(s), 1 resource(s)
    device: Configured MCL reverse engineered support ["hub", "motion.controller", "reverse.engineered"]
    device: mcl-x ["stage.axis", "stage.x", "reverse.engineered"]
    device: mcl-y ["stage.axis", "stage.y", "reverse.engineered"]
    device: mcl-z ["stage.axis", "stage.z", "reverse.engineered"]
60: Configured Agilent Laser Combiner, 9 device(s), 1 resource(s)
    device: agilent-combiner-hub ["hub", "light.engine"]
    device: agilent-laser-line-1 ["light.source", "laser", "trigger.sink"]
    device: agilent-laser-line-2 ["light.source", "laser", "trigger.sink"]
    device: agilent-laser-line-3 ["light.source", "laser", "trigger.sink"]
    device: agilent-laser-line-4 ["light.source", "laser", "trigger.sink"]
    device: agilent-analog-output-1 ["analog.output"]
    device: agilent-analog-output-2 ["analog.output"]
    device: agilent-analog-output-3 ["analog.output"]
    device: agilent-analog-output-4 ["analog.output"]
61: Configured Arduino controller, 5 device(s), 1 resource(s)
    device: arduino-hub ["hub", "microcontroller"]
    device: arduino-digital-out ["digital.io", "trigger.source"]
    device: arduino-shutter ["shutter", "trigger.sink"]
    device: arduino-adc ["analog.input"]
    device: arduino-dac ["analog.output"]
62: Configured Arduino Counter, 3 device(s), 1 resource(s)
    device: arduino-counter-hub ["hub", "microcontroller"]
    device: arduino-counter ["counter", "timing.source"]
    device: arduino-counter-pulse ["trigger.source", "pulse.generator"]
63: Configured ESP32 controller, 7 device(s), 1 resource(s)
    device: esp32-hub ["hub", "microcontroller"]
    device: esp32-digital-out ["digital.io", "trigger.source"]
    device: esp32-shutter ["shutter", "trigger.sink"]
    device: esp32-pwm ["analog.output", "pwm"]
    device: esp32-adc ["analog.input", "adc"]
    device: esp32-xy ["axis.xy"]
    device: esp32-z ["axis.z"]
64: Configured OpenUC2 Feather controller, 4 device(s), 1 resource(s)
    device: openuc2-hub ["hub", "microcontroller"]
    device: openuc2-xy ["axis.xy"]
    device: openuc2-z ["axis.z"]
    device: openuc2-laser ["light.source", "shutter", "trigger.sink"]
65: Configured Teensy pulse generator, 2 device(s), 1 resource(s)
    device: teensy-pulse-hub ["hub", "microcontroller"]
    device: teensy-pulse-generator ["trigger.source", "pulse.generator", "timing.source"]
saved discovery lock with 66 persistent entrie(s) to /tmp/numanager-discovery-lock.toml
first lock entry: id=Some("driver:1:Simulated Toupcam camera") label=Simulated Toupcam camera aliases=1 metadata=map keys=[device_count, model, resource_count, vendor]
mightex slc lock entry: id=Some("driver:37:Configured Mightex Sirius SLC") aliases=3 metadata=map keys=[channel_count, device_count, family, model, module_type, product_id, product_string, resource_count, support_level, vendor, vendor_id]
mightex slc support level: String("diagnostic")
added Simulated Toupcam camera with 1 device(s)
added Simulated Spark Cyto with 8 device(s)
added Simulated Cephla Squid controller with 25 device(s)
added Simulated ASI MS-2000 controller with 3 device(s)
added Simulated ASI Tiger controller with 6 device(s)
added Simulated Cobolt serial laser with 1 device(s)
added Simulated CoolLED pE-4000 with 5 device(s)
added Simulated CoolLED pE-300 with 4 device(s)
added Simulated Zaber ASCII stage with 2 device(s)
added Simulated Coherent OBIS laser with 1 device(s)
added Simulated Omicron serial laser with 1 device(s)
added Simulated Prior ProScan controller with 8 device(s)
added Simulated SutterStage controller with 4 device(s)
added Simulated Sutter MP-285 manipulator with 3 device(s)
added Simulated Marzhauser L-Step/TANGO controller with 3 device(s)
added Configured PI GCS controller fixture with 3 device(s)
added Configured Thorlabs APT motor fixture with 2 device(s)
added Configured Lumencor SpectraX fixture with 7 device(s)
added Configured Lumencor CIA fixture with 1 device(s)
added Configured Thorlabs DC2010/DC2100 LED controller with 1 device(s)
added Configured Thorlabs DC3100 LED controller with 1 device(s)
added Configured Thorlabs DC2200 SCPI LED controller with 1 device(s)
added Configured Thorlabs DC4100/DC4104 LED controller with 5 device(s)
added Configured Thorlabs DC4100 LED controller with 5 device(s)
added Configured Modbus IO fixture with 1 device(s)
added Configured Mightex Sirius BLS with 5 device(s)
added Configured Thorlabs APT stage with 2 device(s)
added Configured GenICam node map genicam-local-camera with 1 device(s)
added Configured GenICam node map Configured GenICam local node-map camera with 1 device(s)
added Configured GigE Vision camera Configured GigE Vision fixture camera with 1 device(s)
added Configured USB3 Vision camera Configured USB3 Vision fixture camera with 1 device(s)
added Configured Thorlabs KURIOS LCTF fixture with 1 device(s)
added Configured Thorlabs KURIOS filter with 1 device(s)
added Configured platform camera Configured platform fixture camera (fixture) with 1 device(s)
added Configured Mightex Sirius SLC with 3 device(s)
added Standa 8SMC4 8SMC4-USB STANDA-CONFIG-0002 with 2 device(s)
added Configured Hamilton Serial MVP valve with 2 device(s)
added Configured Trinamic TMCL stage controller with 4 device(s)
added Configured Velleman K8055 IO board with 9 device(s)
added Configured Velleman K8061 IO board with 22 device(s)
added Configured Starlight Xpress filter wheel with 1 device(s)
added Configured Spectral LMM5 with 6 device(s)
added Configured OpenStage controller with 3 device(s)
added Configured WOSM controller with 10 device(s)
added Configured TriggerScope controller with 12 device(s)
added Configured Chuo Seiki QT controller with 3 device(s)
added Configured ITK Corvus controller with 3 device(s)
added Configured Bluebox Optics niji with 8 device(s)
added Configured Opentrons OT-2 robot with 6 device(s)
added Configured Thorlabs SC10 shutter controller with 2 device(s)
added Configured CoolLED pE-340 with 5 device(s)
added Configured Andor SDK2 camera (136e:0012) with 3 device(s)
added Configured Andor SDK3 camera (136e:0014) with 3 device(s)
added Configured Photometrics PVCAM camera (PVCAM-CONFIG-0002) with 3 device(s)
added Configured Evident IX85 microscope body (IX85-CONFIG-0002) with 8 device(s)
added Configured Okolab environmental controller (H201 T Unit-BL) with 3 device(s)
added Configured ABS camera reverse engineered support (ABS CamUSB camera) with 1 device(s)
added Configured Mightex camera reverse engineered support (Mightex buffered USB camera) with 1 device(s)
added Configured MCL reverse engineered support (Mad City Labs MicroDrive/NanoDrive) with 4 device(s)
added Configured Agilent Laser Combiner with 9 device(s)
added Configured Arduino controller with 5 device(s)
added Configured Arduino Counter with 3 device(s)
added Configured ESP32 controller with 7 device(s)
added Configured OpenUC2 Feather controller with 4 device(s)
added Configured Teensy pulse generator with 2 device(s)
```

### Discovery With HID Devices

Command:

```sh
cargo run -p numanager-examples --features os-hid -- discover_devices
```

Recorded output excerpt:

```text
detected 66 candidate driver(s)
25: Configured Mightex Sirius BLS, 5 device(s), 1 resource(s)
    device: mightex-bls-hub ["hub", "light.engine", "hid.device"]
    device: mightex-bls-channel-1 ["light.source", "led.channel", "trigger.sink"]
    device: mightex-bls-channel-2 ["light.source", "led.channel", "trigger.sink"]
    device: mightex-bls-channel-3 ["light.source", "led.channel", "trigger.sink"]
    device: mightex-bls-channel-4 ["light.source", "led.channel", "trigger.sink"]
34: Configured Mightex Sirius SLC, 3 device(s), 1 resource(s)
    device: mightex-slc-hub ["hub", "light.engine", "hid.device"]
    device: mightex-slc-channel-1 ["light.source", "led.channel", "trigger.sink"]
    device: mightex-slc-channel-2 ["light.source", "led.channel", "trigger.sink"]
35: Standa 8SMC4 8SMC4-USB STANDA-CONFIG-0002, 2 device(s), 1 resource(s)
    device: standa-8smc4-hub ["hub", "motion.controller", "standa.8smc4"]
    device: standa-8smc4-x ["axis.x", "stage.1d", "standa.8smc4.axis"]
36: Configured Hamilton Serial MVP valve, 2 device(s), 1 resource(s)
    device: hamilton-mvp-hub ["hub", "fluidics.controller", "hamilton.mvp"]
    device: hamilton-mvp-valve ["fluidics.valve", "state.device", "hamilton.mvp.valve"]
37: Configured Trinamic TMCL stage controller, 4 device(s), 1 resource(s)
    device: trinamic-tmcl-hub ["hub", "motion.controller", "trinamic.tmcl"]
    device: trinamic-tmcl-x-stage ["stage.1d", "motion.stage", "state.device", "trinamic.tmcl.axis"]
    device: trinamic-tmcl-y-stage ["stage.1d", "motion.stage", "state.device", "trinamic.tmcl.axis"]
    device: trinamic-tmcl-z-stage ["stage.1d", "motion.stage", "state.device", "trinamic.tmcl.axis"]
38: Configured Velleman K8055 IO board, 9 device(s), 1 resource(s)
    device: velleman-k8055-hub ["hub", "usb.io", "velleman.k8055"]
    device: velleman-k8055-digital-input ["digital.input", "state.device"]
    device: velleman-k8055-digital-output ["digital.output", "state.device"]
    device: velleman-k8055-analog-input-1 ["analog.input", "adc"]
    device: velleman-k8055-analog-input-2 ["analog.input", "adc"]
    device: velleman-k8055-counter-1 ["counter", "digital.input.counter"]
    device: velleman-k8055-counter-2 ["counter", "digital.input.counter"]
    device: velleman-k8055-analog-output-1 ["analog.output", "dac"]
    device: velleman-k8055-analog-output-2 ["analog.output", "dac"]
39: Configured Velleman K8061 IO board, 22 device(s), 1 resource(s)
    device: velleman-k8061-hub ["hub", "usb.io", "velleman.k8061"]
    device: velleman-k8061-digital-input ["digital.input", "state.device"]
    device: velleman-k8061-digital-output ["digital.output", "state.device"]
    device: velleman-k8061-analog-input-1 ["analog.input", "adc"]
    device: velleman-k8061-analog-input-2 ["analog.input", "adc"]
    device: velleman-k8061-analog-input-3 ["analog.input", "adc"]
    device: velleman-k8061-analog-input-4 ["analog.input", "adc"]
    device: velleman-k8061-analog-input-5 ["analog.input", "adc"]
    device: velleman-k8061-analog-input-6 ["analog.input", "adc"]
    device: velleman-k8061-analog-input-7 ["analog.input", "adc"]
    device: velleman-k8061-analog-input-8 ["analog.input", "adc"]
    device: velleman-k8061-counter-1 ["counter", "digital.input.counter"]
    device: velleman-k8061-counter-2 ["counter", "digital.input.counter"]
    device: velleman-k8061-analog-output-1 ["analog.output", "dac"]
    device: velleman-k8061-analog-output-2 ["analog.output", "dac"]
    device: velleman-k8061-analog-output-3 ["analog.output", "dac"]
    device: velleman-k8061-analog-output-4 ["analog.output", "dac"]
    device: velleman-k8061-analog-output-5 ["analog.output", "dac"]
    device: velleman-k8061-analog-output-6 ["analog.output", "dac"]
    device: velleman-k8061-analog-output-7 ["analog.output", "dac"]
    device: velleman-k8061-analog-output-8 ["analog.output", "dac"]
    device: velleman-k8061-pwm-output ["pwm.output", "dac"]
40: Configured Starlight Xpress filter wheel, 1 device(s), 1 resource(s)
    device: starlight-xpress-filter-wheel ["filter.wheel", "state.device"]
41: Configured Spectral LMM5, 6 device(s), 1 resource(s)
    device: spectral-lmm5-hub ["hub", "light.engine", "serial.ascii.hex"]
    device: spectral-lmm5-line-1 ["light.source", "laser.line", "shutter", "trigger.sink"]
    device: spectral-lmm5-line-2 ["light.source", "laser.line", "shutter", "trigger.sink"]
    device: spectral-lmm5-line-3 ["light.source", "laser.line", "shutter", "trigger.sink"]
    device: spectral-lmm5-line-4 ["light.source", "laser.line", "shutter", "trigger.sink"]
    device: spectral-lmm5-line-5 ["light.source", "laser.line", "shutter", "trigger.sink"]
42: Configured OpenStage controller, 3 device(s), 1 resource(s)
    device: openstage-hub ["hub", "motion.controller", "serial.ascii"]
    device: openstage-xy ["axis.xy", "stage.xy", "motion.stage"]
    device: openstage-z ["axis.z", "stage.z", "motion.stage"]
43: Configured WOSM controller, 10 device(s), 1 resource(s)
    device: wosm-hub ["hub", "microscope.controller", "tcp.text"]
    device: wosm-switch ["digital.output", "state.device", "trigger.source"]
    device: wosm-shutter ["shutter", "light.gate", "trigger.sink"]
    device: wosm-xy-stage ["axis.xy", "stage.xy", "motion.stage"]
    device: wosm-z-stage ["axis.z", "stage.z", "motion.stage"]
    device: wosm-input ["digital.input", "analog.input", "state.device"]
    device: wosm-light-1 ["light.source", "dac.output", "trigger.sink"]
    device: wosm-light-2 ["light.source", "dac.output", "trigger.sink"]
    device: wosm-light-3 ["light.source", "dac.output", "trigger.sink"]
    device: wosm-light-4 ["light.source", "dac.output", "trigger.sink"]
44: Configured TriggerScope controller, 12 device(s), 1 resource(s)
    device: triggerscope-hub ["hub", "trigger.controller", "serial.ascii"]
    device: triggerscope-focus ["axis.z", "stage.z", "motion.stage"]
    device: triggerscope-cam-1 ["camera.trigger", "trigger.source", "state.device"]
    device: triggerscope-cam-2 ["camera.trigger", "trigger.source", "state.device"]
    device: triggerscope-ttl-1 ["digital.output", "ttl.output", "trigger.source", "trigger.sink"]
    device: triggerscope-ttl-2 ["digital.output", "ttl.output", "trigger.source", "trigger.sink"]
    device: triggerscope-ttl-3 ["digital.output", "ttl.output", "trigger.source", "trigger.sink"]
    device: triggerscope-ttl-4 ["digital.output", "ttl.output", "trigger.source", "trigger.sink"]
    device: triggerscope-dac-1 ["analog.output", "dac.output", "trigger.sink"]
    device: triggerscope-dac-2 ["analog.output", "dac.output", "trigger.sink"]
    device: triggerscope-dac-3 ["analog.output", "dac.output", "trigger.sink"]
    device: triggerscope-dac-4 ["analog.output", "dac.output", "trigger.sink"]
45: Configured Chuo Seiki QT controller, 3 device(s), 1 resource(s)
    device: chuo-qt-hub ["hub", "motion.controller", "serial.ascii"]
    device: chuo-qt-xy-stage ["axis.xy", "stage.xy", "motion.stage"]
    device: chuo-qt-z-stage ["axis.z", "stage.z", "motion.stage"]
46: Configured ITK Corvus controller, 3 device(s), 1 resource(s)
    device: corvus-hub ["hub", "motion.controller", "serial.ascii"]
    device: corvus-xy-stage ["axis.xy", "stage.xy", "motion.stage"]
    device: corvus-z-stage ["axis.z", "stage.z", "motion.stage"]
47: Configured Bluebox Optics niji, 8 device(s), 1 resource(s)
    device: niji-hub ["hub", "light.engine", "shutter", "serial.ascii"]
    device: niji-channel-1 ["light.source", "led.channel", "trigger.sink"]
    device: niji-channel-2 ["light.source", "led.channel", "trigger.sink"]
    device: niji-channel-3 ["light.source", "led.channel", "trigger.sink"]
    device: niji-channel-4 ["light.source", "led.channel", "trigger.sink"]
    device: niji-channel-5 ["light.source", "led.channel", "trigger.sink"]
    device: niji-channel-6 ["light.source", "led.channel", "trigger.sink"]
    device: niji-channel-7 ["light.source", "led.channel", "trigger.sink"]
48: Configured Opentrons OT-2 robot, 6 device(s), 1 resource(s)
    device: opentrons-ot2 ["hub", "liquid_handler.robot", "network.http"]
    device: opentrons-ot2-gantry ["stage.xyz", "motion.robot"]
    device: opentrons-ot2-deck ["deck", "labware.host"]
    device: opentrons-ot2-left-pipette ["liquid_handler.pipette", "mount.left"]
    device: opentrons-ot2-camera ["camera.snapshot", "inspection.camera"]
    device: opentrons-ot2-module-1 ["module.temperature", "module.opentrons"]
49: Configured Thorlabs SC10 shutter controller, 2 device(s), 1 resource(s)
    device: thorlabs-sc10-controller ["hub", "shutter.controller", "serial.ascii"]
    device: thorlabs-sc10-shutter ["shutter", "light.gate", "trigger.sink"]
50: Configured CoolLED pE-340, 5 device(s), 1 resource(s)
    device: coolled-pe340-hub ["hub", "light.engine", "shutter"]
    device: coolled-pe340-channel-1 ["light.source", "led.channel", "trigger.sink"]
    device: coolled-pe340-channel-2 ["light.source", "led.channel", "trigger.sink"]
    device: coolled-pe340-channel-3 ["light.source", "led.channel", "trigger.sink"]
    device: coolled-pe340-channel-4 ["light.source", "led.channel", "trigger.sink"]
52: Configured Andor SDK2 camera (136e:0012), 3 device(s), 3 resource(s)
    device: Configured Andor SDK2 camera hub ["hub", "usb.camera", "camera.controller"]
    device: Configured Andor SDK2 camera ["camera", "camera.scientific", "detector.mono", "andor.sdk2"]
    device: Configured Andor SDK2 camera cooler ["temperature.controller", "cooler", "state.device"]
53: Configured Andor SDK3 camera (136e:0014), 3 device(s), 3 resource(s)
    device: Configured Andor SDK3 camera hub ["hub", "usb.camera", "camera.controller"]
    device: Configured Andor SDK3 camera ["camera", "camera.scientific", "detector.mono", "andor.sdk3"]
    device: Configured Andor SDK3 camera cooler ["temperature.controller", "cooler", "state.device"]
54: Configured Photometrics PVCAM camera (PVCAM-CONFIG-0002), 3 device(s), 2 resource(s)
    device: Configured Photometrics PVCAM camera hub ["hub", "camera.controller", "pvcam"]
    device: Configured Photometrics PVCAM camera ["camera", "camera.scientific", "detector.mono", "pvcam"]
    device: Configured Photometrics PVCAM camera cooler ["temperature.controller", "cooler", "state.device"]
55: Configured Evident IX85 microscope body (IX85-CONFIG-0002), 8 device(s), 1 resource(s)
    device: ix85-hub ["hub", "microscope.body", "serial.ascii"]
    device: ix85-focus ["axis.z", "stage.z", "microscope.focus"]
    device: ix85-nosepiece ["objective.turret", "state.device"]
    device: ix85-light-path ["light.path", "state.device"]
    device: ix85-mirror-unit-1 ["filter.cube", "mirror.unit", "state.device"]
    device: ix85-dia-shutter ["shutter", "light.gate", "state.device"]
    device: ix85-epi-shutter-1 ["shutter", "light.gate", "state.device"]
    device: ix85-zdc-autofocus ["autofocus", "zdc", "state.device"]
56: Configured Okolab environmental controller (H201 T Unit-BL), 3 device(s), 2 resource(s)
    device: Configured Okolab environmental controller hub ["hub", "environment.controller", "serial.device"]
    device: Configured Okolab environmental controller temperature ["environment.temperature", "measure"]
    device: Configured Okolab environmental controller gas ["environment.gas", "measure"]
57: Configured ABS camera reverse engineered support (ABS CamUSB camera), 1 device(s), 1 resource(s)
    device: Configured ABS camera reverse engineered support ["camera", "reverse.engineered"]
58: Configured Mightex camera reverse engineered support (Mightex buffered USB camera), 1 device(s), 2 resource(s)
    device: Configured Mightex camera reverse engineered support ["camera", "reverse.engineered"]
59: Configured MCL reverse engineered support (Mad City Labs MicroDrive/NanoDrive), 4 device(s), 1 resource(s)
    device: Configured MCL reverse engineered support ["hub", "motion.controller", "reverse.engineered"]
    device: mcl-x ["stage.axis", "stage.x", "reverse.engineered"]
    device: mcl-y ["stage.axis", "stage.y", "reverse.engineered"]
    device: mcl-z ["stage.axis", "stage.z", "reverse.engineered"]
60: Configured Agilent Laser Combiner, 9 device(s), 1 resource(s)
    device: agilent-combiner-hub ["hub", "light.engine"]
    device: agilent-laser-line-1 ["light.source", "laser", "trigger.sink"]
    device: agilent-laser-line-2 ["light.source", "laser", "trigger.sink"]
    device: agilent-laser-line-3 ["light.source", "laser", "trigger.sink"]
    device: agilent-laser-line-4 ["light.source", "laser", "trigger.sink"]
    device: agilent-analog-output-1 ["analog.output"]
    device: agilent-analog-output-2 ["analog.output"]
    device: agilent-analog-output-3 ["analog.output"]
    device: agilent-analog-output-4 ["analog.output"]
61: Configured Arduino controller, 5 device(s), 1 resource(s)
    device: arduino-hub ["hub", "microcontroller"]
    device: arduino-digital-out ["digital.io", "trigger.source"]
    device: arduino-shutter ["shutter", "trigger.sink"]
    device: arduino-adc ["analog.input"]
    device: arduino-dac ["analog.output"]
62: Configured Arduino Counter, 3 device(s), 1 resource(s)
    device: arduino-counter-hub ["hub", "microcontroller"]
    device: arduino-counter ["counter", "timing.source"]
    device: arduino-counter-pulse ["trigger.source", "pulse.generator"]
63: Configured ESP32 controller, 7 device(s), 1 resource(s)
    device: esp32-hub ["hub", "microcontroller"]
    device: esp32-digital-out ["digital.io", "trigger.source"]
    device: esp32-shutter ["shutter", "trigger.sink"]
    device: esp32-pwm ["analog.output", "pwm"]
    device: esp32-adc ["analog.input", "adc"]
    device: esp32-xy ["axis.xy"]
    device: esp32-z ["axis.z"]
64: Configured OpenUC2 Feather controller, 4 device(s), 1 resource(s)
    device: openuc2-hub ["hub", "microcontroller"]
    device: openuc2-xy ["axis.xy"]
    device: openuc2-z ["axis.z"]
    device: openuc2-laser ["light.source", "shutter", "trigger.sink"]
65: Configured Teensy pulse generator, 2 device(s), 1 resource(s)
    device: teensy-pulse-hub ["hub", "microcontroller"]
    device: teensy-pulse-generator ["trigger.source", "pulse.generator", "timing.source"]
saved discovery lock with 66 persistent entrie(s) to /tmp/numanager-discovery-lock.toml
mightex slc lock entry: id=Some("driver:37:Configured Mightex Sirius SLC") aliases=3 metadata=map keys=[channel_count, device_count, family, model, module_type, product_id, product_string, resource_count, support_level, vendor, vendor_id]
mightex slc support level: String("diagnostic")
added Configured Mightex Sirius BLS with 5 device(s)
added Configured Mightex Sirius SLC with 3 device(s)
added Standa 8SMC4 8SMC4-USB STANDA-CONFIG-0002 with 2 device(s)
added Configured Hamilton Serial MVP valve with 2 device(s)
added Configured Trinamic TMCL stage controller with 4 device(s)
added Configured Velleman K8055 IO board with 9 device(s)
added Configured Velleman K8061 IO board with 22 device(s)
added Configured Starlight Xpress filter wheel with 1 device(s)
added Configured Spectral LMM5 with 6 device(s)
added Configured OpenStage controller with 3 device(s)
added Configured WOSM controller with 10 device(s)
added Configured TriggerScope controller with 12 device(s)
added Configured Chuo Seiki QT controller with 3 device(s)
added Configured ITK Corvus controller with 3 device(s)
added Configured Bluebox Optics niji with 8 device(s)
added Configured Opentrons OT-2 robot with 6 device(s)
added Configured Thorlabs SC10 shutter controller with 2 device(s)
added Configured CoolLED pE-340 with 5 device(s)
added Configured Andor SDK2 camera (136e:0012) with 3 device(s)
added Configured Andor SDK3 camera (136e:0014) with 3 device(s)
added Configured Photometrics PVCAM camera (PVCAM-CONFIG-0002) with 3 device(s)
added Configured Evident IX85 microscope body (IX85-CONFIG-0002) with 8 device(s)
added Configured Okolab environmental controller (H201 T Unit-BL) with 3 device(s)
added Configured ABS camera reverse engineered support (ABS CamUSB camera) with 1 device(s)
added Configured Mightex camera reverse engineered support (Mightex buffered USB camera) with 1 device(s)
added Configured MCL reverse engineered support (Mad City Labs MicroDrive/NanoDrive) with 4 device(s)
added Configured Agilent Laser Combiner with 9 device(s)
added Configured Arduino controller with 5 device(s)
added Configured Arduino Counter with 3 device(s)
added Configured ESP32 controller with 7 device(s)
added Configured OpenUC2 Feather controller with 4 device(s)
added Configured Teensy pulse generator with 2 device(s)
```

The listed Mightex candidates above are configured identity fixtures used by
the discovery workflow. When a real Sirius HID controller is attached, the
`live HID discovery found ...` line should report the additional live candidate
count before the candidate table is printed.

## Digital IO

Command:

```sh
cargo run -p numanager-examples -- digital_io
```

Recorded output:

```text
selected digital output: arduino-digital-out [digital.io, trigger.source]
selected shutter input: arduino-shutter [shutter, trigger.sink]
selected analog input: arduino-adc [analog.input]
selected analog output: arduino-dac [analog.output]
selected counter: arduino-counter [counter, timing.source]; pulse output: arduino-counter-pulse [trigger.source, pulse.generator]
selected ASI Tiger TTL: asi-tiger-ttl [digital.output, trigger.source, asi.tiger.card]; ring buffer: asi-tiger-ring-buffer [motion.program, ring.buffer, asi.tiger.card]
selected standalone pulse generator: teensy-pulse-generator [trigger.source, pulse.generator, timing.source]
capabilities: digital=DigitalIo request=DigitalIo; shutter=TriggerSink request=Trigger; adc=Adc request=Adc; dac=Dac request=Dac; measure=Measure request=Measure; pulse_program=PulseProgram request=PulseProgram; pulse_trigger=TriggerSource request=Trigger; tiger_trigger=TriggerSource request=Trigger; tiger_program=PulseProgram request=PulseProgram; standalone_program=PulseProgram request=PulseProgram; standalone_trigger=TriggerSource request=Trigger
state set completed: map keys=[channel_0, duration, gate, interval x2, level, mask, number_of_pulses, open, timed_delays, wait_for_input]
digital write completed: map keys=[mask]
analog output completed: map keys=[property, value]
pulse program completed: map keys=[counter_summary, interval]
pulse trigger completed: map keys=[level, triggered]
standalone pulse program completed: map keys=[program_summary]
standalone pulse completed: map keys=[counted_pulses, running, triggered]
ASI Tiger TTL pulse completed: map keys=[action, ttl0]
ASI Tiger ring program completed: map keys=[mode, running, size]
shutter pulse completed: map keys=[open, triggered]
analog read completed: I64(2048)
counter measure completed: map keys=[count, counter_summary, gate]
timing arm completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
timing start completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
timing stop completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
mask: I64(2)
timed_delays: list len=2
open: Bool(false)
input_summary: map keys=[adc_channel, adc_count, digital_inputs]
channel_0: I64(256)
gate: TimeInterval(TimeInterval { value: 10.0, unit: Milliseconds })
count: I64(350)
interval: TimeInterval(TimeInterval { value: 1000.0, unit: Microseconds })
level: Bool(false)
interval: TimeInterval(TimeInterval { value: 1500.0, unit: Microseconds })
duration: TimeInterval(TimeInterval { value: 200.0, unit: Microseconds })
number_of_pulses: I64(4)
running: Bool(false)
event: operation on [arduino-digital-out, arduino-shutter, arduino-dac, arduino-counter, arduino-counter-pulse, teensy-pulse-generator] running
event: arduino-digital-out.mask changed to I64(3)
event: arduino-digital-out.timed_delays changed to List([TimeInterval(TimeInterval { value: 2.0, unit: Milliseconds }), TimeInterval(TimeInterval { value: 5.0, unit: Milliseconds })])
event: arduino-shutter.open changed to Bool(false)
event: arduino-dac.channel_0 changed to I64(128)
event: arduino-counter.gate changed to TimeInterval(TimeInterval { value: 25.0, unit: Milliseconds })
event: arduino-counter.count changed to I64(250)
event: arduino-counter.interval changed to TimeInterval(TimeInterval { value: 1000.0, unit: Microseconds })
event: arduino-counter-pulse.level changed to Bool(false)
event: teensy-pulse-generator.interval changed to TimeInterval(TimeInterval { value: 2000.0, unit: Microseconds })
event: teensy-pulse-generator.duration changed to TimeInterval(TimeInterval { value: 250.0, unit: Microseconds })
event: teensy-pulse-generator.number_of_pulses changed to I64(3)
event: teensy-pulse-generator.wait_for_input changed to Bool(false)
event: operation on [arduino-digital-out, arduino-shutter, arduino-dac, arduino-counter, arduino-counter-pulse, teensy-pulse-generator] completed map keys=[channel_0, duration, gate, interval x2, level, mask, number_of_pulses, open, timed_delays, wait_for_input]
event: operation on [arduino-digital-out] running
event: arduino-digital-out.mask changed to I64(5)
event: operation on [arduino-digital-out] completed map keys=[mask]
event: operation on [arduino-dac] running
event: arduino-dac.channel_0 changed to I64(256)
event: operation on [arduino-dac] completed map keys=[property, value]
event: operation on [arduino-counter] running
event: arduino-counter.interval changed to TimeInterval(TimeInterval { value: 500.0, unit: Microseconds })
event: arduino-counter.counter_summary changed to Map({"count": I64(250), "pulse_level": Bool(false)})
event: operation on [arduino-counter] completed map keys=[counter_summary, interval]
event: operation on [arduino-counter-pulse] running
event: arduino-counter-pulse.level changed to Bool(true)
event: arduino-counter-pulse.level changed to Bool(false)
event: operation on [arduino-counter-pulse] completed map keys=[level, triggered]
event: operation on [teensy-pulse-generator] running
event: teensy-pulse-generator.interval changed to TimeInterval(TimeInterval { value: 1500.0, unit: Microseconds })
event: teensy-pulse-generator.duration changed to TimeInterval(TimeInterval { value: 200.0, unit: Microseconds })
event: teensy-pulse-generator.wait_for_input changed to Bool(false)
event: teensy-pulse-generator.number_of_pulses changed to I64(4)
event: teensy-pulse-generator.program_summary changed to Map({"counted_pulses": I64(0), "duration": TimeInterval(TimeInterval { value: 200.0, unit: Microseconds }), "interval": TimeInterval(TimeInterval { value: 1500.0, unit: Microseconds }), "number_of_pulses": I64(4), "running": Bool(false), "wait_for_input": Bool(false)})
event: operation on [teensy-pulse-generator] completed map keys=[program_summary]
event: operation on [teensy-pulse-generator] running
event: teensy-pulse-generator.running changed to Bool(true)
event: teensy-pulse-generator.running changed to Bool(false)
event: teensy-pulse-generator.counted_pulses changed to I64(4)
event: operation on [teensy-pulse-generator] completed map keys=[counted_pulses, running, triggered]
event: operation on [asi-tiger-ttl] running
event: asi-tiger-ttl.ttl0 changed to Bool(true)
event: asi-tiger-ttl.ttl0 changed to Bool(false)
event: operation on [asi-tiger-ttl] completed map keys=[action, ttl0]
event: operation on [asi-tiger-ring-buffer] running
event: asi-tiger-ring-buffer.running changed to Bool(true)
event: operation on [asi-tiger-ring-buffer] completed map keys=[mode, running, size]
event: operation on [arduino-shutter] running
event: arduino-shutter.open changed to Bool(true)
event: arduino-shutter.open changed to Bool(false)
event: operation on [arduino-shutter] completed map keys=[open, triggered]
event: operation on [arduino-counter] running
event: arduino-counter.gate changed to TimeInterval(TimeInterval { value: 10.0, unit: Milliseconds })
event: arduino-counter.count changed to I64(350)
event: arduino-counter.counter_summary changed to Map({"count": I64(350), "pulse_level": Bool(false)})
event: operation on [arduino-counter] completed map keys=[count, counter_summary, gate]
event: operation on [arduino-digital-out, arduino-shutter, arduino-counter, arduino-counter-pulse] running
event: operation on [arduino-digital-out, arduino-shutter, arduino-counter, arduino-counter-pulse] completed map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
event: arduino-digital-out.mask changed to I64(1)
event: arduino-shutter.open changed to Bool(true)
event: arduino-digital-out.sequence changed to String("On")
event: operation on [arduino-digital-out, arduino-shutter, arduino-counter, arduino-counter-pulse] running
event: arduino-counter.interval changed to TimeInterval(TimeInterval { value: 500.0, unit: Microseconds })
event: arduino-counter-pulse.level changed to Bool(true)
event: operation on [arduino-digital-out, arduino-shutter, arduino-counter, arduino-counter-pulse] completed map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
event: arduino-digital-out.mask changed to I64(2)
event: arduino-shutter.open changed to Bool(false)
event: arduino-digital-out.sequence changed to String("Off")
event: operation on [arduino-digital-out, arduino-shutter, arduino-counter, arduino-counter-pulse] running
event: arduino-counter.interval changed to TimeInterval(TimeInterval { value: 1000.0, unit: Microseconds })
event: arduino-counter-pulse.level changed to Bool(false)
event: operation on [arduino-digital-out, arduino-shutter, arduino-counter, arduino-counter-pulse] completed map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
event: operation on [arduino-digital-out] running
event: operation on [arduino-digital-out] completed I64(2)
event: operation on [arduino-digital-out] running
event: operation on [arduino-digital-out] completed list len=2
event: operation on [arduino-shutter] running
event: operation on [arduino-shutter] completed Bool(false)
event: operation on [arduino-dac] running
event: operation on [arduino-dac] completed I64(256)
event: operation on [arduino-counter] running
event: operation on [arduino-counter] completed TimeInterval(TimeInterval { value: 10.0, unit: Milliseconds })
event: operation on [arduino-counter] running
event: operation on [arduino-counter] completed I64(350)
event: operation on [arduino-counter] running
event: operation on [arduino-counter] completed TimeInterval(TimeInterval { value: 1000.0, unit: Microseconds })
event: operation on [arduino-counter-pulse] running
event: operation on [arduino-counter-pulse] completed Bool(false)
event: operation on [teensy-pulse-generator] running
event: operation on [teensy-pulse-generator] completed TimeInterval(TimeInterval { value: 1500.0, unit: Microseconds })
event: operation on [teensy-pulse-generator] running
event: operation on [teensy-pulse-generator] completed TimeInterval(TimeInterval { value: 200.0, unit: Microseconds })
event: operation on [teensy-pulse-generator] running
event: operation on [teensy-pulse-generator] completed I64(4)
event: operation on [teensy-pulse-generator] running
event: operation on [teensy-pulse-generator] completed Bool(false)
```

Command for the Arduino configured source:

```sh
cargo run -p numanager-examples -- digital_io arduino
```

Recorded output excerpt:

```text
selected digital IO source: arduino
selected digital device: arduino-digital-out [digital.io, trigger.source]
selected trigger source: arduino-digital-out [digital.io, trigger.source]
selected trigger sink: arduino-shutter [shutter, trigger.sink]
selected analog output: arduino-dac [analog.output]
selected analog input: arduino-adc [analog.input]
state set completed: map keys=[channel_0, mask, open]
digital write completed: map keys=[mask]
analog output completed: map keys=[property, value]
trigger source completed: map keys=[sequence, triggered]
trigger sink completed: map keys=[open, triggered]
analog read completed: map keys=[adc_channel, adc_count, digital_inputs, input_pullups]
arduino-digital-out mask: I64(5)
arduino-dac channel_0: I64(430)
arduino-adc input_summary: map keys=[adc_channel, adc_count, digital_inputs, input_pullups]
```

Command for the Arduino Counter configured source:

```sh
cargo run -p numanager-examples -- digital_io arduino_counter
```

Recorded output excerpt:

```text
selected digital IO source: arduino_counter
selected trigger source: arduino-counter-pulse [trigger.source, pulse.generator]
selected measurement device: arduino-counter [counter, timing.source]
state set completed: none
trigger source completed: map keys=[level, triggered]
measure completed: map keys=[count, counter_summary, gate]
```

Command for the ESP32 configured source:

```sh
cargo run -p numanager-examples -- digital_io esp32
```

Recorded output excerpt:

```text
selected digital IO source: esp32
selected digital device: esp32-digital-out [digital.io, trigger.source]
selected trigger sink: esp32-shutter [shutter, trigger.sink]
selected analog output: esp32-pwm [analog.output, pwm]
selected analog input: none
state set completed: map keys=[channel_0, mask, open]
digital write completed: map keys=[mask]
analog output completed: map keys=[channel_0]
trigger source completed: map keys=[mask, triggered]
trigger sink completed: map keys=[open, triggered]
esp32-pwm channel_0: Ratio(Ratio { value: 42.0, unit: Percent })
```

Command for the Teensy Pulse configured source:

```sh
cargo run -p numanager-examples -- digital_io teensy_pulse
```

Recorded output excerpt:

```text
selected digital IO source: teensy_pulse
selected trigger source: teensy-pulse-generator [trigger.source, pulse.generator, timing.source]
state set completed: none
trigger source completed: map keys=[counted_pulses, running, triggered]
```

Command for TriggerScope:

```sh
cargo run -p numanager-examples -- digital_io triggerscope
```

Recorded output excerpt:

```text
selected digital IO source: triggerscope
selected digital device: triggerscope-ttl-1 [digital.output, ttl.output, trigger.source, trigger.sink]
selected trigger source: triggerscope-cam-1 [camera.trigger, trigger.source, state.device]
selected analog output: triggerscope-dac-1 [analog.output, dac.output, trigger.sink]
state set completed: map keys=[enabled, high, voltage]
digital write completed: Bool(true)
analog output completed: Voltage(Voltage { value: 1.386, unit: Volts })
trigger source completed: Bool(true)
trigger sink completed: Bool(true)
triggerscope-dac-1 voltage: Voltage(Voltage { value: 1.386, unit: Volts })
triggerscope-hub last_transaction: map keys=[action, completion_basis, encoded_length]
```

Command for WOSM:

```sh
cargo run -p numanager-examples -- digital_io wosm
```

Recorded output excerpt:

```text
selected digital IO source: wosm
selected digital device: wosm-switch [digital.output, state.device, trigger.source]
selected analog input: wosm-input [digital.input, analog.input, state.device]
state set completed: map keys=[enabled, open, output, state]
digital write completed: I64(5)
analog output completed: Ratio(Ratio { value: 42.0, unit: Percent })
analog read completed: Ratio(Ratio { value: 10.0, unit: Percent })
measure completed: Ratio(Ratio { value: 10.0, unit: Percent })
wosm-switch state: I64(5)
wosm-input digital_input: I64(0)
```

Command for Modbus mapped IO:

```sh
cargo run -p numanager-examples -- digital_io modbus
```

Recorded output excerpt:

```text
selected digital IO source: modbus
selected digital device: none
state set completed: none
mapped IO state set completed: map keys=[enabled, target_register]
modbus-mapped-io enabled: Bool(true)
modbus-mapped-io target_register: I64(42)
modbus-mapped-io measured_register: I64(23)
```

Command for Velleman:

```sh
cargo run -p numanager-examples -- digital_io velleman
```

Recorded output excerpt:

```text
selected digital IO source: velleman
selected digital device: velleman-k8055-digital-output [digital.output, state.device]
selected analog output: velleman-k8055-analog-output-1 [analog.output, dac]
selected analog input: velleman-k8055-analog-input-1 [analog.input, adc]
state set completed: map keys=[mask, value]
digital write completed: map keys=[completion_basis, mask]
analog output completed: map keys=[completion_basis, value]
analog read completed: Ratio(Ratio { value: 49.80392156862745, unit: Percent })
measure completed: map keys=[input_count, mask]
velleman-k8055-digital-output mask: I64(5)
velleman-k8055-hub last_transaction: map keys=[analog_input_1, analog_input_2, command, completion_basis, digital_input_mask]
```

## Autofocus

Command:

```sh
cargo run -p numanager-examples -- autofocus
```

Recorded output:

```text
generic autofocus providers:
  device=squid-autofocus capability=Autofocus request=Autofocus
    depends on squid-z-stage as ZStage
    depends on squid-illumination-d1 as LightSource
  device=asi-tiger-crisp-autofocus capability=Autofocus request=Autofocus
    depends on asi-tiger-z as ZStage
  device=sutter-autofocus capability=Autofocus request=Autofocus
    depends on sutter-z-stage as ZStage
  device=sim-composed-autofocus capability=Autofocus request=Autofocus
    depends on sim-af-camera as Camera
    depends on sim-af-z as ZStage
    depends on sim-af-light as LightSource
squid-autofocus autofocus hold completed: driver-owned completion
asi-tiger-crisp-autofocus autofocus hold completed: map keys=[focus_score, locked, mode]
sutter-autofocus autofocus hold completed: map keys=[enabled, focus_score, mode, parameter, status]
sim-composed-autofocus autofocus hold completed: map keys=[focus_score, light_enabled, mode, provider_model, status, z]
composed autofocus timing arm: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
composed autofocus timing start: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
composed autofocus timing stop: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
```

## Biological Simulation

Command:

```sh
cargo run -p numanager-examples -- biology_simulation
```

Recorded output:

```text
biological simulation devices:
  sim-af-camera [camera, simulator]
  sim-af-z [axis.z, simulator]
  sim-af-light [light.source, shutter, simulator]
  sim-composed-autofocus [autofocus, service, simulator]
capabilities: capture=CameraCapture request=CameraCapture move=StageMove request=StageMove autofocus=Autofocus request=Autofocus
scene setup completed: map keys=[enabled, exposure, power]
off-focus Z move completed: Position(Position { value: 3700.0, unit: Micrometers })
before autofocus: 640x480 307200 bytes Mono8 metadata=[autofocus_mode, exposure, focal_plane, focus_score, light_enabled, light_power, scene, z]
  focus_score=F64(0.0016854039927594855)
  z=Position(Position { value: 3700.0, unit: Micrometers })
autofocus completed: map keys=[focus_score, light_enabled, mode, provider_model, status, z]
after autofocus: 640x480 307200 bytes Mono8 metadata=[autofocus_mode, exposure, focal_plane, focus_score, light_enabled, light_power, scene, z]
  focus_score=F64(1.0)
  z=Position(Position { value: 4250.0, unit: Micrometers })
timing arm completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
timing start completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
timing stop completed: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
```

## Squid Controller Graph

Command:

```sh
cargo run -p numanager-examples -- squid
```

Recorded output:

```text
squid devices:
  squid-controller ["hub", "serial.controller"] typed caps=[none]
  squid-xy-stage ["stage.xy"] typed caps=[StageMove request=StageMove, StageHome request=None]
  squid-z-stage ["stage.z"] typed caps=[StageMove request=StageMove, StageHome request=None]
  squid-theta ["stage.theta"] typed caps=[none]
  squid-filter-wheel-w ["filter.wheel"] typed caps=[none]
  squid-filter-wheel-w2 ["filter.wheel"] typed caps=[none]
  squid-autofocus ["autofocus"] typed caps=[Autofocus request=Autofocus]
  squid-illumination-d1 ["light.source", "illumination.port"] typed caps=[Dac request=Dac]
  squid-illumination-d2 ["light.source", "illumination.port"] typed caps=[Dac request=Dac]
  squid-illumination-d3 ["light.source", "illumination.port"] typed caps=[Dac request=Dac]
  squid-illumination-d4 ["light.source", "illumination.port"] typed caps=[Dac request=Dac]
  squid-illumination-d5 ["light.source", "illumination.port"] typed caps=[Dac request=Dac]
  squid-trigger-1 ["trigger.source", "camera.trigger"] typed caps=[TriggerSource request=Trigger]
  squid-trigger-2 ["trigger.source", "camera.trigger"] typed caps=[TriggerSource request=Trigger]
  squid-trigger-3 ["trigger.source", "camera.trigger"] typed caps=[TriggerSource request=Trigger]
  squid-trigger-4 ["trigger.source", "camera.trigger"] typed caps=[TriggerSource request=Trigger]
initialization order has 16 graph nodes
device dependencies:
  squid-z-stage -> squid-autofocus as ZStage
  squid-illumination-d1 -> squid-autofocus as LightSource
typed capabilities: xy move=StageMove request=StageMove home=StageHome request=None; z move=StageMove request=StageMove; d1 dac=Dac request=Dac; trigger=TriggerSource request=Trigger; autofocus=Autofocus request=Autofocus
x move completed from firmware status: completed driver-owned completion
typed XY move completed: driver-owned completion
typed Z relative move completed: driver-owned completion
illumination state remuxed: driver-owned completion
typed illumination DAC completed: driver-owned completion
trigger pulse completed: driver-owned completion
autofocus hold completed: driver-owned completion
typed XY home completed: driver-owned completion
timing arm completion: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
timing start completion: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
timing stop completion: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
x now Position(Position { value: 1750.0, unit: Micrometers })
event: operation on [squid-controller] running
event: squid-controller.watchdog_timeout changed to TimeInterval(TimeInterval { value: 10.0, unit: Seconds })
event: telemetry from squid-controller: command_id, firmware_version, theta_steps, x, y, z
event: telemetry from squid-controller: command_id, firmware_version, theta_steps, x, y, z
configured discovery found 1 Squid candidate(s) from /tmp/numanager-squid-example.toml
```

## Spark Cyto Plate-Reader Graph

Command:

```sh
cargo run -p numanager-examples -- spark_cyto
```

Recorded output:

```text
spark devices:
  spark-mainboard ["hub", "plate.transport"] typed caps=[PlateMove request=PlateMove]
  spark-absorbance ["detector.absorbance"] typed caps=[Measure request=Measure]
  spark-fluorescence ["detector.fluorescence", "light.source"] typed caps=[Measure request=Measure]
  spark-luminescence ["detector.luminescence"] typed caps=[Measure request=Measure]
  spark-temperature ["environment.temperature"] typed caps=[TemperatureControl request=TemperatureControl]
  spark-gas ["environment.gas"] typed caps=[GasControl request=GasControl]
  spark-fim ["imaging.head", "objective.turret"] typed caps=[ImagingHead request=ImagingHead]
  spark-camera-binding ["camera.binding"] typed caps=[CameraBinding request=CameraBinding]
initialization order has 8 graph nodes
capabilities: plate=PlateMove request=PlateMove; absorbance=Measure request=Measure; temperature=TemperatureControl request=TemperatureControl; gas=GasControl request=GasControl; fim=ImagingHead request=ImagingHead; camera=CameraBinding request=CameraBinding
added spark driver with 8 device(s)
state set completed: map keys=[co2_target, enabled, mode, objective, target, wavelength, well]
plate move completed: map keys=[moved, well]
absorbance measure completed: map keys=[device, integration_time, signal, wavelength]
temperature control completed: map keys=[enabled, target]
gas control completed: map keys=[co2_actual, co2_target, enabled]
imaging head completed: map keys=[mode, objective]
camera binding completed: map keys=[bound, imaging_mode]
timing arm: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
timing start: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
timing stop: map keys=[arm_order, participants, prepared_drivers, routes, sequences, start, state, stop, transition_drivers]
gas.co2_target: GasConcentration(GasConcentration { value: 0.04, unit: Percent })
gas.co2_actual: GasConcentration(GasConcentration { value: 0.04, unit: Percent })
gas.enabled: Bool(false)
gas.fault: Bool(false)
fim.objective: I64(1)
fim.mode: String("brightfield")
fim.interlock_closed: Bool(true)
fim.fault: Bool(false)
runtime emitted a driver log event
removed spark driver with 8 device(s)
```
