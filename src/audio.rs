use crate::{
    dsp::{MAX_FRAMES, Meters, Processor},
    model::{Availability, Rack},
    storage::Ports,
};
use jack::{
    AudioIn, AudioOut, Client, ClientOptions, Control, NotificationHandler, Port, PortFlags,
    ProcessHandler, ProcessScope,
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
pub struct Command {
    pub rack: Rack,
    pub sequence: u64,
}
pub struct Shared {
    /// Low eight bits are input/output availability. Upper bits are a graph
    /// generation, allowing off-thread inspection to publish with one CAS.
    graph: AtomicU64,
    pub rate: AtomicU32,
    pub xruns: AtomicU64,
    pub cycles: AtomicU64,
    pub applied: AtomicU64,
    pub frames: AtomicU32,
    pub server_down: AtomicBool,
    pub panic: AtomicU8,
    gate: AtomicBool,
    gated: AtomicBool,
    peaks: [AtomicU32; 4],
    faults: AtomicU8,
    missing: AtomicU8,
    pub buffer_fault: AtomicBool,
    pub rate_fault: AtomicBool,
}
impl Shared {
    pub fn new(rate: u32) -> Self {
        Self {
            graph: AtomicU64::new(0),
            rate: AtomicU32::new(rate),
            xruns: AtomicU64::new(0),
            cycles: AtomicU64::new(0),
            applied: AtomicU64::new(0),
            frames: AtomicU32::new(0),
            server_down: AtomicBool::new(false),
            panic: AtomicU8::new(0),
            gate: AtomicBool::new(false),
            gated: AtomicBool::new(false),
            peaks: std::array::from_fn(|_| AtomicU32::new(0)),
            faults: AtomicU8::new(0),
            missing: AtomicU8::new(0),
            buffer_fault: AtomicBool::new(false),
            rate_fault: AtomicBool::new(false),
        }
    }
    pub fn audit_generation(&self) -> u64 {
        self.graph.load(Ordering::Acquire)
    }
    pub fn publish_availability(&self, generation: u64, available: Availability) -> bool {
        let mask = u64::from(available.inputs & 15) | (u64::from(available.outputs & 15) << 4);
        self.graph
            .compare_exchange(
                generation,
                (generation & !255) | mask,
                Ordering::AcqRel,
                Ordering::Relaxed,
            )
            .is_ok()
    }
    pub fn availability(&self) -> Availability {
        let v = self.graph.load(Ordering::Acquire) as u8;
        Availability {
            inputs: v & 15,
            outputs: v >> 4,
        }
    }
    pub fn meters(&self) -> Meters {
        Meters {
            input: std::array::from_fn(|i| {
                f32::from_bits(self.peaks[i].swap(0, Ordering::Relaxed))
            }),
            output: std::array::from_fn(|i| {
                f32::from_bits(self.peaks[i + 2].swap(0, Ordering::Relaxed))
            }),
            faults: self.faults.load(Ordering::Relaxed),
            missing: self.missing.load(Ordering::Relaxed),
            buffer_fault: self.buffer_fault.load(Ordering::Relaxed),
        }
    }
    fn publish(&self, meters: Meters) {
        for i in 0..2 {
            self.peaks[i].fetch_max(meters.input[i].to_bits(), Ordering::Relaxed);
            self.peaks[i + 2].fetch_max(meters.output[i].to_bits(), Ordering::Relaxed);
        }
        self.faults.store(meters.faults, Ordering::Relaxed);
        self.missing.store(meters.missing, Ordering::Relaxed);
        self.buffer_fault
            .store(meters.buffer_fault, Ordering::Relaxed);
    }
}
pub struct Notifications {
    shared: Arc<Shared>,
    owned: [usize; 8],
}
impl NotificationHandler for Notifications {
    unsafe fn shutdown(&mut self, _: jack::ClientStatus, _: &str) {
        self.shared.server_down.store(true, Ordering::Release);
        self.shared.graph.fetch_and(!255, Ordering::Release);
    }
    fn sample_rate(&mut self, _: &Client, rate: u32) -> Control {
        self.shared.rate.store(rate, Ordering::Release);
        Control::Continue
    }
    fn xrun(&mut self, _: &Client) -> Control {
        self.shared.xruns.fetch_add(1, Ordering::Relaxed);
        Control::Continue
    }
    fn ports_connected(&mut self, client: &Client, a: u32, b: u32, _: bool) {
        let a = client.port_by_id(a).map_or(0, |p| p.raw() as usize);
        let b = client.port_by_id(b).map_or(0, |p| p.raw() as usize);
        let mut mask = 0;
        for (i, port) in self.owned.into_iter().enumerate() {
            if port == a || port == b {
                mask |= 1 << i;
            }
        }
        if mask != 0 {
            self.shared.graph.fetch_add(256, Ordering::AcqRel);
            self.shared.graph.fetch_and(!mask, Ordering::Release);
        }
    }
}
pub struct Callback {
    inputs: [Port<AudioIn>; 4],
    outputs: [Port<AudioOut>; 4],
    pub core: CallbackCore,
}
/// Hardware-free callback body, including bounded command intake and safety
/// gating, shared by the JACK adapter and allocation/recovery tests.
pub struct CallbackCore {
    processor: Processor,
    receiver: rtrb::Consumer<Command>,
    pub shared: Arc<Shared>,
    rate: u32,
}
impl CallbackCore {
    pub fn new(
        rate: u32,
        rack: Rack,
        receiver: rtrb::Consumer<Command>,
        shared: Arc<Shared>,
    ) -> Result<Self, String> {
        Ok(Self {
            processor: Processor::new(rate, rack)?,
            receiver,
            shared,
            rate,
        })
    }
    pub fn process(&mut self, inputs: [&[f32]; 4], mut outputs: [&mut [f32]; 4], frames: usize) {
        for _ in 0..8 {
            if let Ok(command) = self.receiver.pop() {
                self.processor.apply(command.rack);
                self.shared
                    .applied
                    .store(command.sequence, Ordering::Release);
            } else {
                break;
            }
        }
        self.processor
            .panic(self.shared.panic.swap(0, Ordering::AcqRel));
        let rate_fault = self.shared.rate.load(Ordering::Acquire) != self.rate;
        self.shared.rate_fault.store(rate_fault, Ordering::Release);
        self.shared.frames.store(frames as u32, Ordering::Relaxed);
        if rate_fault
            || self.shared.gate.load(Ordering::Acquire)
            || self.shared.server_down.load(Ordering::Acquire)
        {
            for output in &mut outputs {
                output.fill(0.0);
            }
            self.shared.gated.store(true, Ordering::Release);
            return;
        }
        self.shared.gated.store(false, Ordering::Release);
        let meters = self
            .processor
            .process(inputs, outputs, frames, self.shared.availability());
        self.shared.publish(meters);
        self.shared.cycles.fetch_add(1, Ordering::Relaxed);
    }
}
impl ProcessHandler for Callback {
    fn process(&mut self, _: &Client, scope: &ProcessScope) -> Control {
        let frames = scope.n_frames() as usize;
        // JACK owns distinct registered buffers for the duration of this call.
        // Check null before constructing slices (the crate's convenience
        // as_slice methods assume JACK never returns a missing buffer).
        let input_ptrs = self
            .inputs
            .each_ref()
            .map(|p| unsafe { p.buffer(scope.n_frames()).cast::<f32>() });
        let output_ptrs = self
            .outputs
            .each_ref()
            .map(|p| unsafe { p.buffer(scope.n_frames()).cast::<f32>() });
        if frames == 0 {
            return Control::Continue;
        }
        if frames > MAX_FRAMES {
            for ptr in output_ptrs {
                if !ptr.is_null() {
                    unsafe {
                        std::slice::from_raw_parts_mut(ptr, frames).fill(0.0);
                    }
                }
            }
            self.core.shared.buffer_fault.store(true, Ordering::Release);
            self.core.processor.panic(3);
            return Control::Continue;
        }
        let inputs = input_ptrs.map(|p| {
            if p.is_null() {
                &[][..]
            } else {
                unsafe { std::slice::from_raw_parts(p, frames) }
            }
        });
        let outputs = output_ptrs.map(|p| {
            if p.is_null() {
                &mut [][..]
            } else {
                unsafe { std::slice::from_raw_parts_mut(p, frames) }
            }
        });
        self.core.process(inputs, outputs, frames);
        Control::Continue
    }
}
pub struct Audio {
    active: Option<jack::AsyncClient<Notifications, Callback>>,
    sender: rtrb::Producer<Command>,
    pub shared: Arc<Shared>,
    pub ports: Ports,
    pub sequence: u64,
}
impl Audio {
    pub fn start(rack: Rack, ports: Ports) -> Result<Self, String> {
        rack.validate(Availability::ALL)?;
        ports.validate()?;
        jack::jack_sys::library().map_err(|e| e.to_string())?;
        jack::set_logger(jack::LoggerType::None);
        let (client, _) = Client::new(
            "fx",
            ClientOptions::NO_START_SERVER | ClientOptions::USE_EXACT_NAME,
        )
        .map_err(|e| e.to_string())?;
        let rate = client.sample_rate();
        if client.buffer_size() as usize > MAX_FRAMES {
            return Err("JACK period exceeds 8192 frames".into());
        }
        let mut ins = Vec::new();
        let mut outs = Vec::new();
        for i in 1..=4 {
            ins.push(
                client
                    .register_port(&format!("in_{i}"), AudioIn::default())
                    .map_err(|e| e.to_string())?,
            );
            outs.push(
                client
                    .register_port(&format!("out_{i}"), AudioOut::default())
                    .map_err(|e| e.to_string())?,
            );
        }
        let inputs: [Port<AudioIn>; 4] = ins.try_into().map_err(|_| "Input registration")?;
        let outputs: [Port<AudioOut>; 4] = outs.try_into().map_err(|_| "Output registration")?;
        let owned = std::array::from_fn(|i| {
            if i < 4 {
                inputs[i].raw() as usize
            } else {
                outputs[i - 4].raw() as usize
            }
        });
        let shared = Arc::new(Shared::new(rate));
        let (sender, receiver) = rtrb::RingBuffer::new(8);
        let core = CallbackCore::new(rate, rack, receiver, shared.clone())?;
        let callback = Callback {
            inputs,
            outputs,
            core,
        };
        let active = client
            .activate_async(
                Notifications {
                    shared: shared.clone(),
                    owned,
                },
                callback,
            )
            .map_err(|e| e.to_string())?;
        let audio = Self {
            active: Some(active),
            sender,
            shared,
            ports,
            sequence: 0,
        };
        // Missing saved ports leave their slots silent and visible in the UI.
        for (from, to) in connections(&audio.ports) {
            if audio.client().port_by_name(&from).is_some()
                && audio.client().port_by_name(&to).is_some()
            {
                let _ = audio.client().connect_ports_by_name(&from, &to);
            }
        }
        audio.poll();
        Ok(audio)
    }
    fn client(&self) -> &Client {
        self.active
            .as_ref()
            .expect("active audio owner")
            .as_client()
    }
    pub fn cpu_load(&self) -> f32 {
        if self.shared.server_down.load(Ordering::Acquire) {
            0.0
        } else {
            self.client().cpu_load()
        }
    }
    pub fn submit(&mut self, rack: Rack) -> Result<(), String> {
        rack.validate(self.shared.availability())?;
        let sequence = self.sequence + 1;
        self.sender
            .push(Command { rack, sequence })
            .map_err(|_| "Audio busy; change not applied")?;
        self.sequence = sequence;
        Ok(())
    }
    pub fn submit_controls(&mut self, rack: Rack) -> Result<(), String> {
        rack.validate(Availability::ALL)?;
        let sequence = self.sequence + 1;
        self.sender
            .push(Command { rack, sequence })
            .map_err(|_| "Audio busy; change not applied")?;
        self.sequence = sequence;
        Ok(())
    }
    pub fn pending(&self) -> bool {
        self.shared.applied.load(Ordering::Acquire) < self.sequence
    }
    pub fn poll(&self) {
        if self.shared.server_down.load(Ordering::Acquire) {
            return;
        }
        let old = self.shared.graph.load(Ordering::Acquire);
        let mut mask = 0u64;
        for i in 0..8 {
            let target = if i < 4 {
                &self.ports.inputs[i]
            } else {
                &self.ports.outputs[i - 4]
            };
            if let Some(target) = target {
                let own = format!("fx:{}_{}", if i < 4 { "in" } else { "out" }, i % 4 + 1);
                if let Some(port) = self.client().port_by_name(&own)
                    && port.connected_count() == Ok(1)
                    && port.is_connected_to(target) == Ok(true)
                {
                    mask |= 1 << i;
                }
            }
        }
        let _ = self.shared.graph.compare_exchange(
            old,
            (old & !255) | mask,
            Ordering::AcqRel,
            Ordering::Relaxed,
        );
    }
    pub fn choices(&self, input: bool) -> Vec<String> {
        if self.shared.server_down.load(Ordering::Acquire) {
            return Vec::new();
        }
        let mut ports = self.client().ports(
            None,
            Some("32 bit float mono audio"),
            if input {
                PortFlags::IS_OUTPUT
            } else {
                PortFlags::IS_INPUT
            },
        );
        ports.retain(|p| !p.starts_with("fx:"));
        ports.sort();
        ports
    }
    fn pause(&self) -> Result<(), String> {
        if self.shared.server_down.load(Ordering::Acquire) {
            return Err("JACK stopped; Retry audio".into());
        }
        self.shared.gated.store(false, Ordering::Release);
        self.shared.gate.store(true, Ordering::Release);
        let start = Instant::now();
        while !self.shared.gated.load(Ordering::Acquire) {
            if start.elapsed() > Duration::from_millis(150) {
                self.shared.gate.store(false, Ordering::Release);
                return Err("Audio did not acknowledge pause".into());
            }
            thread::sleep(Duration::from_millis(1));
        }
        Ok(())
    }
    /// Preflight all fields first. Pause owned returns during physical edits;
    /// restore exact old connections if connect or persistence fails.
    pub fn set_ports(
        &mut self,
        ports: Ports,
        rack: Rack,
        persist: impl FnOnce() -> Result<(), String>,
    ) -> Result<(), String> {
        ports.validate()?;
        rack.validate(ports.assigned())?;
        if self.shared.server_down.load(Ordering::Acquire) {
            return Err("JACK stopped; Retry audio".into());
        }
        for (i, side) in [&ports.inputs, &ports.outputs].into_iter().enumerate() {
            for name in side.iter().flatten() {
                let port = self
                    .client()
                    .port_by_name(name)
                    .ok_or_else(|| format!("Missing: {name}"))?;
                let flag = if i == 0 {
                    PortFlags::IS_OUTPUT
                } else {
                    PortFlags::IS_INPUT
                };
                if !port.flags().contains(flag)
                    || port.port_type().map_err(|e| e.to_string())? != "32 bit float mono audio"
                {
                    return Err("Wrong JACK port direction/type".into());
                }
            }
        }
        self.pause()?;
        let edit = edit_connections(self.client(), &self.ports, &ports, persist);
        if edit.rollback_failed {
            self.shared.graph.fetch_and(!255, Ordering::Release);
            return Err("Port restore failed; muted. Retry audio".into());
        }
        if edit.error.is_none() {
            self.ports = ports;
        }
        self.poll();
        self.shared.gate.store(false, Ordering::Release);
        edit.error.map_or(Ok(()), Err)
    }
}
impl Drop for Audio {
    fn drop(&mut self) {
        let _ = self.pause();
        if let Some(active) = self.active.take() {
            let _ = active.deactivate();
        }
        // Deactivation joins the callback before its engine/queue memory drops.
        // Closing this exact client removes only connections involving it.
    }
}
pub fn connections(ports: &Ports) -> Vec<(String, String)> {
    let mut result = Vec::new();
    for i in 0..4 {
        if let Some(input) = &ports.inputs[i] {
            result.push((input.clone(), format!("fx:in_{}", i + 1)));
        }
        if let Some(output) = &ports.outputs[i] {
            result.push((format!("fx:out_{}", i + 1), output.clone()));
        }
    }
    result
}

/// Off-thread graph boundary. The transaction is tested with failure injection;
/// the live adapter uses only exact connections involving fx-owned ports.
pub trait PortGraph {
    fn linked(&self, from: &str, to: &str) -> bool;
    fn connect(&self, from: &str, to: &str) -> Result<(), String>;
    fn disconnect(&self, from: &str, to: &str) -> Result<(), String>;
    fn unique(&self, owned: &str) -> bool;
}
impl PortGraph for Client {
    fn linked(&self, from: &str, to: &str) -> bool {
        self.port_by_name(from)
            .is_some_and(|p| p.is_connected_to(to) == Ok(true))
    }
    fn connect(&self, from: &str, to: &str) -> Result<(), String> {
        self.connect_ports_by_name(from, to)
            .map_err(|e| e.to_string())
    }
    fn disconnect(&self, from: &str, to: &str) -> Result<(), String> {
        self.disconnect_ports_by_name(from, to)
            .map_err(|e| e.to_string())
    }
    fn unique(&self, owned: &str) -> bool {
        self.port_by_name(owned)
            .is_some_and(|p| p.connected_count() == Ok(1))
    }
}
pub struct ConnectionEdit {
    pub error: Option<String>,
    pub rollback_failed: bool,
}
pub fn edit_connections(
    graph: &impl PortGraph,
    old: &Ports,
    new: &Ports,
    persist: impl FnOnce() -> Result<(), String>,
) -> ConnectionEdit {
    let old = connections(old);
    let new = connections(new);
    let mut removed = Vec::new();
    let mut added = Vec::new();
    let result = (|| {
        for (from, to) in &old {
            if !new.contains(&(from.clone(), to.clone())) && graph.linked(from, to) {
                graph.disconnect(from, to)?;
                removed.push((from.clone(), to.clone()));
            }
        }
        for (from, to) in &new {
            if !graph.linked(from, to) {
                graph.connect(from, to)?;
                added.push((from.clone(), to.clone()));
            }
        }
        for (from, to) in &new {
            let own = if from.starts_with("fx:") { from } else { to };
            if !graph.unique(own) {
                return Err("Extra connection; remove it in JACK".into());
            }
        }
        persist()
    })();
    let mut rollback_failed = false;
    if result.is_err() {
        for (from, to) in &added {
            rollback_failed |= graph.disconnect(from, to).is_err();
        }
        for (from, to) in &removed {
            rollback_failed |= graph.connect(from, to).is_err();
        }
    }
    ConnectionEdit {
        error: result.err(),
        rollback_failed,
    }
}
