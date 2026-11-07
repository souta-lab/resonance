//! Resonance control API: HTTP/JSON, WebSocket events, static SPA hosting.

pub mod error;
pub mod routes;
pub mod server;
pub mod state;

pub use error::{ok, ApiError, ApiResult};
pub use routes::{
    api_router, capture_preset, mark_engine_ready, meters_of, revision_of, spawn_meter_pump,
    swap_of, update_host,
};
pub use server::serve;
pub use state::{
    apply_bands, apply_params, AppState, AutoloadRule, Config, DeviceInfo, DevicesInfo,
    HostSnapshot, PipelineManager, PipelineState, PipewireInfo, PresetStore, ServerEvent,
    SlotHandle, StreamInfo, VirtualNodes,
};
