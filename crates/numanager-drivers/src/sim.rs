use numanager_core::*;
use std::collections::{BTreeMap, VecDeque};

#[derive(Debug, Clone)]
pub struct SimComposedAutofocusConfig {
    pub z_travel_um: f64,
    pub focal_plane_um: f64,
    pub search_span_um: f64,
    pub light_power_percent: f64,
    pub exposure: TimeInterval,
}

impl Default for SimComposedAutofocusConfig {
    fn default() -> Self {
        Self {
            z_travel_um: 10_000.0,
            focal_plane_um: 4_250.0,
            search_span_um: 800.0,
            light_power_percent: 25.0,
            exposure: TimeInterval::from_milliseconds(20.0),
        }
    }
}

pub struct SimComposedAutofocusDriver {
    id: DriverId,
    resource: ResourceId,
    camera: DeviceId,
    z: DeviceId,
    light: DeviceId,
    autofocus: DeviceId,
    config: SimComposedAutofocusConfig,
    z_um: f64,
    exposure_s: f64,
    light_enabled: bool,
    light_power_percent: f64,
    autofocus_enabled: bool,
    autofocus_mode: AutofocusMode,
    autofocus_status: String,
    focus_score: f64,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
}

impl SimComposedAutofocusDriver {
    pub fn new(id: DriverId, config: SimComposedAutofocusConfig) -> Self {
        Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 801)),
            camera: DeviceId(NodeId(id.0 * 1000 + 811)),
            z: DeviceId(NodeId(id.0 * 1000 + 812)),
            light: DeviceId(NodeId(id.0 * 1000 + 813)),
            autofocus: DeviceId(NodeId(id.0 * 1000 + 814)),
            z_um: 0.0,
            exposure_s: config.exposure.seconds(),
            light_enabled: false,
            light_power_percent: config.light_power_percent,
            autofocus_enabled: false,
            autofocus_mode: AutofocusMode::Stop,
            autofocus_status: "idle".into(),
            focus_score: 0.0,
            next_token: 1,
            pending: VecDeque::new(),
            config,
        }
    }

    pub fn simulated(id: DriverId) -> Self {
        Self::new(id, SimComposedAutofocusConfig::default())
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn descriptors_for(&self) -> Vec<DeviceDescriptor> {
        vec![
            DeviceDescriptor {
                id: self.camera,
                driver: self.id,
                label: "sim-af-camera".into(),
                vendor: Some("numanager".into()),
                model: Some("composed autofocus scene camera".into()),
                serial: None,
                kinds: vec!["camera".into(), "simulator".into()],
                properties: vec![time_property("exposure", "Exposure", true, 0.0001, 10.0)],
                metadata: BTreeMap::from([
                    (
                        "scene".into(),
                        Value::String("biological focus plane".into()),
                    ),
                    ("role".into(), Value::String("autofocus.camera".into())),
                ]),
            },
            DeviceDescriptor {
                id: self.z,
                driver: self.id,
                label: "sim-af-z".into(),
                vendor: Some("numanager".into()),
                model: Some("composed autofocus Z stage".into()),
                serial: None,
                kinds: vec!["axis.z".into(), "simulator".into()],
                properties: vec![position_property(
                    "z",
                    "Z position",
                    true,
                    self.config.z_travel_um,
                )],
                metadata: BTreeMap::from([
                    (
                        "z_travel_um".into(),
                        Value::Position(Position::from_micrometers(self.config.z_travel_um)),
                    ),
                    ("role".into(), Value::String("autofocus.z_stage".into())),
                ]),
            },
            DeviceDescriptor {
                id: self.light,
                driver: self.id,
                label: "sim-af-light".into(),
                vendor: Some("numanager".into()),
                model: Some("composed autofocus illumination".into()),
                serial: None,
                kinds: vec!["light.source".into(), "shutter".into(), "simulator".into()],
                properties: light_properties(),
                metadata: BTreeMap::from([
                    (
                        "role".into(),
                        Value::String("autofocus.light_source".into()),
                    ),
                    (
                        "safety_policy".into(),
                        Value::String(
                            "enabled only during focus operation in this simulation".into(),
                        ),
                    ),
                ]),
            },
            DeviceDescriptor {
                id: self.autofocus,
                driver: self.id,
                label: "sim-composed-autofocus".into(),
                vendor: Some("numanager".into()),
                model: Some("camera/Z/light composed autofocus service".into()),
                serial: None,
                kinds: vec!["autofocus".into(), "service".into(), "simulator".into()],
                properties: autofocus_properties(),
                metadata: BTreeMap::from([
                    (
                        "provider_model".into(),
                        Value::String("composed camera/z/light service".into()),
                    ),
                    (
                        "focal_plane".into(),
                        Value::Position(Position::from_micrometers(self.config.focal_plane_um)),
                    ),
                    (
                        "default_search_span".into(),
                        Value::Position(Position::from_micrometers(self.config.search_span_um)),
                    ),
                ]),
            },
        ]
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        match (device, key) {
            (device, "exposure") if device == self.camera => Ok(Value::TimeInterval(
                TimeInterval::from_seconds(self.exposure_s),
            )),
            (device, "z") if device == self.z => {
                Ok(Value::Position(Position::from_micrometers(self.z_um)))
            }
            (device, "enabled") if device == self.light => Ok(Value::Bool(self.light_enabled)),
            (device, "power") if device == self.light => {
                Ok(Value::Ratio(Ratio::from_percent(self.light_power_percent)))
            }
            (device, "enabled") if device == self.autofocus => {
                Ok(Value::Bool(self.autofocus_enabled))
            }
            (device, "mode") if device == self.autofocus => Ok(Value::String(
                autofocus_mode_name(self.autofocus_mode).into(),
            )),
            (device, "status") if device == self.autofocus => {
                Ok(Value::String(self.autofocus_status.clone()))
            }
            (device, "focus_score") if device == self.autofocus => Ok(Value::F64(self.focus_score)),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown composed autofocus property {key}"),
            )),
        }
    }

    fn validate_write(&self, device: DeviceId, key: &str, value: &Value) -> Result<()> {
        let descriptor = self
            .descriptors_for()
            .into_iter()
            .find(|descriptor| descriptor.id == device)
            .ok_or_else(|| Error::new(ErrorCode::InvalidCommand, "unknown device"))?;
        let schema = descriptor
            .properties
            .iter()
            .find(|property| property.key == key)
            .ok_or_else(|| Error::new(ErrorCode::InvalidProperty, "unknown property"))?;
        if !schema.writable {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "property is read-only",
            ));
        }
        schema.validate(value)
    }

    fn write_property(&mut self, device: DeviceId, key: &str, value: &Value) -> Result<Value> {
        self.validate_write(device, key, value)?;
        match (device, key, value) {
            (device, "exposure", Value::TimeInterval(value)) if device == self.camera => {
                self.exposure_s = value.seconds();
                self.update_focus_score();
                Ok(Value::TimeInterval(*value))
            }
            (device, "z", Value::Position(value)) if device == self.z => {
                self.z_um = value.micrometers().clamp(0.0, self.config.z_travel_um);
                self.update_focus_score();
                Ok(Value::Position(Position::from_micrometers(self.z_um)))
            }
            (device, "enabled", Value::Bool(value)) if device == self.light => {
                self.light_enabled = *value;
                self.update_focus_score();
                Ok(Value::Bool(*value))
            }
            (device, "power", Value::Ratio(value)) if device == self.light => {
                self.light_power_percent = value.percent().clamp(0.0, 100.0);
                self.update_focus_score();
                Ok(Value::Ratio(Ratio::from_percent(self.light_power_percent)))
            }
            (device, "enabled", Value::Bool(value)) if device == self.autofocus => {
                self.autofocus_enabled = *value;
                self.light_enabled = *value;
                self.autofocus_mode = if *value {
                    AutofocusMode::Hold
                } else {
                    AutofocusMode::Stop
                };
                self.autofocus_status = if *value { "locked" } else { "idle" }.into();
                self.update_focus_score();
                Ok(Value::Bool(*value))
            }
            (device, "mode", Value::String(value)) if device == self.autofocus => {
                let mode = parse_autofocus_mode(value)?;
                self.autofocus_mode = mode;
                self.autofocus_enabled = mode != AutofocusMode::Stop;
                self.light_enabled = self.autofocus_enabled;
                self.autofocus_status = if self.autofocus_enabled {
                    "locked"
                } else {
                    "idle"
                }
                .into();
                self.update_focus_score();
                Ok(Value::String(
                    autofocus_mode_name(self.autofocus_mode).into(),
                ))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("invalid composed autofocus write {key}"),
            )),
        }
    }

    fn validate_autofocus(&self, device: DeviceId, request: &CapabilityRequest) -> Result<()> {
        if device != self.autofocus {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "unknown autofocus device",
            ));
        }
        match request {
            CapabilityRequest::Autofocus(request) => {
                if let Some(range) = request.range {
                    let center = range
                        .center
                        .unwrap_or(Position::from_micrometers(self.z_um));
                    let min_um = center.micrometers() - range.span.micrometers() / 2.0;
                    let max_um = center.micrometers() + range.span.micrometers() / 2.0;
                    if max_um < 0.0 || min_um > self.config.z_travel_um {
                        return Err(Error::new(
                            ErrorCode::InvalidProperty,
                            "autofocus range does not overlap Z travel",
                        ));
                    }
                }
                Ok(())
            }
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "composed autofocus expects an AutofocusRequest",
            )),
        }
    }

    fn apply_autofocus(&mut self, request: AutofocusRequest) -> Value {
        self.autofocus_mode = request.mode;
        self.autofocus_enabled = request.mode != AutofocusMode::Stop;
        self.light_enabled = self.autofocus_enabled;
        if request.mode == AutofocusMode::Stop {
            self.autofocus_status = "idle".into();
            self.update_focus_score();
        } else {
            let range = request.range.unwrap_or(AutofocusRange {
                center: Some(Position::from_micrometers(self.config.focal_plane_um)),
                span: Position::from_micrometers(self.config.search_span_um),
                step: Some(Position::from_micrometers(25.0)),
            });
            let center_um = range
                .center
                .unwrap_or(Position::from_micrometers(self.z_um))
                .micrometers();
            let half_span_um = range.span.micrometers() / 2.0;
            let min_um = (center_um - half_span_um).clamp(0.0, self.config.z_travel_um);
            let max_um = (center_um + half_span_um).clamp(0.0, self.config.z_travel_um);
            self.z_um = self.config.focal_plane_um.clamp(min_um, max_um);
            self.autofocus_status = "locked".into();
            self.update_focus_score();
        }
        self.emit_property(
            self.z,
            "z",
            Value::Position(Position::from_micrometers(self.z_um)),
        );
        self.emit_property(self.light, "enabled", Value::Bool(self.light_enabled));
        self.emit_property(
            self.autofocus,
            "enabled",
            Value::Bool(self.autofocus_enabled),
        );
        self.emit_property(
            self.autofocus,
            "mode",
            Value::String(autofocus_mode_name(self.autofocus_mode).into()),
        );
        self.emit_property(
            self.autofocus,
            "status",
            Value::String(self.autofocus_status.clone()),
        );
        self.emit_property(self.autofocus, "focus_score", Value::F64(self.focus_score));

        Value::Map(BTreeMap::from([
            (
                "mode".into(),
                Value::String(autofocus_mode_name(self.autofocus_mode).into()),
            ),
            (
                "status".into(),
                Value::String(self.autofocus_status.clone()),
            ),
            ("focus_score".into(), Value::F64(self.focus_score)),
            (
                "z".into(),
                Value::Position(Position::from_micrometers(self.z_um)),
            ),
            ("light_enabled".into(), Value::Bool(self.light_enabled)),
            (
                "provider_model".into(),
                Value::String("composed camera/z/light service".into()),
            ),
        ]))
    }

    fn update_focus_score(&mut self) {
        let z_error_um = self.z_um - self.config.focal_plane_um;
        let focus = gaussian(z_error_um, 0.0, 150.0);
        let illumination = if self.light_enabled {
            (self.light_power_percent / 25.0).clamp(0.0, 2.0)
        } else {
            0.2
        };
        let exposure = (self.exposure_s / 0.02).clamp(0.1, 4.0);
        self.focus_score = (focus * illumination * exposure).clamp(0.0, 1.0);
    }

    fn emit_property(&mut self, device: DeviceId, key: &str, value: Value) {
        self.pending
            .push_back(DriverEvent::Event(Event::PropertyChanged(
                PropertyChanged {
                    device,
                    key: key.into(),
                    value,
                },
            )));
    }

    fn local_timing_sequences<'a>(&self, plan: &'a TimingPlan) -> Vec<&'a DeviceSequence> {
        plan.sequences
            .iter()
            .filter(|sequence| {
                sequence.device == self.camera
                    || sequence.device == self.z
                    || sequence.device == self.light
                    || sequence.device == self.autofocus
            })
            .collect()
    }

    fn validate_timing_plan(&self, plan: &TimingPlan) -> Result<()> {
        for sequence in self.local_timing_sequences(plan) {
            if sequence.values.is_empty() {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "composed autofocus timing sequence must contain at least one value",
                ));
            }
            match (sequence.device, sequence.property.as_str()) {
                (device, "exposure") if device == self.camera => {}
                (device, "z") if device == self.z => {}
                (device, "enabled" | "power") if device == self.light => {}
                (device, "enabled" | "mode") if device == self.autofocus => {}
                _ => {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        format!(
                            "composed autofocus timing does not support {} on {:?}",
                            sequence.property, sequence.device
                        ),
                    ))
                }
            }
            for value in &sequence.values {
                self.validate_write(sequence.device, &sequence.property, value)?;
                if sequence.device == self.autofocus && sequence.property == "mode" {
                    if let Value::String(mode) = value {
                        let _ = parse_autofocus_mode(mode)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn timing_summary(&self, plan: &TimingPlan, phase: &str) -> Value {
        Value::Map(BTreeMap::from([
            ("phase".into(), Value::String(phase.into())),
            (
                "camera_participant".into(),
                Value::Bool(plan.participants.contains(&self.camera)),
            ),
            (
                "z_participant".into(),
                Value::Bool(plan.participants.contains(&self.z)),
            ),
            (
                "light_participant".into(),
                Value::Bool(plan.participants.contains(&self.light)),
            ),
            (
                "autofocus_participant".into(),
                Value::Bool(plan.participants.contains(&self.autofocus)),
            ),
            (
                "exposure".into(),
                Value::TimeInterval(TimeInterval::from_seconds(self.exposure_s)),
            ),
            (
                "z".into(),
                Value::Position(Position::from_micrometers(self.z_um)),
            ),
            ("light_enabled".into(), Value::Bool(self.light_enabled)),
            (
                "light_power".into(),
                Value::Ratio(Ratio::from_percent(self.light_power_percent)),
            ),
            (
                "autofocus_enabled".into(),
                Value::Bool(self.autofocus_enabled),
            ),
            (
                "autofocus_mode".into(),
                Value::String(autofocus_mode_name(self.autofocus_mode).into()),
            ),
            ("focus_score".into(), Value::F64(self.focus_score)),
            (
                "sequences".into(),
                Value::I64(self.local_timing_sequences(plan).len() as i64),
            ),
        ]))
    }

    fn apply_timing_sequence_step(&mut self, plan: &TimingPlan, first: bool) -> Result<Value> {
        let sequences = self
            .local_timing_sequences(plan)
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        let mut applied = BTreeMap::new();
        for sequence in sequences {
            let value = if first {
                sequence.values.first()
            } else {
                sequence.values.last()
            }
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::InvalidCommand,
                    "composed autofocus timing sequence must contain at least one value",
                )
            })?
            .clone();
            let applied_value = self.write_property(sequence.device, &sequence.property, &value)?;
            self.emit_property(sequence.device, &sequence.property, applied_value.clone());
            applied.insert(
                format!("{}:{}", sequence.device.0 .0, sequence.property),
                applied_value,
            );
        }
        self.emit_property(self.autofocus, "focus_score", Value::F64(self.focus_score));
        applied.insert("focus_score".into(), Value::F64(self.focus_score));
        Ok(Value::Map(applied))
    }

    fn capture_biological_frame(&self, token: DriverToken, request: CameraCaptureRequest) -> Frame {
        let focus_gain = self.focus_score.clamp(0.05, 1.0);
        let light_gain = if self.light_enabled {
            (self.light_power_percent / 25.0).clamp(0.1, 2.0)
        } else {
            0.2
        };
        let scene = gel_scene(640, 480, self.exposure_s * focus_gain * light_gain);
        let pixel_format = image_encoding_name(request.encoding.unwrap_or(ImageEncoding::Mono8));
        Frame {
            handle: FrameHandle {
                stream: StreamId(self.camera.0 .0),
                frame: FrameId(token.0),
            },
            device: self.camera,
            width: scene.width,
            height: scene.height,
            pixel_format: pixel_format.into(),
            data: scene.pixels,
            metadata: BTreeMap::from([
                (
                    "scene".into(),
                    Value::String("biological focus plane".into()),
                ),
                (
                    "exposure".into(),
                    Value::TimeInterval(TimeInterval::from_seconds(self.exposure_s)),
                ),
                (
                    "z".into(),
                    Value::Position(Position::from_micrometers(self.z_um)),
                ),
                (
                    "focal_plane".into(),
                    Value::Position(Position::from_micrometers(self.config.focal_plane_um)),
                ),
                ("focus_score".into(), Value::F64(self.focus_score)),
                ("light_enabled".into(), Value::Bool(self.light_enabled)),
                (
                    "light_power".into(),
                    Value::Ratio(Ratio::from_percent(self.light_power_percent)),
                ),
                (
                    "autofocus_mode".into(),
                    Value::String(autofocus_mode_name(self.autofocus_mode).into()),
                ),
            ]),
            buffer: request.buffer.unwrap_or_default(),
        }
    }
}

impl Driver for SimComposedAutofocusDriver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        self.descriptors_for()
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        vec![ResourceDescriptor {
            id: self.resource,
            driver: self.id,
            label: "sim-composed-autofocus-scene".into(),
            kind: "simulated.biological_scene".into(),
            metadata: BTreeMap::from([
                (
                    "completion".into(),
                    Value::String("service-telemetry".into()),
                ),
                (
                    "model".into(),
                    Value::String("single biological focus plane".into()),
                ),
            ]),
        }]
    }

    fn graph(&self) -> DeviceGraph {
        let mut graph = DeviceGraph::default();
        let _ = graph.insert_node(GraphNode {
            id: self.resource.0,
            kind: NodeKind::Simulator,
            label: "sim-composed-autofocus-scene".into(),
        });
        for descriptor in self.descriptors_for() {
            let kind = if descriptor.id == self.autofocus {
                NodeKind::Service
            } else {
                NodeKind::Device
            };
            let _ = graph.insert_node(GraphNode {
                id: descriptor.id.0,
                kind,
                label: descriptor.label,
            });
            let _ = graph.insert_edge(GraphEdge {
                from: self.resource.0,
                to: descriptor.id.0,
                kind: EdgeKind::OffersDevice,
            });
        }
        let _ = graph.insert_device_dependency(self.camera.0, self.autofocus.0, Role::Camera);
        let _ = graph.insert_device_dependency(self.z.0, self.autofocus.0, Role::ZStage);
        let _ = graph.insert_device_dependency(self.light.0, self.autofocus.0, Role::LightSource);
        graph
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        if device == self.camera {
            vec![capability(810, device, CapabilityKind::CameraCapture)]
        } else if device == self.z {
            vec![capability(812, device, CapabilityKind::StageMove)]
        } else if device == self.autofocus {
            vec![capability(814, device, CapabilityKind::Autofocus)]
        } else {
            Vec::new()
        }
    }

    fn prepare(&mut self, batch: &CommandBatch) -> Result<PreparedBatch> {
        let mut physical_transactions = Vec::new();
        for command in &batch.commands {
            match command {
                Command::ReadProperty { device, key } => {
                    let _ = self.read_property(*device, key)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("sim composed autofocus read {key}"),
                        payload: Value::String(key.clone()),
                    });
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("sim composed autofocus write {key}"),
                        payload: value.clone(),
                    });
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: "sim composed autofocus state set".into(),
                        payload: Value::I64(set.writes.len() as i64),
                    });
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } => {
                    let Some(capability) = self
                        .capabilities(*device)
                        .into_iter()
                        .find(|candidate| candidate.id == *capability)
                    else {
                        return Err(Error::new(ErrorCode::Unsupported, "unknown capability"));
                    };
                    match (&capability.kind, request) {
                        (CapabilityKind::Autofocus, request) => {
                            self.validate_autofocus(*device, request)?;
                            physical_transactions.push(PhysicalTransaction {
                                resource: Some(self.resource),
                                description: "sim composed autofocus camera/z/light search".into(),
                                payload: Value::Map(BTreeMap::from([
                                    ("camera".into(), Value::I64(self.camera.0 .0 as i64)),
                                    ("z_stage".into(), Value::I64(self.z.0 .0 as i64)),
                                    ("light".into(), Value::I64(self.light.0 .0 as i64)),
                                ])),
                            });
                        }
                        (CapabilityKind::StageMove, CapabilityRequest::StageMove(request)) => {
                            if *device != self.z || !request.target.contains_key(&StageAxis::Z) {
                                return Err(Error::new(
                                    ErrorCode::InvalidCommand,
                                    "composed autofocus simulation only exposes a Z stage move",
                                ));
                            }
                            physical_transactions.push(PhysicalTransaction {
                                resource: Some(self.resource),
                                description: "sim composed autofocus Z move".into(),
                                payload: Value::Map(BTreeMap::from([(
                                    "z".into(),
                                    Value::Position(request.target[&StageAxis::Z]),
                                )])),
                            });
                        }
                        (
                            CapabilityKind::CameraCapture,
                            CapabilityRequest::CameraCapture(_) | CapabilityRequest::None,
                        ) => {
                            physical_transactions.push(PhysicalTransaction {
                                resource: Some(self.resource),
                                description: "sim composed autofocus camera capture".into(),
                                payload: Value::String("biological focus-scene frame".into()),
                            });
                        }
                        _ => {
                            return Err(Error::new(
                                ErrorCode::Unsupported,
                                "unsupported composed autofocus capability request",
                            ))
                        }
                    }
                }
                Command::Arm(_) | Command::Start(_) | Command::Stop(_) => {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "composed autofocus direct timing transitions are runtime-owned",
                    ));
                }
            }
        }
        Ok(PreparedBatch {
            id: batch.id,
            commands: batch.commands.clone(),
            physical_transactions,
        })
    }

    fn dispatch(&mut self, prepared: PreparedBatch) -> Result<DriverToken> {
        let token = self.token();
        let mut last = Value::Null;
        for command in prepared.commands {
            match command {
                Command::ReadProperty { device, key } => {
                    last = self.read_property(device, &key)?;
                }
                Command::WriteProperty { device, key, value } => {
                    last = self.write_property(device, &key, &value)?;
                    self.emit_property(device, &key, last.clone());
                }
                Command::ApplyStateSet(set) => {
                    let mut result = BTreeMap::new();
                    for write in set.writes {
                        let value =
                            self.write_property(write.device, &write.property, &write.value)?;
                        self.emit_property(write.device, &write.property, value.clone());
                        result.insert(format!("{}:{}", write.device.0 .0, write.property), value);
                    }
                    last = Value::Map(result);
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } => {
                    let Some(capability) = self
                        .capabilities(device)
                        .into_iter()
                        .find(|candidate| candidate.id == capability)
                    else {
                        return Err(Error::new(ErrorCode::Unsupported, "unknown capability"));
                    };
                    last = match (capability.kind, request) {
                        (CapabilityKind::Autofocus, CapabilityRequest::Autofocus(request)) => {
                            self.apply_autofocus(request)
                        }
                        (CapabilityKind::StageMove, CapabilityRequest::StageMove(request)) => {
                            let z = request.target.get(&StageAxis::Z).ok_or_else(|| {
                                Error::new(ErrorCode::InvalidCommand, "missing Z target")
                            })?;
                            let value = self.write_property(device, "z", &Value::Position(*z))?;
                            self.update_focus_score();
                            value
                        }
                        (CapabilityKind::CameraCapture, request) => {
                            let request = match request {
                                CapabilityRequest::CameraCapture(request) => request,
                                CapabilityRequest::None => CameraCaptureRequest::default_frame(),
                                _ => {
                                    return Err(Error::new(
                                        ErrorCode::InvalidCommand,
                                        "CameraCapture expects CameraCaptureRequest",
                                    ))
                                }
                            };
                            self.update_focus_score();
                            let frame = self.capture_biological_frame(token, request);
                            let handle = frame.handle;
                            let width = frame.width;
                            let height = frame.height;
                            let pixel_format = frame.pixel_format.clone();
                            self.pending.push_back(DriverEvent::FrameReady(frame));
                            Value::Map(BTreeMap::from([
                                ("stream".into(), Value::I64(handle.stream.0 as i64)),
                                ("frame".into(), Value::I64(handle.frame.0 as i64)),
                                ("width".into(), Value::PixelCount(PixelCount::new(width))),
                                ("height".into(), Value::PixelCount(PixelCount::new(height))),
                                ("pixel_format".into(), Value::String(pixel_format)),
                                ("focus_score".into(), Value::F64(self.focus_score)),
                                (
                                    "z".into(),
                                    Value::Position(Position::from_micrometers(self.z_um)),
                                ),
                                (
                                    "scene".into(),
                                    Value::String("biological focus-scene frame".into()),
                                ),
                            ]))
                        }
                        _ => {
                            return Err(Error::new(
                                ErrorCode::Unsupported,
                                "unsupported composed autofocus invocation",
                            ))
                        }
                    };
                }
                Command::Arm(_) | Command::Start(_) | Command::Stop(_) => unreachable!(),
            }
        }
        self.pending
            .push_back(DriverEvent::TokenCompleted { token, value: last });
        Ok(token)
    }

    fn poll(&mut self) -> Vec<DriverEvent> {
        self.pending.drain(..).collect()
    }

    fn prepare_timing_plan(
        &mut self,
        plan: &TimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        self.validate_timing_plan(plan)?;
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Arm(plan.clone())],
            physical_transactions: vec![PhysicalTransaction {
                resource: Some(self.resource),
                description: "sim composed autofocus timing arm".into(),
                payload: self.timing_summary(plan, "arm"),
            }],
        })
    }

    fn start_timing_plan(
        &mut self,
        armed: &ArmedTimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        let applied = self.apply_timing_sequence_step(&armed.plan, true)?;
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Start(OperationId(0))],
            physical_transactions: vec![PhysicalTransaction {
                resource: Some(self.resource),
                description: "sim composed autofocus timing start".into(),
                payload: Value::Map(BTreeMap::from([
                    ("summary".into(), self.timing_summary(&armed.plan, "start")),
                    ("applied".into(), applied),
                ])),
            }],
        })
    }

    fn stop_timing_plan(
        &mut self,
        armed: &ArmedTimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        let applied = self.apply_timing_sequence_step(&armed.plan, false)?;
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Stop(OperationId(0))],
            physical_transactions: vec![PhysicalTransaction {
                resource: Some(self.resource),
                description: "sim composed autofocus timing stop".into(),
                payload: Value::Map(BTreeMap::from([
                    ("summary".into(), self.timing_summary(&armed.plan, "stop")),
                    ("applied".into(), applied),
                ])),
            }],
        })
    }
}

pub struct SceneFrame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

pub fn gel_scene(width: u32, height: u32, exposure_s: f64) -> SceneFrame {
    let mut pixels = vec![0u8; (width * height) as usize];
    let lanes = [0.18, 0.38, 0.58, 0.78];
    for y in 0..height {
        for x in 0..width {
            let xf = x as f64 / width as f64;
            let yf = y as f64 / height as f64;
            let mut signal = 0.02;
            for (li, lane) in lanes.iter().enumerate() {
                let lane_shape = gaussian(xf, *lane, 0.018);
                for band in 0..5 {
                    let by = 0.15 + band as f64 * 0.15 + li as f64 * 0.01;
                    signal += lane_shape * gaussian(yf, by, 0.012) * (0.4 + band as f64 * 0.16);
                }
            }
            pixels[(y * width + x) as usize] = (255.0 * (signal * exposure_s * 3.0).min(1.0)) as u8;
        }
    }
    SceneFrame {
        width,
        height,
        pixels,
    }
}

pub struct SimTransport {
    responses: VecDeque<Vec<u8>>,
}

impl SimTransport {
    pub fn new() -> Self {
        Self {
            responses: VecDeque::new(),
        }
    }
}

impl Default for SimTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl Transport for SimTransport {
    fn send(&mut self, bytes: &[u8]) -> Result<()> {
        let mut response = b"READY ".to_vec();
        response.extend_from_slice(bytes);
        self.responses.push_back(response);
        Ok(())
    }

    fn poll_recv(&mut self) -> Result<Option<Vec<u8>>> {
        Ok(self.responses.pop_front())
    }
}

fn light_properties() -> Vec<PropertySchema> {
    vec![
        sequenceable_property("enabled", "Enabled", ValueType::Bool, None, true),
        sequenceable_property("power", "Power", ValueType::Ratio, Some("percent"), true),
    ]
}

fn property(
    key: &str,
    display_name: &str,
    value_type: ValueType,
    unit: Option<&str>,
    writable: bool,
) -> PropertySchema {
    PropertySchema {
        key: key.to_string(),
        display_name: display_name.to_string(),
        value_type,
        unit: unit.map(|u| Unit(u.to_string())),
        range: None,
        increment: None,
        enum_values: Vec::new(),
        readable: true,
        writable,
        volatile: false,
        sequenceable: false,
        hardware_address: None,
    }
}

fn position_property(key: &str, display_name: &str, writable: bool, max_um: f64) -> PropertySchema {
    let mut schema = property(key, display_name, ValueType::Position, None, writable);
    schema.range = Some(Range {
        min: Value::Position(Position::from_micrometers(0.0)),
        max: Value::Position(Position::from_micrometers(max_um)),
    });
    schema.sequenceable = writable;
    schema
}

fn time_property(
    key: &str,
    display_name: &str,
    writable: bool,
    min_s: f64,
    max_s: f64,
) -> PropertySchema {
    let mut schema = property(
        key,
        display_name,
        ValueType::TimeInterval,
        Some("s"),
        writable,
    );
    schema.range = Some(Range {
        min: Value::TimeInterval(TimeInterval::from_seconds(min_s)),
        max: Value::TimeInterval(TimeInterval::from_seconds(max_s)),
    });
    schema.sequenceable = writable;
    schema
}

fn autofocus_properties() -> Vec<PropertySchema> {
    vec![
        sequenceable_property("enabled", "Enabled", ValueType::Bool, None, true),
        autofocus_mode_property(),
        property("status", "Status", ValueType::String, None, false),
        property("focus_score", "Focus score", ValueType::F64, None, false),
    ]
}

fn autofocus_mode_property() -> PropertySchema {
    let mut schema = sequenceable_property("mode", "Mode", ValueType::String, None, true);
    schema.enum_values = ["single_shot", "continuous", "hold", "stop"]
        .into_iter()
        .map(|mode| EnumValue {
            value: Value::String(mode.into()),
            label: mode.into(),
        })
        .collect();
    schema
}

fn sequenceable_property(
    key: &str,
    display_name: &str,
    value_type: ValueType,
    unit: Option<&str>,
    writable: bool,
) -> PropertySchema {
    let mut schema = property(key, display_name, value_type, unit, writable);
    schema.sequenceable = true;
    schema
}

fn autofocus_mode_name(mode: AutofocusMode) -> &'static str {
    match mode {
        AutofocusMode::SingleShot => "single_shot",
        AutofocusMode::Continuous => "continuous",
        AutofocusMode::Hold => "hold",
        AutofocusMode::Stop => "stop",
    }
}

fn parse_autofocus_mode(mode: &str) -> Result<AutofocusMode> {
    match mode.trim() {
        "single_shot" | "single-shot" | "single" => Ok(AutofocusMode::SingleShot),
        "continuous" => Ok(AutofocusMode::Continuous),
        "hold" => Ok(AutofocusMode::Hold),
        "stop" | "off" => Ok(AutofocusMode::Stop),
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            "autofocus mode must be single_shot, continuous, hold, or stop",
        )),
    }
}

fn image_encoding_name(encoding: ImageEncoding) -> &'static str {
    match encoding {
        ImageEncoding::Native | ImageEncoding::Mono8 => ImageEncoding::Mono8.property_value(),
        ImageEncoding::Mono16 => ImageEncoding::Mono16.property_value(),
        ImageEncoding::Rgb8 => ImageEncoding::Rgb8.property_value(),
        ImageEncoding::Bgr8 => ImageEncoding::Bgr8.property_value(),
        ImageEncoding::Raw8 => ImageEncoding::Raw8.property_value(),
        ImageEncoding::Raw16 => ImageEncoding::Raw16.property_value(),
    }
}

fn capability(id: u64, device: DeviceId, kind: CapabilityKind) -> CapabilityDescriptor {
    CapabilityDescriptor::new(CapabilityId(id), device, kind, ValueType::Map)
}

fn gaussian(x: f64, mu: f64, sigma: f64) -> f64 {
    let z = (x - mu) / sigma;
    (-0.5 * z * z).exp()
}
