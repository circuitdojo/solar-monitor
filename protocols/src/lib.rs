//! Protocol drivers and registry.
//!
//! Layered as transport (shared bus actors) + protocol families (e.g.
//! `luxpower`) + per-model definitions (`luxpower::models`). New LuxPower-map
//! models need only a `ModelDef`; other vendors implement
//! `core::DeviceProtocol` (and `SettingsAccess` if configurable) and get
//! registered here.

use std::sync::Arc;

use solar_monitor_core as core;

pub mod luxpower;
pub mod transport;

pub fn registered() -> Vec<&'static str> {
    vec![luxpower::models::EG4_6000XP.protocol_name]
}

pub fn create_registry() -> core::ProtocolRegistry {
    let mut reg = core::ProtocolRegistry::new();
    reg.register_protocol(Arc::new(luxpower::LuxPowerProtocol {
        model: &luxpower::models::EG4_6000XP,
    }));
    reg
}
