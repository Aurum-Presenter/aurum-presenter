//! Presenting a set: the control surface, the screens it drives, and the wires between them.

pub mod audience;
pub mod background;
pub mod control;
pub mod displays;
pub mod join;
pub mod pairing;
pub mod slide;
pub mod stage;
pub mod store;
pub mod theme;
pub mod transport;

pub use audience::AudiencePage;
pub use control::ControlPage;
pub use join::JoinPage;
pub use slide::{AudienceSlide, StageSlide};
pub use stage::StagePage;
pub use store::{Sessions, take_snapshot};
pub use transport::{ControlTransport, OutputTransport};
