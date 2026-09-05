//! disc0-gui — native findings window on Forge UI.
//!
//! Exposed as a library so the windowed binary and the headless renderer
//! render the SAME view code. A screenshot path that renders something other
//! than what ships is worse than no screenshot path.
pub mod app;
pub mod theme;
pub mod view;
