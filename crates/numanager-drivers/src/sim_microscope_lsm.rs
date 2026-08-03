//! Composed brightfield and laser-scanning microscope simulator.
//!
//! This driver exposes the existing brightfield microscope simulator and the LSM
//! simulator as one runtime driver. LSM requests inherit the current XY stage,
//! Z focus, objective-derived sample pixel size, and lamp power from the
//! brightfield scene before they are dispatched.

use std::collections::HashMap;

use numanager_core::*;

use crate::sim_lsm::SimLsmDriver;
use crate::sim_microscope::{SimMicroscopeConfig, SimMicroscopeDriver};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum InnerDriver {
    Microscope,
    Lsm,
}

pub struct SimMicroscopeLsmDriver {
    id: DriverId,
    microscope: SimMicroscopeDriver,
    lsm: SimLsmDriver,
    next_token: u64,
    token_map: HashMap<(InnerDriver, DriverToken), DriverToken>,
    reverse_tokens: HashMap<DriverToken, (InnerDriver, DriverToken)>,
}

impl SimMicroscopeLsmDriver {
    pub fn new(id: DriverId, config: SimMicroscopeConfig) -> Self {
        let sample = config.sample_config();
        Self {
            id,
            microscope: SimMicroscopeDriver::new(id, config),
            lsm: SimLsmDriver::simulated_with_sample(id, sample),
            next_token: 1,
            token_map: HashMap::new(),
            reverse_tokens: HashMap::new(),
        }
    }

    pub fn simulated(id: DriverId) -> Self {
        Self::new(id, SimMicroscopeConfig::default())
    }

    fn next_token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn route(&self, command: &Command) -> Result<InnerDriver> {
        let devices = command.target_devices();
        let mut route = None;
        for device in devices {
            let next = if self.is_lsm_device(device) {
                InnerDriver::Lsm
            } else if self.is_microscope_device(device) {
                InnerDriver::Microscope
            } else {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    format!("unknown composed simulator device {:?}", device),
                ));
            };
            if let Some(existing) = route {
                if existing != next {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "composed simulator command cannot span brightfield and LSM inner devices",
                    ));
                }
            }
            route = Some(next);
        }
        route.ok_or_else(|| Error::new(ErrorCode::InvalidCommand, "command has no target device"))
    }

    fn is_lsm_device(&self, device: DeviceId) -> bool {
        self.lsm
            .descriptors()
            .into_iter()
            .any(|descriptor| descriptor.id == device)
    }

    fn is_microscope_device(&self, device: DeviceId) -> bool {
        self.microscope
            .descriptors()
            .into_iter()
            .any(|descriptor| descriptor.id == device)
    }

    fn prepare_inner(
        driver: &mut dyn Driver,
        command_id: CommandId,
        command: Command,
    ) -> Result<PreparedBatch> {
        driver.prepare(&CommandBatch {
            id: command_id,
            commands: vec![command],
        })
    }

    fn dispatch_inner(
        driver: &mut dyn Driver,
        command_id: CommandId,
        command: Command,
    ) -> Result<DriverToken> {
        driver.dispatch(PreparedBatch {
            id: command_id,
            commands: vec![command],
            physical_transactions: Vec::new(),
        })
    }

    fn track_token(&mut self, source: InnerDriver, inner: DriverToken, outer: DriverToken) {
        self.token_map.insert((source, inner), outer);
        self.reverse_tokens.insert(outer, (source, inner));
    }

    fn remap_events(&mut self, source: InnerDriver, events: Vec<DriverEvent>) -> Vec<DriverEvent> {
        events
            .into_iter()
            .map(|event| match event {
                DriverEvent::TokenCompleted { token, value } => {
                    let outer = self.token_map.remove(&(source, token)).unwrap_or(token);
                    self.reverse_tokens.remove(&outer);
                    DriverEvent::TokenCompleted {
                        token: outer,
                        value,
                    }
                }
                DriverEvent::TokenProgress { token, progress } => {
                    let outer = self
                        .token_map
                        .get(&(source, token))
                        .copied()
                        .unwrap_or(token);
                    DriverEvent::TokenProgress {
                        token: outer,
                        progress,
                    }
                }
                DriverEvent::TokenFailed { token, report } => {
                    let outer = self.token_map.remove(&(source, token)).unwrap_or(token);
                    self.reverse_tokens.remove(&outer);
                    DriverEvent::TokenFailed {
                        token: outer,
                        report,
                    }
                }
                other => other,
            })
            .collect()
    }

    fn inject_scene(&self, request: &mut CapabilityRequest) {
        let scene = self.microscope.runtime_scene_snapshot();
        let target = match request {
            CapabilityRequest::ConfocalImageCapture(request) => Some(&mut request.scan),
            CapabilityRequest::ConfocalImageStream(request) => Some(&mut request.scan),
            CapabilityRequest::ScanSignalStream(request) => Some(&mut request.timing),
            _ => None,
        };
        if let Some(map) = target {
            map.insert(
                "stage_x".into(),
                Value::Position(Position::from_micrometers(scene.x_um)),
            );
            map.insert(
                "stage_y".into(),
                Value::Position(Position::from_micrometers(scene.y_um)),
            );
            map.insert(
                "stage_z".into(),
                Value::Position(Position::from_micrometers(scene.z_um)),
            );
            map.insert(
                "pixel_size_um".into(),
                Value::F64(scene.sample_pixel_size_um),
            );
            map.insert(
                "laser_power".into(),
                Value::Ratio(Ratio::from_fraction(scene.lamp_power_fraction)),
            );
            map.insert(
                "laser_gate_enabled".into(),
                Value::Bool(scene.laser_gate_enabled),
            );
            map.insert("magnification".into(), Value::F64(scene.magnification));
            map.insert(
                "numerical_aperture".into(),
                Value::NumericalAperture(NumericalAperture::new(scene.numerical_aperture)),
            );
        }
    }
}

impl Driver for SimMicroscopeLsmDriver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        let mut descriptors = self.microscope.descriptors();
        descriptors.extend(self.lsm.descriptors());
        descriptors
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        let mut resources = self.microscope.resources();
        resources.extend(self.lsm.resources());
        resources
    }

    fn graph(&self) -> DeviceGraph {
        let mut graph = DeviceGraph::default();
        for resource in self.resources() {
            let _ = graph.insert_node(GraphNode {
                id: resource.id.0,
                kind: NodeKind::Resource,
                label: resource.label,
            });
        }
        for device in self.descriptors() {
            let _ = graph.insert_node(GraphNode {
                id: device.id.0,
                kind: NodeKind::Device,
                label: device.label,
            });
        }
        let descriptors = self.descriptors();
        if let Some(lsm) = descriptors.iter().find(|device| device.has_kind("lsm")) {
            if let Some(xy) = descriptors
                .iter()
                .find(|device| device.has_kind("stage.xy"))
            {
                let _ = graph.insert_device_dependency(xy.id.0, lsm.id.0, Role::XYStage);
            }
            if let Some(z) = descriptors.iter().find(|device| device.has_kind("stage.z")) {
                let _ = graph.insert_device_dependency(z.id.0, lsm.id.0, Role::ZStage);
            }
            if let Some(camera) = descriptors
                .iter()
                .find(|device| device.has_kind("camera") && !device.has_kind("lsm"))
            {
                let _ = graph.insert_device_dependency(camera.id.0, lsm.id.0, Role::Camera);
            }
            if let Some(turret) = descriptors
                .iter()
                .find(|device| device.has_kind("objective.turret"))
            {
                let _ = graph.insert_device_dependency(
                    turret.id.0,
                    lsm.id.0,
                    Role::Custom("objective".into()),
                );
            }
        }
        graph
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        if self.is_lsm_device(device) {
            self.lsm.capabilities(device)
        } else if self.is_microscope_device(device) {
            self.microscope.capabilities(device)
        } else {
            Vec::new()
        }
    }

    fn prepare(&mut self, batch: &CommandBatch) -> Result<PreparedBatch> {
        let mut physical_transactions = Vec::new();
        for command in &batch.commands {
            let mut command = command.clone();
            match self.route(&command)? {
                InnerDriver::Microscope => {
                    let prepared = Self::prepare_inner(&mut self.microscope, batch.id, command)?;
                    physical_transactions.extend(prepared.physical_transactions);
                }
                InnerDriver::Lsm => {
                    if let Command::Invoke { request, .. } = &mut command {
                        self.inject_scene(request);
                    }
                    let prepared = Self::prepare_inner(&mut self.lsm, batch.id, command)?;
                    physical_transactions.extend(prepared.physical_transactions);
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
        let outer = self.next_token();
        for command in prepared.commands {
            let mut command = command;
            let source = self.route(&command)?;
            if source == InnerDriver::Lsm {
                if let Command::Invoke { request, .. } = &mut command {
                    self.inject_scene(request);
                }
            }
            let inner = match source {
                InnerDriver::Microscope => {
                    Self::dispatch_inner(&mut self.microscope, prepared.id, command)?
                }
                InnerDriver::Lsm => Self::dispatch_inner(&mut self.lsm, prepared.id, command)?,
            };
            self.track_token(source, inner, outer);
        }
        Ok(outer)
    }

    fn poll(&mut self) -> Vec<DriverEvent> {
        let microscope_events = self.microscope.poll();
        let mut events = self.remap_events(InnerDriver::Microscope, microscope_events);
        let lsm_events = self.lsm.poll();
        events.extend(self.remap_events(InnerDriver::Lsm, lsm_events));
        events
    }

    fn cancel(&mut self, token: DriverToken) -> CancelResult {
        let Some((source, inner)) = self.reverse_tokens.remove(&token) else {
            return CancelResult::Unsupported;
        };
        self.token_map.remove(&(source, inner));
        match source {
            InnerDriver::Microscope => self.microscope.cancel(inner),
            InnerDriver::Lsm => self.lsm.cancel(inner),
        }
    }
}
